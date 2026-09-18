// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agents;
mod catalog;
mod citations;
mod clients;
mod config;
mod db;
mod details;
mod engine;
mod integrations;
mod mcp;
mod models;
mod query;
mod secrets;
mod security;
mod server;
mod skills;
mod sources;
mod web_search;

use db::Database;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};

// The desktop UI talks to the gateway over HTTP/REST; Tauri IPC is only used for
// the operations that must run in-process (settings, credentials, files, skills).
struct AppSharedState {
    pub db: Database,
    pub port: u16,
}

#[tauri::command]
fn get_gateway_port(state: State<'_, AppSharedState>) -> u16 {
    state.port
}

#[tauri::command]
fn read_settings(state: State<'_, AppSharedState>) -> serde_json::Value {
    // Credentials are write-only: the UI receives a sentinel it echoes back unchanged.
    config::sanitize_config(state.db.get_all_config())
}

/// The trusted desktop UI needs the gateway token to call the protected HTTP
/// routes. It is handed over IPC (never over the network) and kept in memory.
#[tauri::command]
fn get_gateway_token(state: State<'_, AppSharedState>) -> String {
    state.db.get_config("mcp_auth_token").unwrap_or_default()
}

#[tauri::command]
fn save_settings(
    payload: serde_json::Value,
    state: State<'_, AppSharedState>,
) -> Result<(), String> {
    state
        .db
        .set_config_patch(&config::validate_patch(payload)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn choose_download_directory() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Choose PDF download directory")
            .pick_folder()
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())
}

/// A site the app can sign into with its own browser window.
///
/// Consensus and OpenEvidence sessions are used directly by the search sources
/// of the same name. The AI providers point at their **developer console**, not
/// their consumer chat app: signing in there keeps the console one click away
/// inside the app and records that the account is connected. API calls still use
/// the API key saved on the same card — a console session is not an API
/// credential, and the UI says so rather than implying otherwise.
struct LoginTarget {
    /// Page the window opens at.
    url: &'static str,
    title: &'static str,
    /// Session capture only fires once the window is on this host.
    host: &'static str,
    /// Paths that still count as "not signed in yet".
    login_paths: &'static [&'static str],
    /// Substring the cookie must contain before it is worth storing. Consensus
    /// hands out several cookies and only `__session=` identifies the account,
    /// so capturing any long cookie would save a useless value.
    cookie_needle: &'static str,
}

fn login_target(service: &str) -> Option<LoginTarget> {
    Some(match service {
        "consensus" => LoginTarget {
            url: "https://consensus.app/login",
            title: "Sign in to Consensus.app",
            host: "consensus.app",
            login_paths: &["/login", "/sign-in"],
            cookie_needle: "__session=",
        },
        "openevidence" => LoginTarget {
            url: "https://www.openevidence.com/",
            title: "Sign in to OpenEvidence",
            host: "openevidence.com",
            login_paths: &["/login"],
            cookie_needle: "",
        },
        "openai" => LoginTarget {
            url: "https://platform.openai.com/api-keys",
            title: "Sign in to the OpenAI platform",
            host: "platform.openai.com",
            login_paths: &["/login", "/auth"],
            cookie_needle: "",
        },
        "anthropic" => LoginTarget {
            url: "https://console.anthropic.com/settings/keys",
            title: "Sign in to the Anthropic console",
            host: "console.anthropic.com",
            login_paths: &["/login", "/oauth"],
            cookie_needle: "",
        },
        "gemini" => LoginTarget {
            url: "https://aistudio.google.com/apikey",
            title: "Sign in to Google AI Studio",
            host: "aistudio.google.com",
            login_paths: &["/signin"],
            cookie_needle: "",
        },
        "deepseek" => LoginTarget {
            url: "https://platform.deepseek.com/api_keys",
            title: "Sign in to the DeepSeek platform",
            host: "platform.deepseek.com",
            login_paths: &["/sign_in"],
            cookie_needle: "",
        },
        "perplexity" => LoginTarget {
            url: "https://www.perplexity.ai/account/api/keys",
            title: "Sign in to Perplexity",
            host: "perplexity.ai",
            login_paths: &["/login"],
            cookie_needle: "",
        },
        _ => return None,
    })
}

/// Config key each captured session is stored under. Every one of these is in
/// `config::SECRET_KEYS`, so it is never returned to the UI.
fn session_key(service: &str) -> Option<String> {
    login_target(service).map(|_| format!("{service}_session"))
}

