//! Scoped local agent credentials. Only hashes are persisted; creation returns the token once.
use crate::{db::Database, server::AppState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
pub struct Grant {
    pub id: String,
    pub name: String,
    pub workspace_ids: Vec<String>,
    pub writable: bool,
    pub created_at: u64,
    pub revoked: bool,
}

#[derive(Clone)]
pub enum Principal {
    Local,
    Admin,
    Agent(Grant),
}

impl Principal {
    pub fn key(&self) -> String {
        match self {
            Self::Local => "local".into(),
            Self::Admin => "admin".into(),
            Self::Agent(g) => format!("agent:{}", g.id),
        }
    }
}

pub fn authenticate(db: &Database, headers: &HeaderMap) -> Result<Principal, StatusCode> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let master = db.get_config("mcp_auth_token").filter(|v| !v.is_empty());
    if let Some(token) = token {
        if master.as_deref() == Some(token) {
            return Ok(Principal::Admin);
        }
        if token.len() <= 256 && token.starts_with("sg_agent_") {
            let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
            let conn = db
                .conn
                .lock()
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            let grant = conn.query_row("SELECT id, name, workspace_ids, writable, created_at, revoked FROM agent_credentials WHERE token_hash=?1 AND revoked=0", [hash], grant_row)
                .optional().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            if let Some(grant) = grant {
                return Ok(Principal::Agent(grant));
            }
        }
        return Err(StatusCode::UNAUTHORIZED);
    }
    if headers.contains_key("authorization") || master.is_some() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    // Never reopen anonymous access if an administrator clears the master token
    // while scoped credentials remain active. Native settings can restore it.
    let conn = db
        .conn
        .lock()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let active: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM agent_credentials WHERE revoked=0)",
            [],
            |r| r.get(0),
        )
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if active {
        Err(StatusCode::UNAUTHORIZED)
    } else {
        Ok(Principal::Local)
    }
}

fn grant_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Grant> {
    let raw: String = row.get(2)?;
    let ids = serde_json::from_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Grant {
        id: row.get(0)?,
        name: row.get(1)?,
        workspace_ids: ids,
        writable: row.get(3)?,
        created_at: row.get(4)?,
        revoked: row.get(5)?,
    })
}

pub fn workspace_allowed(grant: &Grant, id: &str, write: bool) -> bool {
    (!write || grant.writable) && grant.workspace_ids.iter().any(|allowed| allowed == id)
}

pub fn paper_allowed(db: &Database, grant: &Grant, id: &str) -> bool {
    let canonical = db
        .find_paper(id)
        .map(|p| p.id)
        .unwrap_or_else(|| id.to_string());
    let Ok(conn) = db.conn.lock() else {
        return false;
    };
    let Ok(mut stmt) = conn.prepare("SELECT workspace_id FROM workspace_papers WHERE paper_id=?1")
    else {
        return false;
    };
    let Ok(rows) = stmt.query_map([canonical], |r| r.get::<_, String>(0)) else {
        return false;
    };
    let Ok(ids) = rows.collect::<Result<Vec<_>, _>>() else {
        return false;
    };
    ids.is_empty() || ids.iter().any(|id| workspace_allowed(grant, id, false))
}

