//! One-step setup for the AI clients installed on this machine.
//!
//! The app already knows how to edit an MCP client config (`integrations`) and
//! how to install a skill folder (`skills`). What it could not do was tell the
//! user *which* clients exist on this machine, whether ScholarGate is already
//! wired into them, and do both edits in one click. This module adds exactly
//! that: detection plus the two install actions, reusing the safe primitives so
//! backups, receipts and the "never touch what we did not install" rule apply
//! unchanged.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// The entry name this app writes into every client config.
pub const SERVER_NAME: &str = "scholargate";
/// The entry name written before the app was renamed. Installing removes it, so
/// a client set up by an older build is not left with two entries pointing at
/// the same gateway.
pub const LEGACY_SERVER_NAME: &str = "scholargateway";
/// Environment variable Codex reads the gateway token from. Codex only accepts a
/// variable name, never a literal token, so the secret stays out of the file.
pub const CODEX_TOKEN_ENV: &str = "SCHOLARGATE_TOKEN";

const BUNDLED_SKILLS: [(&str, &str); 3] = [
    ("paper-search", "builtin:paper-search"),
    ("paper-collect", "builtin:paper-collect"),
    ("research-resume", "builtin:research-resume"),
];

#[derive(Serialize, Clone)]
pub struct SkillState {
    pub name: String,
    pub source: String,
    /// A directory with this name exists in the client's skills folder.
    pub installed: bool,
    /// Installed by this app, so it can also be removed by this app.
    pub managed: bool,
    pub enabled: bool,
    pub up_to_date: bool,
}

#[derive(Serialize, Clone)]
pub struct ClientStatus {
    pub id: String,
    pub name: String,
    /// The client keeps state on this machine, so it is worth offering.
    pub detected: bool,
    pub mcp_path: String,
    /// "json" or "toml" — the UI explains that Codex is edited as TOML.
    pub mcp_format: String,
    /// A server entry pointing at this gateway is present in the config.
    pub mcp_installed: bool,
    /// That entry was written by this app and can be removed again from here.
    pub mcp_managed: bool,
    /// Name of the entry found, which may differ from ours if the user added it.
    pub mcp_entry: Option<String>,
    pub mcp_error: Option<String>,
    pub skills_path: Option<String>,
    pub skills: Vec<SkillState>,
    pub skills_error: Option<String>,
    /// Client-specific instruction the UI shows verbatim.
    pub note: String,
    /// Set right after an install that had to deal with the gateway token, since
    /// what the user must do next differs per client.
    pub token_note: Option<String>,
}

#[derive(Clone)]
struct Profile {
    id: &'static str,
    name: &'static str,
    /// Files or directories that indicate at least one client surface exists.
    markers: Vec<PathBuf>,
    mcp: PathBuf,
    toml: bool,
    skills: Option<PathBuf>,
    note: &'static str,
}

fn home() -> Option<PathBuf> {
    home_from(
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(PathBuf::from),
    )
}

fn home_from(home: Option<PathBuf>, user_profile: Option<PathBuf>) -> Option<PathBuf> {
    [home, user_profile]
        .into_iter()
        .flatten()
        .find(|path| path.is_absolute())
}

/// Deterministic Claude Desktop config path beneath a supplied home directory.
/// Keeping this free of process-wide environment variables also isolates tests.
fn claude_desktop_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/Claude")
    }
    #[cfg(target_os = "windows")]
    {
        home.join("AppData/Roaming/Claude")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home.join(".config/Claude")
    }
}

fn system_claude_desktop_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .map(|path| path.join("Claude"))
            .unwrap_or_else(|| claude_desktop_dir(home))
    }
    #[cfg(not(target_os = "windows"))]
    {
        claude_desktop_dir(home)
    }
}

fn profiles() -> Vec<Profile> {
    match home() {
        Some(home) => {
            let claude_dir = system_claude_desktop_dir(&home);
            profiles_in_with_claude_dir(&home, claude_dir)
        }
        None => Vec::new(),
    }
}

/// Split out from `profiles` so the tests can point a full set of clients at a
/// throwaway home directory instead of touching the real one.
fn opencode_marker(home: &Path) -> PathBuf {
    let config_dir = home.join(".config/opencode");
    if config_dir.is_dir() {
        config_dir
    } else {
        home.join(".opencode")
    }
}

fn opencode_config(home: &Path) -> PathBuf {
    let jsonc = home.join(".config/opencode/opencode.jsonc");
    if jsonc.exists() {
        return jsonc;
    }
    let json = home.join(".config/opencode/opencode.json");
    if json.exists() {
        return json;
    }
    let dot_json = home.join(".opencode/config.json");
    if dot_json.exists() {
        return dot_json;
    }
    home.join(".config/opencode/opencode.jsonc")
}

fn opencode_skills(home: &Path) -> PathBuf {
    let config_skills = home.join(".config/opencode/skills");
    if config_skills.is_dir() {
        config_skills
    } else if home.join(".opencode").is_dir() {
        home.join(".opencode/skills")
    } else {
        home.join(".config/opencode/skills")
    }
}

