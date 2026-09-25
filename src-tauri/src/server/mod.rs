use crate::db::Database;
use crate::engine::AcademicEngine;
use crate::models::{
    AgentLog, CreateWorkspaceRequest, DownloadRecord, DownloadRequest, DownloadResponse,
    OpenFileRequest, SearchHistoryItem, SearchRequest, SearchResponse, TestLlmRequest,
    TestLlmResponse, UpdateWorkspacePaperRequest, UpdateWorkspaceRequest, Workspace,
    WorkspacePaperRequest,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::{delete, get, patch, post},
    Json, Router,
};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tower_http::cors::{Any, CorsLayer};

mod downloads;
mod search;
mod settings;
mod workspaces;

use downloads::*;
pub(crate) use search::search_service;
use search::*;
use settings::*;
use workspaces::*;

/// `?workspace_id=` filter shared by search and download history.
#[derive(serde::Deserialize)]
struct HistoryQuery {
    workspace_id: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub engine: Arc<AcademicEngine>,
    pub port: u16,
    pub mcp_sessions: crate::mcp::Sessions,
}

pub async fn start_server(port: u16, db: Database) {
    let state = AppState {
        db,
        engine: Arc::new(AcademicEngine::new()),
        port,
        mcp_sessions: Default::default(),
    };

    let app = gateway_router(state);

    let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("Gateway could not bind port {port}: {error}");
            return;
        }
    };
    println!("ScholarGate: http://127.0.0.1:{port} (REST + /mcp + /sse)");
    let _ = axum::serve(listener, app).await;
}

pub(crate) fn gateway_router(state: AppState) -> Router {
    let port = state.port;
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            move |origin, _| crate::security::trusted_origin(origin, port),
        ))
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_handler))
        .route("/api/health", get(health_handler))
        .route(
            "/api/catalog",
            get(|| async { Json(crate::catalog::catalog()) }),
        )
        .route(
            "/api/agents",
            get(crate::agents::list).post(crate::agents::create_handler),
        )
        .route("/api/agents/{id}", delete(crate::agents::revoke))
        .route("/api/search", post(search_handler))
        .route("/api/query/preview", post(query_preview_handler))
        .route("/api/web/search", post(crate::web_search::handler))
        .route("/api/paper/{id}", get(paper_handler))
        .route("/api/paper/{id}/citations", get(citations_handler))
        .route("/api/citations", get(citations_query_handler))
        .route("/api/download", post(download_handler))
        .route("/api/downloads/{id}/content", get(download_content_handler))
        .route("/api/telemetry", get(telemetry_handler))
        .route("/api/trends", get(trends_handler))
        .route(
            "/api/history/searches",
            get(get_search_history_handler).delete(clear_search_history_handler),
        )
        .route(
            "/api/history/searches/{id}",
            delete(delete_search_history_item_handler).patch(save_search_handler),
        )
        .route(
            "/api/history/downloads",
            get(get_download_history_handler).delete(clear_download_history_handler),
        )
        .route(
            "/api/history/downloads/{id}",
            delete(delete_download_handler),
        )
        .route("/api/open-file", post(open_file_handler))
        .route(
            "/api/config",
            get(get_config_handler).post(set_config_handler),
        )
        .route(
            "/api/library",
            get(list_library_handler)
                .post(add_library_paper_handler)
                .patch(update_library_paper_handler)
                .delete(remove_library_paper_handler),
        )
        .route(
            "/api/workspaces",
            get(list_workspaces_handler).post(create_workspace_handler),
        )
        .route(
            "/api/workspaces/{id}",
            patch(update_workspace_handler).delete(delete_workspace_handler),
        )
        .route(
            "/api/workspaces/{id}/papers",
            get(list_workspace_papers_handler)
                .post(add_workspace_paper_handler)
                .patch(update_workspace_note_handler)
                .delete(remove_workspace_paper_handler),
        )
        .route("/api/source/check", post(source_check_handler))
        .route("/api/test-llm", post(test_llm_handler))
        .route("/api/test-searxng", post(test_searxng_handler))
        .route("/api/cache/clear", post(clear_cache_handler))
        .route("/mcp", post(crate::mcp::http))
        .route("/sse", get(crate::mcp::sse))
        .route("/messages", post(crate::mcp::message))
        // The last layer added is the outermost, so CORS must come after the guard:
        // otherwise the guard's rejections skip it and the browser reports an opaque
        // "Failed to fetch" instead of the explanation the response actually carries.
        // The allowlist predicate is unchanged, so an untrusted origin still gets a
        // 403 with no CORS header.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::security::guard,
        ))
        .layer(cors)
        .with_state(state)
}

/// Identify who issued a search. The UI labels itself explicitly; MCP injects
/// `x-sg-agent`; everything else is treated as an external REST caller.
fn client_label(headers: &HeaderMap) -> String {
    let clean = |value: &str| value.trim().chars().take(60).collect::<String>();
    if let Some(agent) = headers
        .get("x-sg-agent")
        .and_then(|v| v.to_str().ok())
        .map(clean)
    {
        if !agent.is_empty() {
            return format!("Agent · {agent}");
        }
    }
    if headers.get("x-sg-client").and_then(|v| v.to_str().ok()) == Some("ui") {
        return "User (UI)".to_string();
    }
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(clean)
        .unwrap_or_default();
    if user_agent.is_empty() {
        "REST client".to_string()
    } else {
        format!("REST · {user_agent}")
    }
}

// Handler 1: Health check
async fn health_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "ScholarGate Desktop",
        "port": state.port,
        "mode": "standalone_local",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Hex digest used wherever a value is addressed by its content.
///
/// SHA-256 rather than MD5: a collision here does not leak anything, but it
/// does serve one request's cached results to a different query, or write one
/// paper's PDF over another's file. Truncated to 32 hex characters, which is
/// the same key width as before while keeping SHA-256's collision resistance.
pub(crate) fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests;