#[tauri::command]
async fn open_service_login(app: AppHandle, service: String) -> Result<(), String> {
    use tauri::{Url, WebviewUrl, WebviewWindowBuilder};

    let target = login_target(&service).ok_or("Unsupported service")?;
    let window_label = format!("login_{service}");

    if let Some(existing) = app.get_webview_window(&window_label) {
        let _ = existing.close();
    }

    let parsed_url = Url::parse(target.url).map_err(|e| e.to_string())?;

    // The window reports back by navigating to a sentinel path, which the
    // navigation handler below intercepts and cancels. Consensus hands out a
    // Clerk token; everywhere else the session cookie is what identifies us.
    let script = format!(
        r#"
    (function() {{
      if (window.__sg_injected) return;
      window.__sg_injected = true;

      var HOST = {host};
      var LOGIN_PATHS = {login_paths};
      var COOKIE_NEEDLE = {cookie_needle};

      function onLoginPage() {{
        var path = window.location.pathname;
        return LOGIN_PATHS.some(function(prefix) {{ return path.indexOf(prefix) === 0; }});
      }}

      function checkSession() {{
        try {{
          if (window.location.hostname.indexOf(HOST) === -1 || onLoginPage()) return;
          if (window.Clerk && window.Clerk.session) {{
            window.Clerk.session.getToken().then(function(tok) {{
              if (tok) notify(tok, document.cookie);
            }}).catch(function() {{}});
          }}
          if (document.cookie && document.cookie.length > 20
              && document.cookie.indexOf(COOKIE_NEEDLE) !== -1) {{
            notify(null, document.cookie);
          }}
        }} catch (e) {{}}
      }}

      var reported = false;
      function notify(token, cookie) {{
        if (reported) return;
        reported = true;
        var payload = JSON.stringify({{ token: token || "", cookie: cookie || "" }});
        window.location.href = "https://" + window.location.hostname + "/__scholargate_session__?data=" + encodeURIComponent(payload);
      }}

      setInterval(checkSession, 1500);
    }})();
    "#,
        host = serde_json::to_string(target.host).unwrap_or_else(|_| "\"\"".into()),
        login_paths = serde_json::to_string(target.login_paths).unwrap_or_else(|_| "[]".into()),
        cookie_needle =
            serde_json::to_string(target.cookie_needle).unwrap_or_else(|_| "\"\"".into()),
    );

    let app_handle = app.clone();
    let service_name = service.clone();
    let close_label = window_label.clone();

    let builder = WebviewWindowBuilder::new(&app, &window_label, WebviewUrl::External(parsed_url))
        .title(target.title)
        .inner_size(960.0, 720.0)
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15")
        .initialization_script(&script)
        .on_navigation(move |nav_url| {
            if nav_url.path() == "/__scholargate_session__" {
                if let Some((_, encoded)) = nav_url.query_pairs().find(|(k, _)| k == "data") {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&encoded) {
                        let token = val.get("token").and_then(|v| v.as_str()).unwrap_or("");
                        let cookie = val.get("cookie").and_then(|v| v.as_str()).unwrap_or("");
                        let session_val = if !token.is_empty() {
                            token.to_string()
                        } else {
                            cookie.to_string()
                        };

                        if !session_val.is_empty() {
                            if let Some(key) = session_key(&service_name) {
                                let state = app_handle.state::<AppSharedState>();
                                let mut patch = std::collections::BTreeMap::new();
                                patch.insert(key, session_val);
                                if state.db.set_config_patch(&patch).is_ok() {
                                    let _ = app_handle.emit("settings-session-updated", &service_name);
                                }
                            }
                        }
                    }
                }
                if let Some(win) = app_handle.get_webview_window(&close_label) {
                    let _ = win.close();
                }
                return false;
            }
            true
        });

    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn save_service_session(
    service: String,
    session: String,
    state: State<'_, AppSharedState>,
) -> Result<(), String> {
    let key = session_key(&service).ok_or("Invalid service")?;
    let mut patch = std::collections::BTreeMap::new();
    patch.insert(key, session.trim().to_string());
    state.db.set_config_patch(&patch).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_service_session(service: String, state: State<'_, AppSharedState>) -> Result<(), String> {
    let key = session_key(&service).ok_or("Invalid service")?;
    let mut patch = std::collections::BTreeMap::new();
    patch.insert(key, String::new());
    state.db.set_config_patch(&patch).map_err(|e| e.to_string())
}

/// ---- AI client setup (MCP + skills) -------------------------------------
/// The gateway port and token live in the app process; the UI only names the
/// client it wants set up.
#[tauri::command]
fn list_ai_clients(state: State<'_, AppSharedState>) -> Vec<clients::ClientStatus> {
    clients::list(config::gateway_port(&state.db))
}

#[tauri::command]
fn setup_ai_client(
    client: String,
    action: String,
    state: State<'_, AppSharedState>,
) -> Result<clients::ClientStatus, String> {
    let token = state
        .db
        .get_config("mcp_auth_token")
        .filter(|token| !token.trim().is_empty());
    let configured_port = config::gateway_port(&state.db);
    match action.as_str() {
        "install_all" => clients::install_all(&client, configured_port, token),
        "install_mcp" => clients::install_mcp(&client, configured_port, token),
        "remove_mcp" => clients::remove_mcp(&client, configured_port),
        "install_skills" => clients::install_skills(&client, configured_port),
        "remove_skills" => clients::remove_skills(&client, configured_port),
        _ => Err("Unsupported action".to_string()),
    }
}

fn main() {
    let db = match Database::init() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to initialize SQLite: {}", e);
            return;
        }
    };

    let port = config::gateway_port(&db);

    // Start background Axum Localhost Agent Gateway
    let db_server = db.clone();
    tauri::async_runtime::spawn(async move {
        server::start_server(port, db_server).await;
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppSharedState { db, port })
        .setup(|app| {
            // Build System Tray Menu
            let quit_i = MenuItem::with_id(app, "quit", "Quit ScholarGate", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Open ScholarGate", true, None::<&str>)?;
            let status_i = MenuItem::with_id(
                app,
                "status",
                format!(
                    "Gateway: port {} — see status in the app",
                    app.state::<AppSharedState>().port
                ),
                false,
                None::<&str>,
            )?;

            let menu = Menu::with_items(app, &[&status_i, &show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("ScholarGate Desktop — Localhost Agent Gateway")
                .on_menu_event(|app: &AppHandle, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Do not close app; hide window to tray so localhost gateway remains running
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_gateway_port,
            get_gateway_token,
            read_settings,
            save_settings,
            choose_download_directory,
            skills::preview_skill,
            skills::install_skill,
            skills::list_skills,
            skills::remove_skill,
            skills::set_skill_enabled,
            integrations::read_mcp_config,
            integrations::edit_mcp_config,
            integrations::test_mcp_http,
            open_service_login,
            save_service_session,
            clear_service_session,
            list_ai_clients,
            setup_ai_client,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