fn antigravity_config(home: &Path) -> PathBuf {
    let standard = home.join(".gemini/config/mcp_config.json");
    if standard.exists() {
        return standard;
    }
    let legacy = home.join(".gemini/antigravity/mcp_config.json");
    if legacy.exists() {
        return legacy;
    }
    standard
}

fn profiles_in(home: &Path) -> Vec<Profile> {
    profiles_in_with_claude_dir(home, claude_desktop_dir(home))
}

fn profiles_in_with_claude_dir(home: &Path, claude_dir: PathBuf) -> Vec<Profile> {
    let gemini = home.join(".gemini");
    vec![
        Profile {
            id: "antigravity",
            name: "Antigravity",
            markers: [
                "config",
                "antigravity",
                "antigravity-cli",
                "antigravity-ide",
            ]
            .into_iter()
            .map(|name| gemini.join(name))
            .collect(),
            mcp: antigravity_config(home),
            toml: false,
            skills: Some(gemini.join("config/skills")),
            note: "Reload MCP servers and skills in Antigravity after installing. Antigravity 2.0, IDE and CLI share this configuration.",
        },
        Profile {
            id: "claude_desktop",
            name: "Claude Desktop",
            markers: vec![claude_dir.clone()],
            mcp: claude_dir.join("claude_desktop_config.json"),
            toml: false,
            skills: None,
            note: "Quit and reopen Claude Desktop after installing. Claude Desktop does not load skill folders.",
        },
        Profile {
            id: "codex",
            name: "Codex",
            markers: vec![home.join(".codex")],
            mcp: home.join(".codex/config.toml"),
            toml: true,
            skills: Some(home.join(".codex/skills")),
            note: "Codex is configured in TOML. Start a new Codex session after installing.",
        },
        Profile {
            id: "opencode",
            name: "OpenCode",
            markers: vec![opencode_marker(home)],
            mcp: opencode_config(home),
            toml: false,
            skills: Some(opencode_skills(home)),
            note: "Restart OpenCode or run `opencode mcp list` after installing.",
        },
    ]
}

fn profile(id: &str) -> Result<Profile, String> {
    find(&profiles(), id)
}

fn find(profiles: &[Profile], id: &str) -> Result<Profile, String> {
    profiles
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| "Unknown AI client".to_string())
}

fn detected(profile: &Profile) -> bool {
    profile.markers.iter().any(|marker| marker.exists())
}

/// Resolve symlinks so the caller edits the real file. The user's Antigravity
/// config is a symlink into another directory, and the config editor refuses to
/// replace a symlink — pointing it at the target is the fix it asks for, and the
/// resolved path is what the UI shows.
fn real_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        // The file itself may not exist yet; resolving the parent is enough to
        // avoid writing through a symlinked directory.
        match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => fs::canonicalize(parent)
                .map(|parent| parent.join(name))
                .unwrap_or_else(|_| path.to_path_buf()),
            _ => path.to_path_buf(),
        }
    })
}

/// The MCP endpoint of this gateway, as clients must address it.
pub fn gateway_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

fn gateway_sse_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/sse")
}

fn points_at_gateway(definition: &Value, port: u16) -> bool {
    [("url", "mcp"), ("serverUrl", "sse")]
        .iter()
        .any(|(field, path)| {
            definition
                .get(*field)
                .and_then(Value::as_str)
                .is_some_and(|url| {
                    let url = url.trim_end_matches('/');
                    url == format!("http://127.0.0.1:{port}/{path}")
                        || url == format!("http://localhost:{port}/{path}")
                })
        })
}

// ---------------------------------------------------------------------------
// TOML clients (Codex)
// ---------------------------------------------------------------------------

fn read_toml(path: &Path) -> Result<toml_edit::DocumentMut, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("{} is not valid TOML: {error}", path.display()))
}

