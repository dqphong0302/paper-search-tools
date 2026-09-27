//! Browser-origin, DNS-rebinding, bearer-token and rate-limit protection for the
//! loopback gateway.
use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Fixed-window per-client counter. Keyed by agent/token so one runaway agent
/// cannot exhaust the gateway for everyone else.
static RATE: OnceLock<Mutex<HashMap<String, (u64, u32)>>> = OnceLock::new();

fn rate_check(key: &str, limit: u32) -> Result<(), u64> {
    let state = RATE.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut windows) = state.lock() else {
        return Ok(());
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if windows.len() > 2048 {
        windows.retain(|_, (start, _)| now.saturating_sub(*start) < 120);
    }
    let entry = windows.entry(key.to_string()).or_insert((now, 0));
    if now.saturating_sub(entry.0) >= 60 {
        *entry = (now, 1);
        return Ok(());
    }
    if entry.1 >= limit {
        return Err(60u64.saturating_sub(now.saturating_sub(entry.0)));
    }
    entry.1 += 1;
    Ok(())
}

pub const UI_ORIGINS: &[&str] = &[
    "http://localhost:1420",
    "http://127.0.0.1:1420",
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

pub fn trusted_origin(origin: &HeaderValue, port: u16) -> bool {
    origin.to_str().ok().is_some_and(|origin| {
        UI_ORIGINS.contains(&origin)
            || origin == format!("http://127.0.0.1:{port}")
            || origin == format!("http://localhost:{port}")
    })
}

/// Endpoints reachable without a token: health probes, the sanitized config
/// read (no secret values) so the UI can bootstrap and discover that a token is
/// required, and public OCR language models, which the OCR web worker fetches
/// without the UI's headers.
fn is_public(method: &Method, path: &str) -> bool {
    method == Method::OPTIONS
        || matches!(path, "/health" | "/api/health")
        || (method == Method::GET && path == "/api/config")
        || (method == Method::GET && path.starts_with("/api/ocr/lang/"))
}

pub async fn guard(
    State(state): State<crate::server::AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    // Only the listening loopback authority is accepted, not arbitrary DNS names.
    let authority = headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .or_else(|| request.uri().authority().map(|value| value.as_str()));
    let Some(authority) = authority else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let parsed = authority.parse::<axum::http::uri::Authority>();
    let Ok(authority) = parsed else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !["localhost", "127.0.0.1", "[::1]"].contains(&authority.host()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let port = authority.port_u16().unwrap_or(80);
    if let Some(origin) = headers.get("origin") {
        if !trusted_origin(origin, port) {
            return StatusCode::FORBIDDEN.into_response();
        }
    } else if headers
        .get("sec-fetch-site")
        .is_some_and(|value| value != "same-origin" && value != "none")
    {
        // Cross-site resource loads may omit Origin; they still must not reach GET side effects.
        return StatusCode::FORBIDDEN.into_response();
    }

    // Once a token is configured, every non-public route must present it — REST,
    // MCP Streamable HTTP and the legacy SSE/messages transport alike.
    if !is_public(request.method(), request.uri().path()) {
        let principal = match crate::agents::authenticate(&state.db, request.headers()) {
            Ok(principal) => principal,
            Err(status) => return status.into_response(),
        };
        if request.uri().path().starts_with("/api/agents")
            && !matches!(principal, crate::agents::Principal::Admin)
        {
            return (StatusCode::FORBIDDEN, axum::Json(serde_json::json!({"error":"Set a gateway administrator token in Settings, then reload this page"}))).into_response();
        }
        if let crate::agents::Principal::Agent(grant) = &principal {
            request = match scope_rest(&state, grant, request).await {
                Ok(request) => request,
                Err(status) => return status.into_response(),
            };
        }

        // Optional per-client rate limit (0 = disabled).
        let limit = state
            .db
            .get_config("rate_limit_per_minute")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        if limit > 0 {
            let key = principal.key();
            if let Err(retry) = rate_check(&key, limit) {
                let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
                response
                    .headers_mut()
                    .insert("retry-after", HeaderValue::from(retry));
                return response;
            }
        }
    }

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

async fn scope_rest(
    state: &crate::server::AppState,
    grant: &crate::agents::Grant,
    request: Request,
) -> Result<Request, StatusCode> {
    use crate::agents::{paper_allowed, workspace_allowed};
    let path = urlencoding::decode(request.uri().path())
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .into_owned();
    let method = request.method().clone();
    let parts: Vec<_> = path.split('/').collect();
    let url = reqwest::Url::parse(&format!("http://localhost{}", request.uri()))
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let query = |key: &str| -> Result<String, StatusCode> {
        let values: Vec<_> = url
            .query_pairs()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .collect();
        if values.len() != 1 {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(values[0].clone())
    };
    let allowed = match (method.as_str(), path.as_str()) {
        ("POST", "/mcp" | "/messages") | ("GET", "/sse" | "/api/catalog" | "/api/workspaces") => {
            true
        }
        ("GET", "/api/library") => workspace_allowed(grant, crate::db::INTEREST_LIBRARY_ID, false),
        ("POST" | "PATCH" | "DELETE", "/api/library") => {
            workspace_allowed(grant, crate::db::INTEREST_LIBRARY_ID, true)
        }
        ("POST", "/api/search" | "/api/web/search") => true,
        ("GET", "/api/history/searches") => {
            workspace_allowed(grant, &query("workspace_id")?, false)
        }
        ("GET", "/api/citations") => paper_allowed(&state.db, grant, &query("id")?),
        ("GET", "/api/fulltext") => paper_allowed(&state.db, grant, &query("paper_id")?),
        _ if parts.len() == 5
            && parts[1] == "api"
            && parts[2] == "workspaces"
            && parts[4] == "papers" =>
        {
            ["GET", "POST", "PATCH", "DELETE"].contains(&method.as_str())
                && workspace_allowed(grant, parts[3], method != Method::GET)
        }
        _ if method == Method::GET && parts.get(1..3) == Some(&["api", "paper"][..]) => {
            let id = if path.ends_with("/citations") {
                path.trim_start_matches("/api/paper/")
                    .trim_end_matches("/citations")
            } else {
                path.trim_start_matches("/api/paper/")
            };
            paper_allowed(&state.db, grant, id)
        }
        _ => false,
    };
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    if method == Method::POST && (path == "/api/search" || path.ends_with("/papers")) {
        let (parts, body) = request.into_parts();
        let bytes = axum::body::to_bytes(body, 1024 * 1024)
            .await
            .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
        if path == "/api/search" {
            if value.get("workspace_id").is_some_and(|id| {
                !id.is_null()
                    && !id
                        .as_str()
                        .is_some_and(|id| workspace_allowed(grant, id, false))
            }) || ["searxng_url", "searxng_categories", "searxng_engines"]
                .iter()
                .any(|key| value.get(*key).is_some_and(|v| !v.is_null()))
            {
                return Err(StatusCode::FORBIDDEN);
            }
        } else {
            let id = value["paper"]["id"]
                .as_str()
                .ok_or(StatusCode::BAD_REQUEST)?;
            if !paper_allowed(&state.db, grant, id) {
                return Err(StatusCode::FORBIDDEN);
            }
            if let Some(paper) = state.db.find_paper(id) {
                value["paper"] = serde_json::to_value(paper).unwrap();
            }
        }
        return Ok(Request::from_parts(
            parts,
            axum::body::Body::from(value.to_string()),
        ));
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::AppState;
    use axum::{body::Body, routing::get, Router};
    use tower::ServiceExt;

    fn state() -> AppState {
        AppState {
            db: crate::db::Database::in_memory().unwrap(),
            engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()),
            port: 8795,
            mcp_sessions: Default::default(),
        }
    }

    #[test]
    fn rate_limiter_blocks_over_limit_within_window() {
        let key = format!("rate-test-{}", uuid::Uuid::new_v4());
        for _ in 0..3 {
            assert!(rate_check(&key, 3).is_ok());
        }
        assert!(rate_check(&key, 3).is_err());
    }

    #[tokio::test]
    async fn gateway_token_protects_rest_and_mcp() {
        let state = state();
        state
            .db
            .set_config("mcp_auth_token", "0123456789abcdef")
            .unwrap();
        state
            .db
            .set_config("openalex_email", "private@example.com")
            .unwrap();
        state
            .db
            .set_config("download_directory", "/Users/private/Papers")
            .unwrap();
        let app = crate::server::gateway_router(state);
        // Sanitized config stays readable without a token for bootstrap.
        let request = Request::builder()
            .uri("/api/config")
            .header("host", "127.0.0.1:8795")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let public: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(public, serde_json::json!({"authentication_required":true}));
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        for (auth, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some("Bearer 0123456789abcdef"), StatusCode::OK),
        ] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("host", "127.0.0.1:8795")
                .header("content-type", "application/json");
            if let Some(value) = auth {
                request = request.header("authorization", value);
            }
            let request = request.body(Body::from(body)).unwrap();
            assert_eq!(
                app.clone().oneshot(request).await.unwrap().status(),
                expected,
                "auth={auth:?}"
            );
        }
        // SSE legacy transport is protected too.
        let request = Request::builder()
            .uri("/sse")
            .header("host", "127.0.0.1:8795")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn production_router_enforces_cors_and_blocks_config_mutation() {
        let db = crate::db::Database::in_memory().unwrap();
        db.set_config("domain_preset", "education").unwrap();
        let app = crate::server::gateway_router(crate::server::AppState {
            db: db.clone(),
            engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()),
            port: 8795,
            mcp_sessions: Default::default(),
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/config")
            .header("host", "127.0.0.1:8795")
            .header("origin", "https://evil.example")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"domain_preset":"medical"}"#))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(db.get_config("domain_preset").as_deref(), Some("education"));
        let request = Request::builder()
            .method("OPTIONS")
            .uri("/api/config")
            .header("host", "127.0.0.1:8795")
            .header("origin", "http://localhost:1420")
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "content-type")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success());
        assert_eq!(
            response.headers()["access-control-allow-origin"],
            "http://localhost:1420"
        );
        let request = Request::builder()
            .uri("/api/config")
            .header("host", "localhost:8795")
            .header("origin", "http://localhost:1420")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.oneshot(request).await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn blocks_foreign_origins_and_rebinding_but_allows_local_agents() {
        let app = Router::new()
            .route("/api/config", get(|| async { "private settings" }))
            .layer(axum::middleware::from_fn_with_state(state(), guard));
        for (host, origin, fetch_site, expected) in [
            ("127.0.0.1:8795", None, None, StatusCode::OK),
            (
                "localhost:8795",
                Some("http://localhost:1420"),
                Some("same-site"),
                StatusCode::OK,
            ),
            (
                "localhost:8795",
                Some("tauri://localhost"),
                None,
                StatusCode::OK,
            ),
            (
                "localhost:8795",
                Some("https://tauri.localhost"),
                None,
                StatusCode::OK,
            ),
            (
                "localhost:8795",
                Some("https://evil.example"),
                None,
                StatusCode::FORBIDDEN,
            ),
            ("localhost:8795", Some("null"), None, StatusCode::FORBIDDEN),
            (
                "localhost:8795",
                Some("http://localhost:9999"),
                None,
                StatusCode::FORBIDDEN,
            ),
            ("evil.example:8795", None, None, StatusCode::FORBIDDEN),
            (
                "localhost:8795",
                None,
                Some("cross-site"),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let mut request = Request::builder().uri("/api/config").header("host", host);
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            if let Some(site) = fetch_site {
                request = request.header("sec-fetch-site", site);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "host={host} origin={origin:?}");
            if expected == StatusCode::OK {
                assert_eq!(response.headers()["cache-control"], "no-store");
            }
        }
    }

    // The guard rejects agent-token management until an administrator token exists.
    // That rejection has to reach the trusted UI as a readable 403, otherwise the
    // browser turns it into an opaque "Failed to fetch" and the screen cannot say
    // what to do next. A foreign origin must still get nothing.
    #[tokio::test]
    async fn rejections_carry_cors_headers_for_the_trusted_ui_only() {
        let app = crate::server::gateway_router(crate::server::AppState {
            db: crate::db::Database::in_memory().unwrap(),
            engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()),
            port: 8795,
            mcp_sessions: Default::default(),
        });

        let request = Request::builder()
            .uri("/api/agents")
            .header("host", "localhost:8795")
            .header("origin", "http://localhost:1420")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers()["access-control-allow-origin"],
            "http://localhost:1420"
        );
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            value["error"]
                .as_str()
                .unwrap()
                .contains("administrator token"),
            "{value}"
        );

        let request = Request::builder()
            .uri("/api/agents")
            .header("host", "localhost:8795")
            .header("origin", "https://evil.example")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(!response
            .headers()
            .contains_key("access-control-allow-origin"));
    }
}