pub fn tool_allowed(db: &Database, grant: &Grant, name: &str, args: &Value) -> bool {
    match name {
        "list_workspaces" | "get_search_catalog" | "search_web" => true,
        "list_interested_papers" => workspace_allowed(grant, crate::db::INTEREST_LIBRARY_ID, false),
        "mark_paper_interested" => {
            workspace_allowed(grant, crate::db::INTEREST_LIBRARY_ID, true)
                && args["paper"]["id"]
                    .as_str()
                    .is_some_and(|id| paper_allowed(db, grant, id))
        }
        "unmark_paper_interested" => {
            workspace_allowed(grant, crate::db::INTEREST_LIBRARY_ID, true)
                && args["paper_id"]
                    .as_str()
                    .is_some_and(|id| paper_allowed(db, grant, id))
        }
        "search_academic_papers" => args.get("workspace_id").map_or(true, |v| {
            v.as_str()
                .is_some_and(|id| workspace_allowed(grant, id, false))
        }),
        "get_workspace" => args["workspace_id"]
            .as_str()
            .is_some_and(|id| workspace_allowed(grant, id, false)),
        "save_paper_to_workspace" => {
            args["workspace_id"]
                .as_str()
                .is_some_and(|id| workspace_allowed(grant, id, true))
                && args["paper"]["id"]
                    .as_str()
                    .is_some_and(|id| paper_allowed(db, grant, id))
        }
        // Writes files to the user's disk, so it needs a write grant.
        "export_collection" => args["workspace_id"]
            .as_str()
            .is_some_and(|id| workspace_allowed(grant, id, true)),
        "get_paper_details" | "get_citations" | "get_paper_fulltext" => args["paper_id"]
            .as_str()
            .is_some_and(|id| paper_allowed(db, grant, id)),
        _ => false,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGrant {
    pub name: String,
    pub workspace_ids: Vec<String>,
    #[serde(default)]
    pub writable: bool,
}

pub fn create(db: &Database, input: CreateGrant) -> Result<Value, String> {
    if db
        .get_config("mcp_auth_token")
        .filter(|v| !v.is_empty())
        .is_none()
    {
        return Err("Set a gateway administrator token first".into());
    }
    let name = input.name.trim();
    if name.is_empty()
        || name.chars().count() > 60
        || name.chars().any(char::is_control)
        || input.workspace_ids.len() > 100
        || input
            .workspace_ids
            .iter()
            .any(|id| !db.workspace_exists(id))
    {
        return Err("Choose a name and valid library access".into());
    }
    let mut ids = if input.workspace_ids.is_empty() {
        vec![crate::db::INTEREST_LIBRARY_ID.to_string()]
    } else {
        input.workspace_ids
    };
    ids.sort();
    ids.dedup();
    let token = format!(
        "sg_agent_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let grant = Grant {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        workspace_ids: ids,
        writable: input.writable,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        revoked: false,
    };
    let conn = db.conn.lock().map_err(|_| "Credential store unavailable")?;
    conn.execute("INSERT INTO agent_credentials (id,name,token_hash,workspace_ids,writable,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![grant.id, grant.name, format!("{:x}", Sha256::digest(token.as_bytes())), serde_json::to_string(&grant.workspace_ids).unwrap(), grant.writable, grant.created_at]).map_err(|e| e.to_string())?;
    Ok(json!({"agent":grant,"token":token}))
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Grant>>, StatusCode> {
    let conn = state
        .db
        .conn
        .lock()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut stmt = conn.prepare("SELECT id,name,workspace_ids,writable,created_at,revoked FROM agent_credentials ORDER BY created_at DESC,id").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let rows = stmt
        .query_map([], grant_row)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
    ))
}

pub async fn create_handler(
    State(state): State<AppState>,
    Json(input): Json<CreateGrant>,
) -> (StatusCode, Json<Value>) {
    match create(&state.db, input) {
        Ok(value) => (StatusCode::CREATED, Json(value)),
        Err(message) => (StatusCode::BAD_REQUEST, Json(json!({"error":message}))),
    }
}

pub async fn revoke(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let conn = state
        .db
        .conn
        .lock()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let changed = conn
        .execute("UPDATE agent_credentials SET revoked=1 WHERE id=?1", [id])
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if changed == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(json!({"success":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
        Router,
    };
    use tower::ServiceExt;

    fn fixture() -> (AppState, String, String, Value, Value) {
        let state = AppState {
            db: Database::in_memory().unwrap(),
            engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()),
            port: 8795,
            mcp_sessions: Default::default(),
        };
        state
            .db
            .set_config("mcp_auth_token", "administrator-test-token")
            .unwrap();
        let a = state.db.list_workspaces()[0].id.clone();
        let b = state
            .db
            .create_workspace("Private project", None)
            .unwrap()
            .id;
        let read = create(
            &state.db,
            CreateGrant {
                name: "Reader".into(),
                workspace_ids: vec![a.clone()],
                writable: false,
            },
        )
        .unwrap();
        let write = create(
            &state.db,
            CreateGrant {
                name: "Writer".into(),
                workspace_ids: vec![a.clone()],
                writable: true,
            },
        )
        .unwrap();
        (state, a, b, read, write)
    }

    async fn request(
        app: &Router,
        token: &str,
        method: &str,
        path: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", "127.0.0.1:8795")
                    .header("authorization", format!("Bearer {token}"))
                    .header("x-sg-client", "ui")
                    .header("x-sg-agent", uuid::Uuid::new_v4().to_string())
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn rpc(name: &str, args: Value) -> Value {
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})
    }

    #[tokio::test]
    async fn rest_and_mcp_enforce_scope_and_read_only_without_header_bypass() {
        let (state, a, b, read, write) = fixture();
        let paper = json!({"id":"private-paper","title":"Private draft","authors":[],"source":"Local","open_access":false});
        state
            .db
            .add_workspace_paper(
                &b,
                &serde_json::from_value(paper.clone()).unwrap(),
                Some("private note"),
            )
            .unwrap();
        let app = crate::server::gateway_router(state.clone());
        let token = read["token"].as_str().unwrap();
        let (status, list) = request(&app, token, "GET", "/api/workspaces", json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["id"], a);
        let (_, list) = request(
            &app,
            token,
            "POST",
            "/mcp",
            rpc("list_workspaces", json!({})),
        )
        .await;
        let list: Value =
            serde_json::from_str(list["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(list["workspaces"].as_array().unwrap().len(), 1);
        for (method, path, body) in [
            ("GET", format!("/api/workspaces/{b}/papers"), json!({})),
            (
                "POST",
                format!("/api/workspaces/{a}/papers"),
                json!({"paper":paper}),
            ),
            (
                "PATCH",
                format!("/api/workspaces/{a}/papers?paper_id=p"),
                json!({"note":"bad"}),
            ),
            (
                "DELETE",
                format!("/api/workspaces/{a}/papers?paper_id=p"),
                json!({}),
            ),
            ("POST", "/api/config".into(), json!({"mcp_auth_token":""})),
            ("GET", "/api/agents".into(), json!({})),
            ("POST", "/api/agents".into(), json!({})),
            ("GET", "/api/telemetry".into(), json!({})),
            ("GET", "/api/history/searches".into(), json!({})),
            (
                "GET",
                format!("/api/history/searches?workspace_id={b}"),
                json!({}),
            ),
            (
                "GET",
                format!("/api/history/searches?workspace_id={a}&workspace_id={b}"),
                json!({}),
            ),
            ("GET", "/api/paper/private-paper".into(), json!({})),
            ("GET", "/api/citations?id=private-paper".into(), json!({})),
            ("POST", "/api/open-file".into(), json!({})),
            ("POST", "/api/download".into(), json!({})),
            (
                "POST",
                "/api/search".into(),
                json!({"query":"x","sources":[],"workspace_id":b}),
            ),
            (
                "POST",
                "/api/search".into(),
                json!({"query":"x","sources":[],"searxng_url":"http://127.0.0.1/"}),
            ),
        ] {
            assert_eq!(
                request(&app, token, method, &path, body).await.0,
                StatusCode::FORBIDDEN,
                "{method} {path}"
            );
        }
        for (name, args) in [
            ("get_workspace", json!({"workspace_id":b})),
            (
                "save_paper_to_workspace",
                json!({"workspace_id":a,"paper":paper}),
            ),
            ("get_paper_details", json!({"paper_id":"private-paper"})),
            ("get_citations", json!({"paper_id":"private-paper"})),
            (
                "search_academic_papers",
                json!({"query":"x","sources":[],"workspace_id":b}),
            ),
        ] {
            let (_, response) = request(&app, token, "POST", "/mcp", rpc(name, args)).await;
            assert_eq!(response["error"]["code"], -32003, "{name}: {response}");
        }
        // Read-only queries work (including cache hits), but do not change project history.
        for _ in 0..2 {
            assert_eq!(
                request(
                    &app,
                    token,
                    "POST",
                    "/api/search",
                    json!({"query":"fixture","sources":[],"workspace_id":a})
                )
                .await
                .0,
                StatusCode::OK
            );
        }
        assert!(state.db.get_search_history(100, Some(&a)).is_empty());
        let writer = write["token"].as_str().unwrap();
        let public = json!({"id":"new-paper","title":"Public paper","authors":[],"source":"Crossref","open_access":false});
        assert_eq!(
            request(
                &app,
                writer,
                "POST",
                &format!("/api/workspaces/{a}/papers"),
                json!({"paper":public})
            )
            .await
            .0,
            StatusCode::OK
        );
        let (_, response) = request(
            &app,
            writer,
            "POST",
            "/mcp",
            rpc(
                "save_paper_to_workspace",
                json!({"workspace_id":a,"paper":public,"note":"allowed"}),
            ),
        )
        .await;
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(
            state.db.workspace_papers(&a).unwrap()[0].note.as_deref(),
            Some("allowed")
        );
        assert_eq!(
            request(
                &app,
                writer,
                "POST",
                &format!("/api/workspaces/{a}/papers"),
                json!({"paper":paper})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(
                &app,
                writer,
                "POST",
                &format!("/api/workspaces/{b}/papers"),
                json!({"paper":public})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn tokens_are_write_once_revocable_and_rate_limits_use_authenticated_identity() {
        let (state, a, _, read, _) = fixture();
        let app = crate::server::gateway_router(state.clone());
        let token = read["token"].as_str().unwrap();
        let (_, list) = request(
            &app,
            "administrator-test-token",
            "GET",
            "/api/agents",
            json!({}),
        )
        .await;
        assert!(!list.to_string().contains(token));
        assert!(!list.to_string().contains("token_hash"));
        let hash: String = state
            .db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT token_hash FROM agent_credentials WHERE id=?1",
                [read["agent"]["id"].as_str().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(hash, token);
        assert_eq!(hash.len(), 64);
        state.db.set_config("rate_limit_per_minute", "2").unwrap();
        for _ in 0..2 {
            assert_eq!(
                request(&app, token, "GET", "/api/workspaces", json!({}))
                    .await
                    .0,
                StatusCode::OK
            );
        }
        assert_eq!(
            request(&app, token, "GET", "/api/workspaces", json!({}))
                .await
                .0,
            StatusCode::TOO_MANY_REQUESTS
        );
        state.db.set_config("rate_limit_per_minute", "0").unwrap();
        assert!(state
            .db
            .set_config_patch(&std::collections::BTreeMap::from([(
                "mcp_auth_token".into(),
                String::new()
            )]))
            .is_err());
        let path = format!("/api/agents/{}", read["agent"]["id"].as_str().unwrap());
        assert_eq!(
            request(&app, token, "DELETE", &path, json!({})).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(&app, "administrator-test-token", "DELETE", &path, json!({}))
                .await
                .0,
            StatusCode::OK
        );
        for endpoint in ["/mcp", "/messages?session_id=missing"] {
            assert_eq!(
                request(
                    &app,
                    token,
                    "POST",
                    endpoint,
                    rpc("get_workspace", json!({"workspace_id":a}))
                )
                .await
                .0,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            request(&app, token, "GET", "/sse", json!({})).await.0,
            StatusCode::UNAUTHORIZED
        );
        state.db.set_config("mcp_auth_token", "").unwrap();
        assert!(
            authenticate(&state.db, &HeaderMap::new()).is_err(),
            "Clearing master must not reopen anonymous access"
        );
    }

    #[tokio::test]
    async fn sse_sessions_are_bound_to_the_authenticated_owner() {
        use futures::StreamExt;
        let (state, _, _, read, write) = fixture();
        let app = crate::server::gateway_router(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/sse")
                    .header("host", "127.0.0.1:8795")
                    .header(
                        "authorization",
                        format!("Bearer {}", read["token"].as_str().unwrap()),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let mut stream = response.into_body().into_data_stream();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let frame = String::from_utf8(frame.to_vec()).unwrap();
        let endpoint = frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap();
        let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"});
        assert_eq!(
            request(
                &app,
                write["token"].as_str().unwrap(),
                "POST",
                endpoint,
                body.clone()
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(
                &app,
                read["token"].as_str().unwrap(),
                "POST",
                endpoint,
                body
            )
            .await
            .0,
            StatusCode::ACCEPTED
        );
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(String::from_utf8(frame.to_vec())
            .unwrap()
            .contains("result"));
    }
}