/// Replace the file in place, keeping a timestamped backup of the previous
/// contents. `toml_edit` preserves comments and formatting, so everything the
/// user wrote outside our own table survives untouched.
fn write_toml(path: &Path, document: &toml_edit::DocumentMut) -> Result<Option<String>, String> {
    let backup = if path.exists() {
        let backup = path.with_file_name(format!(
            "{}.{}.bak",
            path.file_name().unwrap().to_string_lossy(),
            uuid::Uuid::new_v4()
        ));
        fs::copy(path, &backup).map_err(|error| error.to_string())?;
        Some(backup.to_string_lossy().into_owned())
    } else {
        None
    };
    let temp = path.with_file_name(format!(".sg-config-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    file.write_all(document.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    fs::rename(&temp, path)
        .map_err(|error| format!("{error}; the temporary file was kept at {}", temp.display()))?;
    Ok(backup)
}

fn toml_status(document: &toml_edit::DocumentMut, port: u16) -> (Option<String>, bool) {
    let Some(servers) = document
        .get("mcp_servers")
        .and_then(|item| item.as_table_like())
    else {
        return (None, false);
    };
    for (name, item) in servers.iter() {
        let url = item
            .as_table_like()
            .and_then(|table| table.get("url"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        if url == gateway_url(port) || url == format!("http://localhost:{port}/mcp") {
            return (Some(name.to_string()), name == SERVER_NAME);
        }
    }
    (None, false)
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

fn skill_states(root: &Path) -> (Vec<SkillState>, Option<String>) {
    let managed_result = crate::skills::list_skills(root.to_string_lossy().into_owned());
    let managed = managed_result.as_deref().unwrap_or_default();
    let mut states = Vec::new();
    for (name, source) in BUNDLED_SKILLS {
        let installed = root.join(name).is_dir();
        let entry = managed.iter().find(|skill| skill.name == name);
        let up_to_date = entry.is_some_and(|skill| {
            crate::skills::preview_skill(source.to_string())
                .is_ok_and(|bundle| bundle.content == skill.content)
        });
        states.push(SkillState {
            name: name.to_string(),
            source: source.to_string(),
            installed,
            managed: entry.is_some(),
            enabled: entry.map(|skill| skill.enabled).unwrap_or(installed),
            up_to_date,
        });
    }
    let error = if root.is_dir() {
        managed_result.err()
    } else {
        Some(format!(
            "{} does not exist yet; installing a skill creates it.",
            root.display()
        ))
    };
    (states, error)
}

fn status(profile: &Profile, port: u16) -> ClientStatus {
    let detected = detected(profile);
    let mcp_path = real_path(&profile.mcp);
    let mut mcp_installed = false;
    let mut mcp_managed = false;
    let mut mcp_entry = None;
    let mut mcp_error = None;

    if detected {
        if profile.toml {
            match read_toml(&mcp_path) {
                Ok(document) => {
                    let (entry, ours) = toml_status(&document, port);
                    mcp_installed = entry.is_some();
                    mcp_managed = ours;
                    mcp_entry = entry;
                }
                Err(error) => mcp_error = Some(error),
            }
        } else {
            match crate::integrations::read_mcp_config(mcp_path.to_string_lossy().into_owned()) {
                Ok(view) => {
                    mcp_entry = view
                        .servers
                        .iter()
                        .find(|(_, definition)| points_at_gateway(definition, port))
                        .map(|(name, _)| name.clone());
                    mcp_installed = mcp_entry.is_some();
                    mcp_managed = view.managed.contains_key(SERVER_NAME);
                }
                Err(error) => mcp_error = Some(error),
            }
        }
    }

    let (skills_path, skills, skills_error) = match (&profile.skills, detected) {
        (Some(root), true) => {
            let root = real_path(root);
            let (skills, error) = skill_states(&root);
            (Some(root.to_string_lossy().into_owned()), skills, error)
        }
        (Some(root), false) => (Some(root.to_string_lossy().into_owned()), Vec::new(), None),
        (None, _) => (None, Vec::new(), None),
    };

    ClientStatus {
        id: profile.id.to_string(),
        name: profile.name.to_string(),
        detected,
        mcp_path: mcp_path.to_string_lossy().into_owned(),
        mcp_format: if profile.toml {
            "toml".into()
        } else {
            "json".into()
        },
        mcp_installed,
        mcp_managed,
        mcp_entry,
        mcp_error,
        skills_path,
        skills,
        skills_error,
        note: profile.note.to_string(),
        token_note: None,
    }
}

pub fn list(port: u16) -> Vec<ClientStatus> {
    profiles()
        .iter()
        .map(|profile| status(profile, port))
        .collect()
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Claude requires an explicit HTTP transport for URL-based servers. The token
/// is only included when the gateway is protected; the UI warns that JSON
/// clients store it in plaintext.
fn claude_definition(port: u16, token: Option<&str>) -> Value {
    match token.map(str::trim).filter(|token| !token.is_empty()) {
        Some(token) => json!({
            "type": "http",
            "url": gateway_url(port),
            "headers": { "Authorization": format!("Bearer {token}") },
        }),
        None => json!({ "type": "http", "url": gateway_url(port) }),
    }
}

/// Antigravity's documented remote transport is legacy SSE and uses
/// `serverUrl`, not the Streamable HTTP `url` field used by Claude and Codex.
fn antigravity_definition(port: u16, token: Option<&str>) -> Value {
    match token.map(str::trim).filter(|token| !token.is_empty()) {
        Some(token) => json!({
            "serverUrl": gateway_sse_url(port),
            "headers": { "Authorization": format!("Bearer {token}") },
        }),
        None => json!({ "serverUrl": gateway_sse_url(port) }),
    }
}

pub fn install_mcp(id: &str, port: u16, token: Option<String>) -> Result<ClientStatus, String> {
    install_mcp_for(&profile(id)?, port, token)
}

fn install_mcp_for(
    profile: &Profile,
    port: u16,
    token: Option<String>,
) -> Result<ClientStatus, String> {
    if !detected(profile) {
        return Err(format!("{} was not found on this machine", profile.name));
    }
    let path = real_path(&profile.mcp);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    let has_token = token
        .as_deref()
        .map(str::trim)
        .is_some_and(|token| !token.is_empty());
    let mut note = None;
    if profile.toml {
        let mut document = read_toml(&path)?;
        if let (Some(existing), false) = toml_status(&document, port) {
            if existing != SERVER_NAME && existing != LEGACY_SERVER_NAME {
                return Err(format!(
                    "{} already points at this gateway under the name '{existing}'. Remove that entry first if you want it managed here.",
                    profile.name
                ));
            }
        }
        let servers = document["mcp_servers"].or_insert(toml_edit::table());
        let servers = servers
            .as_table_mut()
            .ok_or("mcp_servers is not a table in config.toml")?;
        servers.set_implicit(true);
        let mut entry = toml_edit::Table::new();
        entry.insert("url", toml_edit::value(gateway_url(port)));
        if has_token {
            // Codex reads the token from the environment, never from the file.
            entry.insert("bearer_token_env_var", toml_edit::value(CODEX_TOKEN_ENV));
        }
        servers.insert(SERVER_NAME, toml_edit::Item::Table(entry));
        // Installing over a config written before the rename would otherwise
        // leave two entries pointing at the same gateway.
        servers.remove(LEGACY_SERVER_NAME);
        write_toml(&path, &document)?;
        if has_token {
            note = Some(format!(
                "Codex reads the token from the environment: export {CODEX_TOKEN_ENV}=<your gateway token> before starting Codex. The token itself is not written to config.toml."
            ));
        }
    } else {
        let view = crate::integrations::read_mcp_config(path.to_string_lossy().into_owned())?;
        let action = if view.managed.contains_key(SERVER_NAME) {
            "update"
        } else if view.servers.contains_key(SERVER_NAME) {
            return Err(format!(
                "{} already has an entry named '{SERVER_NAME}' that this app did not create; it will not be overwritten.",
                profile.name
            ));
        } else {
            "add"
        };
        let definition = if profile.id == "antigravity" {
            antigravity_definition(port, token.as_deref())
        } else if profile.id == "opencode" {
            match token
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty())
            {
                Some(token) => json!({
                    "type": "remote",
                    "url": gateway_url(port),
                    "enabled": true,
                    "headers": { "Authorization": format!("Bearer {token}") },
                }),
                None => json!({
                    "type": "remote",
                    "url": gateway_url(port),
                    "enabled": true,
                }),
            }
        } else {
            claude_definition(port, token.as_deref())
        };
        crate::integrations::edit_mcp_config(
            path.to_string_lossy().into_owned(),
            view.revision,
            SERVER_NAME.to_string(),
            action.to_string(),
            Some(definition),
        )?;
        // Same cleanup as the TOML path: remove the entry an older build wrote,
        // but only when this app is the one that created it.
        if view.managed.contains_key(LEGACY_SERVER_NAME) {
            let view = crate::integrations::read_mcp_config(path.to_string_lossy().into_owned())?;
            let _ = crate::integrations::edit_mcp_config(
                path.to_string_lossy().into_owned(),
                view.revision,
                LEGACY_SERVER_NAME.to_string(),
                "remove".to_string(),
                None,
            );
        }
        if has_token {
            note = Some(format!(
                "The gateway token was written into {} as an Authorization header. That file is plain text — keep it out of any shared folder or repository.",
                path.display()
            ));
        }
    }
    Ok(ClientStatus {
        token_note: note,
        ..status(profile, port)
    })
}

pub fn remove_mcp(id: &str, port: u16) -> Result<ClientStatus, String> {
    remove_mcp_for(&profile(id)?, port)
}

fn remove_mcp_for(profile: &Profile, port: u16) -> Result<ClientStatus, String> {
    let path = real_path(&profile.mcp);
    // Removal has to cover the pre-rename entry as well. A client wired up by an
    // older build carries the name this app used then, so looking only for the
    // current one would refuse to uninstall — and leave the stale entry pointing
    // at a gateway the user is trying to disconnect.
    if profile.toml {
        let mut document = read_toml(&path)?;
        let removed = document
            .get_mut("mcp_servers")
            .and_then(|item| item.as_table_like_mut())
            .map(|servers| {
                let current = servers.remove(SERVER_NAME).is_some();
                let legacy = servers.remove(LEGACY_SERVER_NAME).is_some();
                current || legacy
            })
            .unwrap_or(false);
        if !removed {
            return Err(format!(
                "No '{SERVER_NAME}' entry to remove in {}",
                path.display()
            ));
        }
        write_toml(&path, &document)?;
    } else {
        let mut view = crate::integrations::read_mcp_config(path.to_string_lossy().into_owned())?;
        let names: Vec<&str> = [SERVER_NAME, LEGACY_SERVER_NAME]
            .into_iter()
            .filter(|name| view.managed.contains_key(*name))
            .collect();
        if names.is_empty() {
            return Err(
                "Only the entry this app installed can be removed from here; edit the client config yourself for anything else."
                    .to_string(),
            );
        }
        for name in names {
            crate::integrations::edit_mcp_config(
                path.to_string_lossy().into_owned(),
                view.revision.clone(),
                name.to_string(),
                "remove".to_string(),
                None,
            )?;
            // Each edit bumps the revision, so re-read before the next one.
            view = crate::integrations::read_mcp_config(path.to_string_lossy().into_owned())?;
        }
    }
    Ok(status(profile, port))
}

pub fn install_skills(id: &str, port: u16) -> Result<ClientStatus, String> {
    install_skills_for(&profile(id)?, port)
}

pub fn install_all(id: &str, port: u16, token: Option<String>) -> Result<ClientStatus, String> {
    let profile = profile(id)?;
    install_all_for(&profile, port, token)
}

fn install_all_for(
    profile: &Profile,
    port: u16,
    token: Option<String>,
) -> Result<ClientStatus, String> {
    let installed = install_mcp_for(profile, port, token)?;
    if profile.skills.is_some() {
        install_skills_for(profile, port)
    } else {
        Ok(installed)
    }
}

fn install_skills_for(profile: &Profile, port: u16) -> Result<ClientStatus, String> {
    let Some(root) = profile.skills.clone() else {
        return Err(format!(
            "{} does not load skills from a folder",
            profile.name
        ));
    };
    if !detected(profile) {
        return Err(format!("{} was not found on this machine", profile.name));
    }
    if !root.exists() {
        fs::create_dir_all(&root)
            .map_err(|error| format!("Could not create {}: {error}", root.display()))?;
    }
    let root = real_path(&root);
    let target = root.to_string_lossy().into_owned();
    let mut failures = Vec::new();
    for (name, source) in BUNDLED_SKILLS {
        if root.join(name).is_dir() {
            let installed = crate::skills::list_skills(target.clone())?
                .into_iter()
                .find(|skill| skill.name == name);
            let current = crate::skills::preview_skill(source.to_string())?;
            if installed
                .as_ref()
                .is_some_and(|skill| skill.content == current.content)
            {
                continue;
            }
            if installed.is_none() {
                continue;
            }
            match crate::skills::remove_skill(target.clone(), name.to_string()) {
                Ok(archive) => {
                    if let Err(error) =
                        crate::skills::install_skill(source.to_string(), target.clone())
                    {
                        let original = root.join(name);
                        let rollback = fs::rename(&archive, &original)
                            .map_err(|rollback| format!("{error}; rollback failed: {rollback}"));
                        failures.push(format!("{name}: {}", rollback.err().unwrap_or(error)));
                    }
                }
                Err(error) => failures.push(format!("{name}: {error}")),
            }
            continue;
        }
        if let Err(error) = crate::skills::install_skill(source.to_string(), target.clone()) {
            failures.push(format!("{name}: {error}"));
        }
    }
    if !failures.is_empty() {
        return Err(failures.join(" · "));
    }
    Ok(status(profile, port))
}

pub fn remove_skills(id: &str, port: u16) -> Result<ClientStatus, String> {
    remove_skills_for(&profile(id)?, port)
}

fn remove_skills_for(profile: &Profile, port: u16) -> Result<ClientStatus, String> {
    let Some(root) = profile.skills.clone() else {
        return Err(format!(
            "{} does not load skills from a folder",
            profile.name
        ));
    };
    let root = real_path(&root);
    let target = root.to_string_lossy().into_owned();
    let mut failures = Vec::new();
    let mut removed = 0;
    for (name, _) in BUNDLED_SKILLS {
        if !root.join(name).is_dir() {
            continue;
        }
        match crate::skills::remove_skill(target.clone(), name.to_string()) {
            Ok(_) => removed += 1,
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    if removed == 0 && !failures.is_empty() {
        return Err(failures.join(" · "));
    }
    Ok(status(profile, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let root = std::env::temp_dir().join(format!("sg-clients-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::canonicalize(root).unwrap()
    }

    /// A home directory containing every client this module knows about, so the
    /// real one is never touched by a test.
    fn fake_home() -> PathBuf {
        let home = scratch();
        let claude_desktop = claude_desktop_dir(&home);
        fs::create_dir_all(&claude_desktop).unwrap();
        fs::write(
            claude_desktop.join("claude_desktop_config.json"),
            br#"{"theme":"dark","mcpServers":{}}"#,
        )
        .unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/config.toml"), "model = \"gpt-5\"\n").unwrap();
        fs::create_dir_all(home.join(".gemini/config")).unwrap();
        fs::write(
            home.join(".gemini/config/mcp_config.json"),
            br#"{"mcpServers":{"other":{"command":"/usr/bin/node"}}}"#,
        )
        .unwrap();
        fs::create_dir_all(home.join(".config/opencode")).unwrap();
        fs::write(
            home.join(".config/opencode/opencode.jsonc"),
            br#"{"$schema":"https://opencode.ai/config.json","mcp":{}}"#,
        )
        .unwrap();
        home
    }

    #[test]
    fn home_falls_back_when_home_is_not_absolute() {
        let absolute = scratch();
        assert_eq!(
            home_from(Some(PathBuf::from("relative-home")), Some(absolute.clone())),
            Some(absolute.clone())
        );
        fs::remove_dir_all(absolute).unwrap();
    }

    #[test]
    fn antigravity_surfaces_share_current_config_and_skills_paths() {
        for surface in [
            "config",
            "antigravity",
            "antigravity-cli",
            "antigravity-ide",
        ] {
            let home = scratch();
            fs::create_dir_all(home.join(".gemini").join(surface)).unwrap();
            let antigravity = find(&profiles_in(&home), "antigravity").unwrap();
            let state = status(&antigravity, 8795);
            assert!(state.detected, "surface {surface} was not detected");
            assert_eq!(antigravity.mcp, home.join(".gemini/config/mcp_config.json"));
            assert_eq!(antigravity.skills, Some(home.join(".gemini/config/skills")));
            fs::remove_dir_all(home).unwrap();
        }
    }

    #[test]
    fn antigravity_keeps_an_existing_legacy_config() {
        let home = scratch();
        let legacy = home.join(".gemini/antigravity/mcp_config.json");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, br#"{"mcpServers":{}}"#).unwrap();
        let antigravity = find(&profiles_in(&home), "antigravity").unwrap();
        assert_eq!(antigravity.mcp, legacy);
        assert_eq!(antigravity.skills, Some(home.join(".gemini/config/skills")));
        fs::remove_dir_all(home).unwrap();
    }

    /// Uninstall has to work for clients wired up before the rename. Removal
    /// looked only for the current entry name, so a config written by an older
    /// build answered "no entry to remove" and kept pointing at the gateway the
    /// user was trying to disconnect.
    #[test]
    fn a_client_wired_up_before_the_rename_can_still_be_unwired() {
        let home = fake_home();
        let profiles = profiles_in(&home);

        // JSON client (Claude Desktop): entry written under the old name, and
        // recorded as managed by this app.
        let claude = find(&profiles, "claude_desktop").unwrap();
        let path = claude_desktop_dir(&home).join("claude_desktop_config.json");
        crate::integrations::edit_mcp_config(
            path.to_string_lossy().into_owned(),
            crate::integrations::read_mcp_config(path.to_string_lossy().into_owned())
                .unwrap()
                .revision,
            LEGACY_SERVER_NAME.to_string(),
            "add".to_string(),
            Some(claude_definition(8795, None)),
        )
        .unwrap();
        let removed = remove_mcp_for(&claude, 8795).unwrap();
        assert!(
            !removed.mcp_installed,
            "the pre-rename entry survived removal"
        );
        let document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(document["mcpServers"].get(LEGACY_SERVER_NAME).is_none());

        // TOML client (Codex): same situation in the other config format.
        let codex = find(&profiles, "codex").unwrap();
        let codex_path = real_path(&codex.mcp);
        fs::create_dir_all(codex_path.parent().unwrap()).unwrap();
        fs::write(
            &codex_path,
            format!(
                "model = \"gpt-5\"\n\n[mcp_servers.{LEGACY_SERVER_NAME}]\nurl = \"{}\"\n",
                gateway_url(8795)
            ),
        )
        .unwrap();
        remove_mcp_for(&codex, 8795).unwrap();
        let written = fs::read_to_string(&codex_path).unwrap();
        assert!(!written.contains(LEGACY_SERVER_NAME), "{written}");
        // Unrelated configuration is untouched.
        assert!(written.contains("model = \"gpt-5\""));
    }

    #[test]
    fn a_json_client_is_detected_wired_up_and_can_be_unwired() {
        let home = fake_home();
        let profiles = profiles_in(&home);
        let claude = find(&profiles, "claude_desktop").unwrap();

        let before = status(&claude, 8795);
        assert!(before.detected);
        assert!(!before.mcp_installed);
        assert!(before.skills_path.is_none());
        assert!(before.skills.is_empty());

        let installed = install_mcp_for(&claude, 8795, None).unwrap();
        assert!(installed.mcp_installed && installed.mcp_managed);
        assert_eq!(installed.mcp_entry.as_deref(), Some(SERVER_NAME));
        // Everything the client stored outside mcpServers survives the edit.
        let document: Value = serde_json::from_slice(
            &fs::read(claude_desktop_dir(&home).join("claude_desktop_config.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(document["theme"], "dark");
        assert_eq!(document["mcpServers"][SERVER_NAME]["type"], "http");
        assert_eq!(
            document["mcpServers"][SERVER_NAME]["url"],
            gateway_url(8795)
        );

        assert!(install_skills_for(&claude, 8795).is_err());
        assert!(install_all_for(&claude, 8795, None).is_ok());

        let removed = remove_mcp_for(&claude, 8795).unwrap();
        assert!(!removed.mcp_installed);
        assert!(remove_skills_for(&claude, 8795).is_err());
        let document: Value = serde_json::from_slice(
            &fs::read(claude_desktop_dir(&home).join("claude_desktop_config.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(document["theme"], "dark");
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn exposes_four_clients_and_installs_mcp_and_skills_together() {
        let home = fake_home();
        let profiles = profiles_in(&home);
        assert_eq!(
            profiles
                .iter()
                .map(|profile| profile.id)
                .collect::<Vec<_>>(),
            vec!["antigravity", "claude_desktop", "codex", "opencode"]
        );
        let codex = find(&profiles, "codex").unwrap();
        let installed = install_all_for(&codex, 8795, None).unwrap();
        assert!(installed.mcp_installed && installed.mcp_managed);
        assert!(installed.skills.iter().all(|skill| skill.installed));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_toml_client_is_wired_up_without_disturbing_the_rest_of_the_file() {
        let home = fake_home();
        let profiles = profiles_in(&home);
        let codex = find(&profiles, "codex").unwrap();
        assert_eq!(status(&codex, 8795).mcp_format, "toml");

        let installed = install_mcp_for(&codex, 8795, Some("a-gateway-token".into())).unwrap();
        assert!(installed.mcp_installed && installed.mcp_managed);
        let written = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(written.contains("model = \"gpt-5\""));
        assert!(written.contains("[mcp_servers.scholargate]"));
        // Codex only accepts an environment variable name, so the token itself
        // never reaches the file.
        assert!(written.contains(CODEX_TOKEN_ENV));
        assert!(!written.contains("a-gateway-token"));

        let removed = remove_mcp_for(&codex, 8795).unwrap();
        assert!(!removed.mcp_installed);
        assert!(remove_mcp_for(&codex, 8795).is_err());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn an_entry_this_app_did_not_write_is_reported_but_never_replaced() {
        let home = fake_home();
        fs::write(
            home.join(".gemini/config/mcp_config.json"),
            format!(
                r#"{{"mcpServers":{{"my-own":{{"url":"{}"}}}}}}"#,
                gateway_url(8795)
            ),
        )
        .unwrap();
        let profiles = profiles_in(&home);
        let antigravity = find(&profiles, "antigravity").unwrap();

        let state = status(&antigravity, 8795);
        assert!(
            state.mcp_installed,
            "an entry pointing here counts as connected"
        );
        assert!(!state.mcp_managed);
        assert_eq!(state.mcp_entry.as_deref(), Some("my-own"));

        // A client that is not installed is reported, not set up behind the
        // user's back.
        let absent = find(&profiles_in(&home.join("empty")), "codex").unwrap();
        assert!(!status(&absent, 8795).detected);
        assert!(install_mcp_for(&absent, 8795, None).is_err());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn toml_install_keeps_surrounding_configuration_and_round_trips() {
        let root = scratch();
        let path = root.join("config.toml");
        let original = "# user comment\nmodel = \"gpt-5\"\n\n[mcp_servers.existing]\ncommand = \"/usr/bin/node\"\n";
        fs::write(&path, original).unwrap();

        let mut document = read_toml(&path).unwrap();
        assert_eq!(toml_status(&document, 8795), (None, false));

        let servers = document["mcp_servers"].or_insert(toml_edit::table());
        let servers = servers.as_table_mut().unwrap();
        let mut entry = toml_edit::Table::new();
        entry.insert("url", toml_edit::value(gateway_url(8795)));
        servers.insert(SERVER_NAME, toml_edit::Item::Table(entry));
        let backup = write_toml(&path, &document).unwrap().unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("# user comment\nmodel = \"gpt-5\""));
        assert!(written.contains("[mcp_servers.existing]"));
        assert!(written.contains("[mcp_servers.scholargate]"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);

        let mut document = read_toml(&path).unwrap();
        assert_eq!(
            toml_status(&document, 8795),
            (Some(SERVER_NAME.to_string()), true)
        );
        // A different port is a different gateway and must not be claimed as ours.
        assert_eq!(toml_status(&document, 9000), (None, false));

        document
            .get_mut("mcp_servers")
            .unwrap()
            .as_table_like_mut()
            .unwrap()
            .remove(SERVER_NAME);
        write_toml(&path, &document).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(!written.contains("scholargate"));
        assert!(written.contains("[mcp_servers.existing]"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_gateway_entry_is_recognised_whatever_the_user_named_it() {
        assert!(points_at_gateway(
            &json!({"url": "http://127.0.0.1:8795/mcp"}),
            8795
        ));
        assert!(points_at_gateway(
            &json!({"url": "http://localhost:8795/mcp/"}),
            8795
        ));
        assert!(points_at_gateway(
            &json!({"serverUrl": "http://127.0.0.1:8795/sse"}),
            8795
        ));
        assert!(!points_at_gateway(
            &json!({"url": "http://127.0.0.1:8795/mcp"}),
            8796
        ));
        assert!(!points_at_gateway(
            &json!({"command": "/usr/bin/node"}),
            8795
        ));
    }

    #[test]
    fn a_definition_carries_the_token_only_when_the_gateway_has_one() {
        assert_eq!(
            claude_definition(8795, None),
            json!({"type": "http", "url": "http://127.0.0.1:8795/mcp"})
        );
        assert_eq!(
            claude_definition(8795, Some("   ")),
            claude_definition(8795, None)
        );
        assert_eq!(
            claude_definition(8795, Some("s3cret-token-value"))["headers"]["Authorization"],
            "Bearer s3cret-token-value"
        );
        assert_eq!(
            antigravity_definition(8795, None),
            json!({"serverUrl": "http://127.0.0.1:8795/sse"})
        );
        assert_eq!(
            antigravity_definition(8795, Some("s3cret-token-value"))["headers"]["Authorization"],
            "Bearer s3cret-token-value"
        );
    }

    #[test]
    fn antigravity_uses_its_documented_sse_schema() {
        let home = fake_home();
        let profiles = profiles_in(&home);
        let antigravity = find(&profiles, "antigravity").unwrap();

        let installed = install_mcp_for(&antigravity, 8795, None).unwrap();
        assert!(installed.mcp_installed && installed.mcp_managed);
        let document: Value =
            serde_json::from_slice(&fs::read(home.join(".gemini/config/mcp_config.json")).unwrap())
                .unwrap();
        let entry = &document["mcpServers"][SERVER_NAME];
        assert_eq!(entry["serverUrl"], gateway_sse_url(8795));
        assert!(entry.get("url").is_none());
        assert!(entry.get("type").is_none());
        fs::remove_dir_all(home).unwrap();
    }

    /// Also covers the rename: the marker written here is the pre-rename one,
    /// so a skill installed by an older build must still be recognised as ours
    /// and updated rather than treated as somebody else's file.
    #[test]
    fn installing_skills_updates_an_unchanged_managed_old_bundle() {
        let home = fake_home();
        let skill = home.join(".codex/skills/paper-search");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::create_dir(&skill).unwrap();
        let old = b"---\nname: paper-search\ndescription: Old bundle\n---\nOld instructions.";
        fs::write(skill.join("SKILL.md"), old).unwrap();
        fs::write(
            skill.join(".scholargateway-install.json"),
            serde_json::to_vec_pretty(&json!({
                "name": "paper-search",
                "source": "builtin:paper-search",
                "enabled": true,
                "hashes": { "SKILL.md": format!("{:x}", md5::compute(old)) }
            }))
            .unwrap(),
        )
        .unwrap();
        let codex = find(&profiles_in(&home), "codex").unwrap();
        let before = status(&codex, 8795);
        assert!(
            !before
                .skills
                .iter()
                .find(|item| item.name == "paper-search")
                .unwrap()
                .up_to_date
        );

        let after = install_skills_for(&codex, 8795).unwrap();
        assert!(after.skills.iter().all(|item| item.up_to_date));
        // The pre-rename receipt must not survive the update, or every skill
        // folder installed by an older build keeps a dead file forever.
        assert!(
            !skill.join(".scholargateway-install.json").exists(),
            "the pre-rename marker was left behind"
        );
        assert!(skill.join(".scholargate-install.json").exists());
        assert_eq!(
            fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            crate::skills::preview_skill("builtin:paper-search".into())
                .unwrap()
                .content
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn unknown_clients_are_rejected_rather_than_guessed() {
        assert!(profile("not-a-client").is_err());
        assert!(install_mcp("not-a-client", 8795, None).is_err());
    }

    #[test]
    fn a_symlinked_config_resolves_to_the_file_it_points_at() {
        let root = scratch();
        let real = root.join("real.json");
        fs::write(&real, "{}").unwrap();
        #[cfg(unix)]
        {
            let link = root.join("link.json");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert_eq!(real_path(&link), real);
        }
        // A file that does not exist yet still resolves through its parent.
        assert_eq!(
            real_path(&root.join("absent.json")),
            root.join("absent.json")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn opencode_is_detected_and_wired_up_under_mcp_key() {
        let home = fake_home();
        let profiles = profiles_in(&home);
        let opencode = find(&profiles, "opencode").unwrap();

        let before = status(&opencode, 8795);
        assert!(before.detected);
        assert!(!before.mcp_installed);
        assert_eq!(before.skills.len(), 3);

        let installed = install_mcp_for(&opencode, 8795, None).unwrap();
        assert!(installed.mcp_installed && installed.mcp_managed);
        assert_eq!(installed.mcp_entry.as_deref(), Some(SERVER_NAME));

        let document: Value = serde_json::from_slice(
            &fs::read(home.join(".config/opencode/opencode.jsonc")).unwrap(),
        )
        .unwrap();
        assert_eq!(document["mcp"][SERVER_NAME]["type"], "remote");
        assert_eq!(document["mcp"][SERVER_NAME]["url"], gateway_url(8795));
        assert_eq!(document["mcp"][SERVER_NAME]["enabled"], true);

        let with_skills = install_skills_for(&opencode, 8795).unwrap();
        assert!(with_skills.skills.iter().all(|s| s.installed));
        assert!(home
            .join(".config/opencode/skills/paper-search/SKILL.md")
            .is_file());

        let removed = remove_mcp_for(&opencode, 8795).unwrap();
        assert!(!removed.mcp_installed);
        fs::remove_dir_all(home).unwrap();
    }
}
