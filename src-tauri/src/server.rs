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

static SEARCH_GATE: tokio::sync::Mutex<Option<tokio::time::Instant>> =
    tokio::sync::Mutex::const_new(None);

async fn wait_for_search_slot(delay: Duration) {
    if delay.is_zero() {
        return;
    }
    let mut next = SEARCH_GATE.lock().await;
    if let Some(ready) = *next {
        tokio::time::sleep_until(ready).await;
    }
    *next = Some(tokio::time::Instant::now() + delay);
}

#[derive(serde::Deserialize)]
struct TrendsQuery {
    geo: Option<String>,
}

#[derive(serde::Deserialize)]
struct HistoryQuery {
    workspace_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct WorkspacePaperQuery {
    paper_id: String,
}

#[derive(serde::Deserialize)]
struct CitationsQuery {
    direction: Option<String>,
    limit: Option<usize>,
}

/// Query form accepts arbitrary IDs (OpenAlex URLs contain slashes).
#[derive(serde::Deserialize)]
struct CitationsByIdQuery {
    id: String,
    direction: Option<String>,
    limit: Option<usize>,
}

#[derive(serde::Deserialize)]
struct SourceCheckRequest {
    id: String,
    /// Optional override; the default is a query the source can plausibly answer.
    #[serde(default)]
    query: Option<String>,
}

#[derive(serde::Serialize)]
struct SourceCheckResponse {
    id: String,
    name: String,
    ok: bool,
    /// Results the probe query returned. Zero is still a pass — the source
    /// answered, it just had nothing on that topic.
    count: usize,
    elapsed_ms: u64,
    query: String,
    needs_setup: bool,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct QueryPreviewRequest {
    query: String,
    #[serde(default)]
    sources: Option<Vec<String>>,
}

#[derive(serde::Serialize)]
struct QueryPreviewResponse {
    query: String,
    adapter_version: &'static str,
    sources: Vec<crate::query::QueryPreview>,
}

#[derive(serde::Deserialize)]
struct SaveSearchRequest {
    #[serde(default)]
    saved: bool,
}

#[derive(serde::Serialize)]
struct TrendItem {
    title: String,
    traffic: Option<String>,
    published_at: Option<String>,
    explore_url: String,
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

// Resolve user defaults once, before hashing or dispatching.
fn apply_search_defaults(req: &mut SearchRequest, get: impl Fn(&str) -> Option<String>) {
    req.limit = Some(
        req.limit
            .or_else(|| get("max_results_default").and_then(|value| value.parse().ok()))
            .unwrap_or(15)
            .clamp(1, 50),
    );
    if req.sources.is_none() {
        req.sources = Some(vec![get("domain_preset")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "auto".to_string())]);
    }
    if let Some(sources) = &mut req.sources {
        let mut expanded = Vec::new();
        for source in sources.iter() {
            if source == "custom" {
                expanded.extend(
                    get("enabled_sources")
                        .unwrap_or_default()
                        .split(',')
                        .map(|value| value.trim().to_lowercase())
                        .filter(|value| !value.is_empty()),
                );
            } else {
                expanded.push(source.clone());
            }
        }
        *sources = expanded;
    }
}

fn cached_search(db: &Database, key: &str, ttl: u64) -> Option<SearchResponse> {
    db.get_cache(key, ttl).filter(|response| {
        !response
            .sources
            .iter()
            .any(|source| source.queried && !source.ok)
            || db.get_cache(key, ttl.min(30)).is_some()
    })
}

fn search_cache_key(
    req: &SearchRequest,
    creds: &crate::models::SourceCredentials,
    timeout: u64,
) -> String {
    let mut candidates = req.clone();
    candidates.limit = None;
    candidates.offset = None;
    candidates.workspace_id = None;
    // Hash credentials rather than storing them. A key/session change selects a
    // new cache entry, including when an older in-flight search completes later.
    let encoded = serde_json::to_vec(&(
        "search-schema-v8",
        crate::query::ADAPTER_VERSION,
        timeout,
        candidates,
        creds,
    ))
    .expect("search cache inputs are serializable");
    content_hash(&encoded)
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

async fn query_preview_handler(
    State(state): State<AppState>,
    Json(payload): Json<QueryPreviewRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let query = payload.query.trim().to_string();
    if query.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "query is required" })),
        );
    }

    let mut request = SearchRequest {
        query: query.clone(),
        sources: payload.sources,
        limit: None,
        year_min: None,
        year_max: None,
        open_access_only: None,
        offset: None,
        searxng_url: None,
        searxng_categories: None,
        searxng_engines: None,
        workspace_id: None,
    };
    apply_search_defaults(&mut request, |key| state.db.get_config(key));
    let resolved = crate::engine::resolve_sources(request.sources.as_deref(), &query);
    let catalog = crate::catalog::catalog();
    let selected: Vec<&str> = catalog
        .sources
        .iter()
        .filter(|source| source.available)
        .filter(|source| {
            resolved
                .as_ref()
                .is_none_or(|ids| ids.iter().any(|id| id.eq_ignore_ascii_case(&source.id)))
        })
        .map(|source| source.id.as_str())
        .collect();
    let invalid = resolved.as_ref().is_some_and(|ids| {
        ids.iter().any(|id| {
            !catalog
                .sources
                .iter()
                .any(|source| source.available && source.id.eq_ignore_ascii_case(id))
        })
    });
    if invalid {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "unknown or unavailable source" })),
        );
    }
    let body = QueryPreviewResponse {
        query: query.clone(),
        adapter_version: crate::query::ADAPTER_VERSION,
        sources: selected
            .into_iter()
            .map(|source| crate::query::preview(source, &query))
            .collect(),
    };
    (
        StatusCode::OK,
        Json(serde_json::to_value(body).unwrap_or_default()),
    )
}

#[cfg(test)]
mod search_settings_tests {
    use super::*;

    #[test]
    fn partial_search_cache_expires_before_successful_searches() {
        let db = Database::in_memory().unwrap();
        let mut response: SearchResponse = serde_json::from_value(serde_json::json!({
            "query":"fixture", "total":0, "elapsed_ms":1, "cache_hit":false, "papers":[],
            "sources":[{"id":"arxiv", "name":"arXiv", "queried":true, "ok":false, "count":0, "error":"HTTP 429"}]
        })).unwrap();
        db.set_cache("partial", "fixture", &response);
        assert!(cached_search(&db, "partial", 3600).is_some());
        response.sources[0].ok = true;
        response.sources[0].error = None;
        db.set_cache("success", "fixture", &response);
        db.conn
            .lock()
            .unwrap()
            .execute("UPDATE search_cache SET created_at = created_at - 31", [])
            .unwrap();
        assert!(cached_search(&db, "partial", 3600).is_none());
        assert!(cached_search(&db, "success", 3600).is_some());
    }

    #[tokio::test]
    async fn query_preview_reports_source_specific_syntax() {
        let state = AppState {
            db: Database::in_memory().unwrap(),
            engine: Arc::new(AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server =
            tokio::spawn(
                async move { axum::serve(listener, gateway_router(state)).await.unwrap() },
            );
        let query = r#""Heart Failure"[MeSH Terms] AND therapy[tiab] NOT animals[mh]"#;
        let response: serde_json::Value = reqwest::Client::new()
            .post(format!("{base}/api/query/preview"))
            .json(&serde_json::json!({
                "query": query,
                "sources": ["pubmed", "europe_pmc", "arxiv", "semantic_scholar"]
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let items = response["sources"].as_array().unwrap();
        let find = |id: &str| items.iter().find(|item| item["id"] == id).unwrap();
        assert_eq!(find("pubmed")["query"], query);
        assert_eq!(find("pubmed")["mode"], "pubmed_mesh");
        assert!(find("europe_pmc")["query"]
            .as_str()
            .unwrap()
            .contains("MESH:"));
        assert!(find("arxiv")["query"].as_str().unwrap().contains("ANDNOT"));
        assert_eq!(
            find("semantic_scholar")["query"],
            "\"Heart Failure\" therapy"
        );
        server.abort();
    }

    #[tokio::test]
    async fn fetch_pdf_separates_blocked_landing_pages_and_real_pdfs() {
        use axum::{http::StatusCode, routing::get as route_get};
        let app = Router::new()
            .route("/blocked", route_get(|| async { StatusCode::FORBIDDEN }))
            .route(
                "/landing",
                route_get(|| async { ([("content-type", "text/html")], "<html>login</html>") }),
            )
            .route("/paper.pdf", route_get(|| async { "%PDF-1.4\nfixture\n%%EOF" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::new();

        assert!(matches!(
            fetch_pdf(&client, &format!("{base}/blocked")).await,
            Err(PdfFetchError::Blocked(403))
        ));
        assert!(matches!(
            fetch_pdf(&client, &format!("{base}/landing")).await,
            Err(PdfFetchError::NotPdf(detail)) if detail.contains("web page")
        ));
        assert!(fetch_pdf(&client, &format!("{base}/paper.pdf"))
            .await
            .is_ok_and(|bytes| bytes.starts_with(b"%PDF")));
        server.abort();
    }

    #[tokio::test]
    async fn downloaded_pdf_content_is_served_only_from_recorded_history() {
        let path = std::env::temp_dir().join(format!(
            "scholargate-pdf-content-{}.pdf",
            uuid::Uuid::new_v4()
        ));
        let fixture = b"%PDF-1.4\nScholarGate PDF fixture\n%%EOF";
        tokio::fs::write(&path, fixture).await.unwrap();

        let db = Database::in_memory().unwrap();
        db.add_download_record(&DownloadRecord {
            id: "download-1".into(),
            paper_id: "paper-1".into(),
            title: "Recorded paper".into(),
            pdf_url: "https://example.org/paper.pdf".into(),
            local_path: path.to_string_lossy().into_owned(),
            file_size_bytes: fixture.len() as u64,
            source: Some("fixture".into()),
            year: Some(2026),
            downloaded_at: 1,
            workspace_id: None,
        });
        let state = AppState {
            db,
            engine: Arc::new(AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server =
            tokio::spawn(
                async move { axum::serve(listener, gateway_router(state)).await.unwrap() },
            );

        let response = reqwest::get(format!("{base}/api/downloads/download-1/content"))
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            response.headers()[reqwest::header::CONTENT_TYPE],
            "application/pdf"
        );
        assert_eq!(response.bytes().await.unwrap().as_ref(), fixture);
        assert_eq!(
            reqwest::get(format!("{base}/api/downloads/not-recorded/content"))
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::NOT_FOUND
        );

        server.abort();
        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn scoped_history_delete_preserves_other_workspaces() {
        let db = Database::in_memory().unwrap();
        for (id, workspace_id) in [("a", Some("A")), ("b", Some("B")), ("legacy", None)] {
            db.add_search_history(&SearchHistoryItem {
                id: id.into(),
                query: id.into(),
                sources: None,
                result_count: 0,
                elapsed_ms: 0,
                created_at: 1,
                workspace_id: workspace_id.map(str::to_string),
                saved: false,
            });
        }
        let state = AppState {
            db: db.clone(),
            engine: Arc::new(AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server =
            tokio::spawn(
                async move { axum::serve(listener, gateway_router(state)).await.unwrap() },
            );
        let client = reqwest::Client::new();
        for workspace in ["missing", "A"] {
            assert!(client
                .delete(format!(
                    "{base}/api/history/searches?workspace_id={workspace}"
                ))
                .send()
                .await
                .unwrap()
                .status()
                .is_success());
        }
        let remaining = db.get_search_history(10, None);
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().any(|item| item.id == "b"));
        assert!(remaining.iter().any(|item| item.id == "legacy"));
        // Preserve the explicitly global API operation for existing clients.
        assert!(client
            .delete(format!("{base}/api/history/searches"))
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        assert!(db.get_search_history(10, None).is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn source_check_probes_the_source_itself_and_never_answers_from_cache() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let source = Router::new().route(
            "/search",
            get(move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Json(
                        serde_json::json!({"results": (0..3).map(|i| serde_json::json!({
                    "title": format!("Probe result {i}"),
                    "url": format!("https://example.org/{i}.pdf"),
                    "publishedDate": "2024-01-01"
                })).collect::<Vec<_>>()}),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_url = format!("http://{}", listener.local_addr().unwrap());
        let upstream = tokio::spawn(async move { axum::serve(listener, source).await.unwrap() });
        let db = Database::in_memory().unwrap();
        db.set_config("searxng_enabled", "true").unwrap();
        db.set_config("searxng_url", &source_url).unwrap();
        let state = AppState {
            db: db.clone(),
            engine: Arc::new(AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server =
            tokio::spawn(
                async move { axum::serve(listener, gateway_router(state)).await.unwrap() },
            );
        let client = reqwest::Client::new();

        let first: serde_json::Value = client
            .post(format!("{base}/api/source/check"))
            .json(&serde_json::json!({"id": "metasearch"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(first["ok"], true);
        assert_eq!(first["count"], 3);
        assert_eq!(first["needs_setup"], false);
        // The default probe query is filled in for the caller.
        assert!(first["query"]
            .as_str()
            .is_some_and(|query| !query.is_empty()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Same source, same query: a cache hit here would report health without
        // ever contacting the source, so the second check must call it again.
        let second: serde_json::Value = client
            .post(format!("{base}/api/source/check"))
            .json(&serde_json::json!({"id": "metasearch"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(second["ok"], true);
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        // A source needing a credential is reported as such, not as a failure.
        let keyed: serde_json::Value = client
            .post(format!("{base}/api/source/check"))
            .json(&serde_json::json!({"id": "scopus"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(keyed["needs_setup"], true);
        assert_eq!(keyed["ok"], false);

        for id in ["not_a_source", "medpharmres", ""] {
            let response = client
                .post(format!("{base}/api/source/check"))
                .json(&serde_json::json!({"id": id}))
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                reqwest::StatusCode::BAD_REQUEST,
                "accepted {id}"
            );
        }
        server.abort();
        upstream.abort();
    }

    #[tokio::test]
    async fn single_source_pages_share_candidates_and_credential_changes_bypass_cache() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let source = Router::new().route(
            "/search",
            get(move || {
                let counter = counter.clone();
                async move {
                    let generation = counter.fetch_add(1, Ordering::SeqCst);
                    Json(
                        serde_json::json!({"results": (0..40).map(|i| serde_json::json!({
                    "title": format!("Candidate {generation} number {i}"),
                    "url": format!("https://example.org/{generation}/{i}.pdf"),
                    "publishedDate": "2024-01-01"
                })).collect::<Vec<_>>()}),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_url = format!("http://{}", listener.local_addr().unwrap());
        let upstream = tokio::spawn(async move { axum::serve(listener, source).await.unwrap() });
        let db = Database::in_memory().unwrap();
        let state = AppState {
            db: db.clone(),
            engine: Arc::new(AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server =
            tokio::spawn(
                async move { axum::serve(listener, gateway_router(state)).await.unwrap() },
            );
        let client = reqwest::Client::new();
        let mut request = serde_json::json!({"query":"pagination fixture", "sources":["metasearch"],
            "limit":15, "searxng_url":source_url, "workspace_id":"A"});
        let mut ids = std::collections::HashSet::new();
        for (offset, expected) in [(0, 15), (15, 15), (30, 10), (40, 0), (1500, 0)] {
            request["offset"] = offset.into();
            let response = client
                .post(format!("{base}/api/search"))
                .json(&request)
                .send()
                .await
                .unwrap();
            assert!(response.status().is_success());
            let page: SearchResponse = response.json().await.unwrap();
            assert_eq!(page.total, expected);
            assert_eq!(page.available_total, 40);
            assert_eq!(page.cache_hit, offset != 0);
            for paper in page.papers {
                assert!(ids.insert(paper.id), "page repeated a paper");
            }
        }
        assert_eq!(ids.len(), 40);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // MCP uses the same cache despite a different page size/workspace.
        request["limit"] = 7.into();
        request["offset"] = 0.into();
        request["workspace_id"] = "B".into();
        // SearXNG connection settings are resolved by MCP from the database.
        db.set_config("searxng_enabled", "true").unwrap();
        db.set_config("searxng_url", &source_url).unwrap();
        let mcp: serde_json::Value = client.post(format!("{base}/mcp"))
            .json(&serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":"search_academic_papers", "arguments":{
                    "query":"pagination fixture", "sources":["metasearch"], "limit":7, "offset":0, "workspace_id":"B"
                }}})).send().await.unwrap().json().await.unwrap();
        let page: SearchResponse =
            serde_json::from_str(mcp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(page.total, 7);
        assert!(page.cache_hit);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // REST settings and the shared desktop settings persistence path both
        // select a fresh pool without manually clearing the cache.
        for (field, value) in [
            ("ncbi_api_key", "new-key"),
            ("consensus_session", "new-session"),
        ] {
            assert!(client
                .post(format!("{base}/api/config"))
                .json(&serde_json::json!({field:value}))
                .send()
                .await
                .unwrap()
                .status()
                .is_success());
            let page: SearchResponse = client
                .post(format!("{base}/api/search"))
                .json(&request)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert!(!page.cache_hit);
            assert_eq!(page.total, 7);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        db.set_config_patch(&std::collections::BTreeMap::from([(
            "ncbi_api_key".into(),
            "".into(),
        )]))
        .unwrap();
        let page: SearchResponse = client
            .post(format!("{base}/api/search"))
            .json(&request)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(!page.cache_hit);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        server.abort();
        upstream.abort();
    }

    #[tokio::test]
    async fn workspace_rest_round_trip_over_tcp() {
        let app = gateway_router(AppState {
            db: crate::db::Database::in_memory().unwrap(),
            engine: Arc::new(AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::new();

        // A default workspace is provisioned and listed.
        let list: serde_json::Value = client
            .get(format!("{base}/api/workspaces"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);

        // Create a project.
        let created: serde_json::Value = client
            .post(format!("{base}/api/workspaces"))
            .json(&serde_json::json!({"name": "Ung thư phổi", "description": "review"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();

        // Add a paper with a note, then read it back.
        let paper = serde_json::json!({"id":"p1","title":"Paper","authors":["A"],"source":"OpenAlex","open_access":false});
        assert!(client
            .post(format!("{base}/api/workspaces/{id}/papers"))
            .json(&serde_json::json!({"paper": paper, "note": "đọc kỹ phần phương pháp"}))
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        let papers: serde_json::Value = client
            .get(format!("{base}/api/workspaces/{id}/papers"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(papers.as_array().unwrap().len(), 1);
        assert_eq!(papers[0]["note"], "đọc kỹ phần phương pháp");
        assert_eq!(papers[0]["paper"]["title"], "Paper");

        // Update the note, then delete the workspace.
        assert!(client
            .patch(format!("{base}/api/workspaces/{id}/papers?paper_id=p1"))
            .json(&serde_json::json!({"note": "cập nhật", "status": "read", "favorite": true, "tags": ["a", "b"]}))
            .send().await.unwrap().status().is_success());
        let papers: serde_json::Value = client
            .get(format!("{base}/api/workspaces/{id}/papers"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(papers[0]["note"], "cập nhật");
        assert_eq!(papers[0]["status"], "read");
        assert_eq!(papers[0]["favorite"], true);
        assert_eq!(papers[0]["tags"][1], "b");
        assert!(client
            .delete(format!("{base}/api/workspaces/{id}"))
            .send()
            .await
            .unwrap()
            .status()
            .is_success());

        server.abort();
    }

    #[tokio::test]
    async fn interest_library_rest_round_trip_over_tcp() {
        let app = gateway_router(AppState {
            db: crate::db::Database::in_memory().unwrap(),
            engine: Arc::new(AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::new();
        let paper = serde_json::json!({"id":"interest-1","title":"Interesting paper","authors":[],"source":"Crossref","open_access":false});

        assert!(client
            .post(format!("{base}/api/library"))
            .json(&serde_json::json!({"paper":paper}))
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        let items: serde_json::Value = client
            .get(format!("{base}/api/library"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(items.as_array().unwrap().len(), 1);
        assert_eq!(items[0]["paper"]["id"], "interest-1");

        assert!(client
            .patch(format!("{base}/api/library?paper_id=interest-1"))
            .json(&serde_json::json!({"note":"review later","status":"reading"}))
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        let items: serde_json::Value = client
            .get(format!("{base}/api/library"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(items[0]["note"], "review later");
        assert_eq!(items[0]["status"], "reading");

        assert!(client
            .delete(format!("{base}/api/library?paper_id=interest-1"))
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        let items: serde_json::Value = client
            .get(format!("{base}/api/library"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(items.as_array().unwrap().is_empty());
        server.abort();
    }

    #[test]
    fn resolves_changed_settings_and_explicit_empty_sources() {
        let request: SearchRequest =
            serde_json::from_value(serde_json::json!({"query":"education"})).unwrap();
        let mut economics = request.clone();
        apply_search_defaults(&mut economics, |key| match key {
            "domain_preset" => Some("economics".into()),
            "max_results_default" => Some("23".into()),
            _ => None,
        });
        assert_eq!(economics.sources, Some(vec!["economics".into()]));
        assert_eq!(economics.limit, Some(23));
        let mut custom = request;
        apply_search_defaults(&mut custom, |key| match key {
            "domain_preset" => Some("custom".into()),
            "enabled_sources" => Some(String::new()),
            _ => None,
        });
        assert_eq!(custom.sources, Some(vec![]));
        assert_ne!(
            serde_json::to_string(&custom).unwrap(),
            serde_json::to_string(&economics).unwrap()
        );
        custom.limit = Some(7);
        apply_search_defaults(&mut custom, |_| Some("all".into()));
        assert_eq!(custom.sources, Some(vec![]));
        assert_eq!(custom.limit, Some(7));
    }
}

// Handler 2: Search papers
/// A query the source can plausibly answer, so that "0 results" stays rare
/// enough for the count to mean something. Specialised records (a protein, a
/// CVE) need a matching term; everything else is grouped by discipline.
fn probe_query(source: &crate::catalog::Source) -> &'static str {
    match source.id.as_str() {
        "uniprot" => "insulin",
        "openfda" => "aspirin",
        "clinvar" => "BRCA1",
        "ncbi_geo" => "breast cancer",
        "cve_nvd" | "cisa_kev" => "openssl",
        "sec_edgar" => "annual report",
        "stackexchange" => "python",
        "hf_datasets" | "huggingface" => "language model",
        "eric" => "education",
        "world_bank" => "poverty",
        _ => match source.group.as_str() {
            "Vietnamese Journals" => "nghiên cứu",
            "Biomedical & Clinical" => "diabetes",
            "CS, AI & Engineering" => "machine learning",
            "Physics & Natural Sciences" => "quantum",
            "Economics & Social Sciences" => "economic growth",
            _ => "climate change",
        },
    }
}

/// Probe one source and report what actually came back. This deliberately skips
/// the response cache: a health check that answers from a cached page would say
/// a source is healthy without contacting it.
async fn source_check_handler(
    State(state): State<AppState>,
    Json(payload): Json<SourceCheckRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let id = payload.id.trim().to_lowercase();
    let Some(source) = crate::catalog::catalog()
        .sources
        .iter()
        .find(|source| source.id == id && source.available)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Unknown or unsupported source" })),
        );
    };
    let query = payload
        .query
        .map(|query| query.trim().to_string())
        .filter(|query| !query.is_empty())
        .unwrap_or_else(|| probe_query(source).to_string());

    crate::engine::reset_source_health(&id).await;

    let mut req = SearchRequest {
        query: query.clone(),
        sources: Some(vec![id.clone()]),
        limit: Some(5),
        year_min: None,
        year_max: None,
        open_access_only: None,
        offset: None,
        searxng_url: None,
        searxng_categories: None,
        searxng_engines: None,
        workspace_id: None,
    };
    if state.db.get_config("searxng_enabled").as_deref() == Some("true") {
        req.searxng_url = state.db.get_config("searxng_url");
        req.searxng_categories = state.db.get_config("searxng_categories");
        req.searxng_engines = state.db.get_config("searxng_engines");
    }

    let timeout = crate::config::search_timeout(&state.db);
    let creds = state.db.source_credentials();
    let started = Instant::now();
    let proxy = crate::config::outbound_proxy(&state.db);
    let custom_engine;
    let engine = if timeout == 12 && proxy.is_none() {
        state.engine.as_ref()
    } else {
        custom_engine = AcademicEngine::with_options(timeout, proxy.as_deref());
        &custom_engine
    };
    let response = Box::pin(engine.search_candidates(&req, &creds)).await;
    let elapsed = started.elapsed().as_millis() as u64;

    let status = response.sources.iter().find(|status| status.id == id);
    let body = match status {
        Some(status) => SourceCheckResponse {
            id: id.clone(),
            name: source.name.clone(),
            ok: status.ok,
            count: status.count,
            elapsed_ms: elapsed,
            query,
            needs_setup: status.needs_setup,
            error: status.error.clone(),
        },
        // Every selected source reports a status; if one ever does not, say so
        // instead of presenting silence as a pass.
        None => SourceCheckResponse {
            id: id.clone(),
            name: source.name.clone(),
            ok: false,
            count: 0,
            elapsed_ms: elapsed,
            query,
            needs_setup: false,
            error: Some("The engine did not query this source".to_string()),
        },
    };
    (
        StatusCode::OK,
        Json(serde_json::to_value(body).unwrap_or_default()),
    )
}

/// Runs a search: validation, settings resolution, cache, fan-out, telemetry.
///
/// Transport-free on purpose. REST and MCP are two front doors onto the same
/// search, and MCP used to reach it by calling the Axum handler with hand-built
/// `State`/`Json` wrappers — which meant the HTTP extractor types leaked into
/// the MCP layer for no reason. Both now call this.
pub(crate) async fn search_service(
    state: &AppState,
    headers: &HeaderMap,
    payload: SearchRequest,
) -> (StatusCode, SearchResponse) {
    let started = Instant::now();
    let principal = crate::agents::authenticate(&state.db, headers);
    let agent_name = match &principal {
        Ok(crate::agents::Principal::Agent(grant)) => format!("Agent · {}", grant.name),
        _ => client_label(&headers),
    };
    let record_history = match &principal {
        Ok(crate::agents::Principal::Agent(grant)) => {
            grant.writable && payload.workspace_id.is_some()
        }
        _ => true,
    };

    let now_sec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut search_req = payload.clone();
    search_req.query = search_req.query.trim().to_string();
    apply_search_defaults(&mut search_req, |key| state.db.get_config(key));
    // Reject unknown sources and catalog entries the engine cannot query, so a
    // selection never silently returns nothing.
    let invalid_source = search_req.sources.as_ref().is_some_and(|sources| {
        let catalog = crate::catalog::catalog();
        sources.iter().any(|raw| {
            let id = raw.trim().to_lowercase();
            if id.is_empty() || ["all", "international", "auto", "custom"].contains(&id.as_str()) {
                return false;
            }
            if catalog.presets.iter().any(|preset| preset.id == id) {
                return false;
            }
            match catalog.sources.iter().find(|source| source.id == id) {
                Some(source) => !source.available,
                None => true,
            }
        })
    });
    if invalid_source
        || search_req.query.is_empty()
        || matches!((search_req.year_min, search_req.year_max), (Some(min), Some(max)) if min > max)
        || search_req
            .year_min
            .is_some_and(|year| !(1000..=9999).contains(&year))
        || search_req
            .year_max
            .is_some_and(|year| !(1000..=9999).contains(&year))
        || search_req.offset.is_some_and(|offset| offset > 10_000)
    {
        return (
            StatusCode::BAD_REQUEST,
            SearchResponse {
                query: String::new(),
                total: 0,
                available_total: 0,
                elapsed_ms: 0,
                cache_hit: false,
                papers: vec![],
                sources: vec![],
            },
        );
    }

    // Direct search via AcademicEngine (inject SearXNG URL if enabled)
    if search_req.searxng_url.is_none() {
        if state.db.get_config("searxng_enabled").as_deref() == Some("true") {
            search_req.searxng_url = state.db.get_config("searxng_url");
            search_req.searxng_categories = state.db.get_config("searxng_categories");
            search_req.searxng_engines = state.db.get_config("searxng_engines");
        }
    }

    // Resolve settings before hashing so changing enabled sources or SearXNG
    // configuration can never reuse a response from the previous configuration.
    let timeout = crate::config::search_timeout(&state.db);
    let creds = state.db.source_credentials();
    let query_hash = search_cache_key(&search_req, &creds, timeout);
    let offset = search_req.offset.unwrap_or(0);
    let limit = search_req.limit.unwrap_or(15);
    let cache_ttl_seconds = state
        .db
        .get_config("cache_ttl_hours")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(24)
        .clamp(1, 720)
        * 3600;
    let cached = cached_search(&state.db, &query_hash, cache_ttl_seconds);
    if let Some(cached) = cached {
        let mut cached = cached.into_page(offset, limit);
        let elapsed = started.elapsed().as_millis() as u64;
        cached.elapsed_ms = elapsed;
        state.db.log_agent_query(&AgentLog {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            agent_name,
            method: "POST /api/search".to_string(),
            query: search_req.query.clone(),
            result_count: cached.papers.len(),
            latency_ms: elapsed,
            status: "Cache Hit (200 OK)".to_string(),
        });
        if record_history {
            state.db.add_search_history(&SearchHistoryItem {
                id: uuid::Uuid::new_v4().to_string(),
                query: search_req.query.clone(),
                sources: search_req
                    .sources
                    .as_ref()
                    .map(|sources| sources.join(", ")),
                result_count: cached.papers.len(),
                elapsed_ms: elapsed,
                created_at: now_sec,
                workspace_id: search_req.workspace_id.clone(),
                saved: false,
            });
        }
        return (StatusCode::OK, cached);
    }

    // Cache misses fan out to third-party APIs. Keep rapid UI/MCP requests from
    // starting overlapping search storms while allowing cached pagination now.
    wait_for_search_slot(crate::config::search_delay(&state.db)).await;

    let proxy = crate::config::outbound_proxy(&state.db);
    let custom_engine;
    let engine = if timeout == 12 && proxy.is_none() {
        state.engine.as_ref()
    } else {
        custom_engine = AcademicEngine::with_options(timeout, proxy.as_deref());
        &custom_engine
    };
    // The fan-out future is large (one branch per source); box it so it lives on
    // the heap instead of the caller's stack.
    let mut resp = Box::pin(engine.search_candidates(&search_req, &creds)).await;
    let elapsed = started.elapsed().as_millis() as u64;
    resp.elapsed_ms = elapsed;

    // Partial candidate pools keep pagination stable, but expire after 30s above.
    state.db.set_cache(&query_hash, &search_req.query, &resp);
    let resp = resp.into_page(offset, limit);

    // Log query for Agent Telemetry
    state.db.log_agent_query(&AgentLog {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        agent_name,
        method: "POST /api/search".to_string(),
        query: search_req.query.clone(),
        result_count: resp.papers.len(),
        latency_ms: elapsed,
        status: "Success (200 OK)".to_string(),
    });

    // Record search history
    if record_history {
        state.db.add_search_history(&SearchHistoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            query: search_req.query.clone(),
            sources: search_req.sources.as_ref().map(|s| s.join(", ")),
            result_count: resp.papers.len(),
            elapsed_ms: elapsed,
            created_at: now_sec,
            workspace_id: search_req.workspace_id.clone(),
            saved: false,
        });
    }

    (StatusCode::OK, resp)
}

/// REST front door for [`search_service`].
pub(crate) async fn search_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<SearchRequest>,
) -> (StatusCode, Json<SearchResponse>) {
    let (status, response) = search_service(&state, &headers, payload).await;
    (status, Json(response))
}

// Handler 3: Get paper by ID
async fn paper_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match crate::details::lookup(&state, &id).await {
        Ok(paper) => (StatusCode::OK, Json(serde_json::to_value(paper).unwrap())),
        Err((status, message)) => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(serde_json::json!({"error":message})),
        ),
    }
}

// Handler 3b: Citation graph (references / cited_by) via OpenAlex
async fn citations_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(params): Query<CitationsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let direction = params.direction.as_deref().unwrap_or("cited_by");
    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    match crate::citations::lookup(&state, &id, direction, limit).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err((status, message)) => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(serde_json::json!({"error": message})),
        ),
    }
}

async fn citations_query_handler(
    State(state): State<AppState>,
    Query(params): Query<CitationsByIdQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let direction = params.direction.as_deref().unwrap_or("cited_by");
    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    match crate::citations::lookup(&state, &params.id, direction, limit).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err((status, message)) => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(serde_json::json!({"error": message})),
        ),
    }
}

// Handler 4: Download PDF
async fn download_handler(
    State(state): State<AppState>,
    Json(payload): Json<DownloadRequest>,
) -> Json<DownloadResponse> {
    let download_dir = match crate::config::download_directory(&state.db) {
        Ok(path) => path,
        Err(error) => {
            return Json(DownloadResponse {
                success: false,
                local_path: None,
                file_size_bytes: None,
                blocked: false,
                error: Some(error),
            })
        }
    };

    let clean_title: String = payload
        .title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let title = if clean_title.trim().is_empty() {
        "paper"
    } else {
        clean_title.trim()
    };
    let hash_key = if payload.pdf_url.trim().is_empty() { &payload.paper_id } else { &payload.pdf_url };
    let url_hash = content_hash(hash_key.as_bytes());
    let file_name = format!("{}_{}.pdf", title, &url_hash[..8]);
    let target_file = download_dir.join(&file_name);

    // `reqwest::get` sends no user agent and has no timeout, and publishers
    // routinely answer an anonymous request with 403 — every PDF from Europe
    // PMC failed that way. Use the same identity the searches use, honour the
    // configured proxy, and allow far longer than a search: a PDF is a file,
    // not a metadata call.
    let downloader = crate::engine::pooled_client(120, crate::config::outbound_proxy(&state.db).as_deref());
    let failure = |blocked: bool, error: String| {
        Json(DownloadResponse {
            success: false,
            local_path: None,
            file_size_bytes: None,
            blocked,
            error: Some(error),
        })
    };

    let mut tried: Vec<String> = Vec::new();
    let mut first_error: Option<PdfFetchError> = None;
    let mut fetched: Option<(String, Vec<u8>)> = None;
    if !payload.pdf_url.trim().is_empty() {
        let url = payload.pdf_url.trim().to_string();
        tried.push(url.clone());
        match fetch_pdf(&downloader, &url).await {
            Ok(bytes) => fetched = Some((url, bytes)),
            Err(error) => first_error = Some(error),
        }
    }
    // The advertised link is often behind bot protection while a repository
    // copy (PMC, arXiv, an institutional archive) is freely downloadable.
    if fetched.is_none() {
        if let Some(doi) = payload.doi.as_deref().and_then(crate::details::normalize_doi) {
            for url in alternate_pdf_urls(&state, &downloader, &doi).await {
                if tried.contains(&url) {
                    continue;
                }
                tried.push(url.clone());
                match fetch_pdf(&downloader, &url).await {
                    Ok(bytes) => {
                        fetched = Some((url, bytes));
                        break;
                    }
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
        }
    }

    let Some((pdf_url, bytes)) = fetched else {
        return match first_error {
            Some(PdfFetchError::Blocked(status)) => failure(
                true,
                format!(
                    "The publisher blocked this download (HTTP {status}) and no other open-access copy could be downloaded. Open the article page to read it there."
                ),
            ),
            Some(PdfFetchError::Status(status)) => failure(false, format!("Source returned HTTP {status}")),
            Some(PdfFetchError::NotPdf(detail)) => {
                failure(false, format!("Full text could not be downloaded: {detail}"))
            }
            Some(PdfFetchError::Network(error)) => failure(false, error),
            None => failure(false, "No downloadable open-access PDF was found for this paper.".to_string()),
        };
    };

    let file_size = bytes.len() as u64;
    if std::fs::write(&target_file, bytes).is_err() {
        return failure(false, "Failed to write PDF to disk".to_string());
    }
    let now_sec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    state.db.add_download_record(&DownloadRecord {
        id: uuid::Uuid::new_v4().to_string(),
        paper_id: payload.paper_id.clone(),
        title: payload.title.clone(),
        pdf_url,
        local_path: target_file.to_string_lossy().to_string(),
        file_size_bytes: file_size,
        source: payload.source.clone(),
        year: payload.year,
        downloaded_at: now_sec,
        workspace_id: payload.workspace_id.clone(),
    });
    Json(DownloadResponse {
        success: true,
        local_path: Some(target_file.to_string_lossy().to_string()),
        file_size_bytes: Some(file_size),
        blocked: false,
        error: None,
    })
}

enum PdfFetchError {
    /// Publishers put bot protection in front of many PDF links, so this is a
    /// routine outcome rather than a fault.
    Blocked(u16),
    Status(u16),
    NotPdf(&'static str),
    Network(String),
}

async fn fetch_pdf(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, PdfFetchError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| PdfFetchError::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(if matches!(status.as_u16(), 401 | 402 | 403 | 429 | 451) {
            PdfFetchError::Blocked(status.as_u16())
        } else {
            PdfFetchError::Status(status.as_u16())
        });
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| PdfFetchError::Network(e.to_string()))?;
    // Paywalled links answer 200 with an HTML landing page; writing that to a
    // .pdf would report a successful download of a file no reader can open.
    if !bytes.starts_with(b"%PDF") {
        return Err(PdfFetchError::NotPdf(if content_type.contains("html") {
            "the source returned a web page (possibly a login or paywall page) instead of a PDF"
        } else {
            "the downloaded content is not a PDF"
        }));
    }
    Ok(bytes.to_vec())
}

/// Other open-access PDF copies of a DOI, from OpenAlex and (when an email is
/// configured) Unpaywall, best first.
async fn alternate_pdf_urls(state: &AppState, client: &reqwest::Client, doi: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let openalex = format!("https://api.openalex.org/works/https://doi.org/{}", urlencoding::encode(doi));
    if let Some(json) = get_json(client, &openalex).await {
        for location in std::iter::once(&json["best_oa_location"])
            .chain(json["locations"].as_array().into_iter().flatten())
        {
            if let Some(url) = location["pdf_url"].as_str().map(str::trim).filter(|u| !u.is_empty()) {
                urls.push(url.to_string());
            }
        }
    }
    let creds = state.db.source_credentials();
    if let Some(email) = crate::models::SourceCredentials::clean(creds.unpaywall_email) {
        let unpaywall = format!(
            "https://api.unpaywall.org/v2/{}?email={}",
            urlencoding::encode(doi),
            urlencoding::encode(&email)
        );
        if let Some(json) = get_json(client, &unpaywall).await {
            urls.extend(crate::engine::unpaywall_pdf_urls(&json));
        }
    }
    let mut seen = std::collections::HashSet::new();
    urls.retain(|url| seen.insert(url.clone()));
    urls.truncate(6);
    urls
}

async fn get_json(client: &reqwest::Client, url: &str) -> Option<serde_json::Value> {
    client
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

/// Streams only files recorded by ScholarGate, never an arbitrary caller-supplied path.
async fn download_content_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let record = state
        .db
        .get_download_record(&id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let bytes = tokio::fs::read(&record.local_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !bytes.starts_with(b"%PDF") {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let file_name: String = record
        .title
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let disposition = format!("inline; filename=\"{}.pdf\"", file_name.trim());
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(header::CACHE_CONTROL, "private, no-store")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition)
                .unwrap_or_else(|_| HeaderValue::from_static("inline")),
        )
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// Handler 5: Agent Telemetry
async fn telemetry_handler(State(state): State<AppState>) -> Json<crate::models::TelemetryStats> {
    let stats = state.db.get_telemetry_stats(state.port);
    Json(stats)
}

// Live daily search topics from the public Google Trends RSS feed. The
// interactive 5-year chart stays on Google Trends because its private widget
// endpoint is intentionally rate-limited and not a stable application API.
async fn trends_handler(
    Query(params): Query<TrendsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let requested_geo = params
        .geo
        .unwrap_or_else(|| "VN".to_string())
        .to_uppercase();
    let geo = match requested_geo.as_str() {
        "VN" | "US" | "GB" | "SG" | "AU" => requested_geo,
        _ => "VN".to_string(),
    };
    let url = format!("https://trends.google.com/trending/rss?geo={}", geo);
    let response = match reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_default()
        .get(&url)
        .header("Accept", "application/rss+xml, application/xml, text/xml")
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(
                    serde_json::json!({ "success": false, "error": format!("Google Trends returned HTTP {}", response.status().as_u16()) }),
                ),
            )
        }
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "success": false, "error": error.to_string() })),
            )
        }
    };
    let xml = match response.text().await {
        Ok(xml) => xml,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "success": false, "error": error.to_string() })),
            )
        }
    };

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut in_item = false;
    let mut current_tag = String::new();
    let mut title = String::new();
    let mut traffic: Option<String> = None;
    let mut published_at: Option<String> = None;
    let mut items: Vec<TrendItem> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(event)) => {
                let name = event.name().as_ref().to_string();
                if name == "item" {
                    in_item = true;
                    title.clear();
                    traffic = None;
                    published_at = None;
                }
                current_tag = name;
            }
            Ok(quick_xml::events::Event::Text(event)) if in_item => {
                let value = event.xml10_content().trim().to_string();
                match current_tag.as_str() {
                    "title" => title = value,
                    "ht:approx_traffic" => traffic = Some(value),
                    "pubDate" => published_at = Some(value),
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::End(event)) => {
                let name = event.name().as_ref().to_string();
                if name == "item" {
                    in_item = false;
                    if !title.is_empty() {
                        items.push(TrendItem {
                            explore_url: format!(
                                "https://trends.google.com/trends/explore?geo={}&q={}",
                                geo,
                                urlencoding::encode(&title)
                            ),
                            title: title.clone(),
                            traffic: traffic.clone(),
                            published_at: published_at.clone(),
                        });
                    }
                }
                current_tag.clear();
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(
                        serde_json::json!({ "success": false, "error": format!("Google Trends data could not be read: {}", error) }),
                    ),
                )
            }
            _ => {}
        }
    }
    (
        StatusCode::OK,
        Json(
            serde_json::json!({ "success": true, "geo": geo, "items": items.into_iter().take(20).collect::<Vec<_>>() }),
        ),
    )
}

// Handler 6: Search History (optionally scoped to a workspace)
async fn get_search_history_handler(
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> Json<Vec<SearchHistoryItem>> {
    let history = state
        .db
        .get_search_history(100, params.workspace_id.as_deref());
    Json(history)
}

// ---- Interest library ----------------------------------------------------

async fn list_library_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.library_papers() {
        Ok(papers) => (
            StatusCode::OK,
            Json(serde_json::to_value(papers).unwrap_or_default()),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn add_library_paper_handler(
    State(state): State<AppState>,
    Json(payload): Json<WorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.add_library_paper(&payload.paper) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn update_library_paper_handler(
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
    Json(payload): Json<UpdateWorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.update_library_paper(
        &params.paper_id,
        payload.note.as_deref(),
        payload.status.as_deref(),
        payload.favorite,
        payload.tags.as_deref(),
    ) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn remove_library_paper_handler(
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.remove_library_paper(&params.paper_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

// ---- Legacy workspaces (kept for existing MCP clients) ------------------

async fn list_workspaces_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Json<Vec<Workspace>> {
    let mut workspaces = state.db.list_workspaces();
    if let Ok(crate::agents::Principal::Agent(grant)) =
        crate::agents::authenticate(&state.db, &headers)
    {
        workspaces.retain(|w| crate::agents::workspace_allowed(&grant, &w.id, false));
    }
    Json(workspaces)
}

async fn create_workspace_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateWorkspaceRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .db
        .create_workspace(&payload.name, payload.description.as_deref())
    {
        Ok(workspace) => (
            StatusCode::OK,
            Json(serde_json::to_value(workspace).unwrap()),
        ),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn update_workspace_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<UpdateWorkspaceRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(existing) = state
        .db
        .list_workspaces()
        .into_iter()
        .find(|workspace| workspace.id == id)
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Workspace not found" })),
        );
    };
    let name = payload.name.unwrap_or(existing.name);
    let description = payload.description.or(existing.description);
    match state
        .db
        .rename_workspace(&id, &name, description.as_deref())
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn delete_workspace_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.delete_workspace(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn list_workspace_papers_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !state.db.workspace_exists(&id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Workspace not found" })),
        );
    }
    match state.db.workspace_papers(&id) {
        Ok(papers) => (StatusCode::OK, Json(serde_json::to_value(papers).unwrap())),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn add_workspace_paper_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<WorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .db
        .add_workspace_paper(&id, &payload.paper, payload.note.as_deref())
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn update_workspace_note_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
    Json(payload): Json<UpdateWorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.update_workspace_paper(
        &id,
        &params.paper_id,
        payload.note.as_deref(),
        payload.status.as_deref(),
        payload.favorite,
        payload.tags.as_deref(),
    ) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn save_search_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<SaveSearchRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.set_search_saved(&id, payload.saved) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

async fn remove_workspace_paper_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.remove_workspace_paper(&id, &params.paper_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

async fn clear_download_history_handler(
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .db
        .clear_download_history(params.workspace_id.as_deref())
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

async fn clear_search_history_handler(
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .db
        .clear_search_history(params.workspace_id.as_deref())
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

async fn delete_search_history_item_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.delete_search_history_item(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

// Handler 7: Download History (optionally scoped to a workspace)
async fn get_download_history_handler(
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> Json<Vec<DownloadRecord>> {
    Json(
        state
            .db
            .get_download_history(params.workspace_id.as_deref()),
    )
}

async fn delete_download_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.delete_download_record(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

// Handler 8: Open File in OS Default App (Preview, Finder, PDF Reader)
async fn open_file_handler(Json(payload): Json<OpenFileRequest>) -> Json<serde_json::Value> {
    let path = std::path::Path::new(&payload.path);
    if path.exists() {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open")
            .arg(&payload.path)
            .spawn();

        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer")
            .arg(&payload.path)
            .spawn();

        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open")
            .arg(&payload.path)
            .spawn();

        Json(serde_json::json!({ "success": true }))
    } else {
        Json(serde_json::json!({ "success": false, "error": "File does not exist on disk" }))
    }
}

// Handler 9: Configuration (LLM Keys & Academic Providers)
async fn get_config_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    // Secrets never leave the process: stored credentials become a sentinel the
    // UI can echo back unchanged.
    if !matches!(
        crate::agents::authenticate(&state.db, &headers),
        Ok(crate::agents::Principal::Local | crate::agents::Principal::Admin)
    ) {
        return Json(serde_json::json!({"authentication_required":true}));
    }
    let config = crate::config::sanitize_config(state.db.get_all_config());
    Json(config)
}

async fn set_config_handler(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let values = match crate::config::validate_patch(payload) {
        Ok(values) => values,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"success":false,"error":error})),
            )
        }
    };
    if let Err(error) = state.db.set_config_patch(&values) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"success":false,"error":error.to_string()})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"success":true})))
}

// Handler 10: Clear Cache
async fn clear_cache_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.clear_cache() {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

fn provider_secret_key(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("openai_api_key"),
        "anthropic" => Some("anthropic_api_key"),
        "gemini" => Some("gemini_api_key"),
        "deepseek" => Some("deepseek_api_key"),
        "groq" => Some("groq_api_key"),
        "perplexity" => Some("perplexity_api_key"),
        _ => None,
    }
}

// Handler 11: Test LLM API Key
async fn test_llm_handler(
    State(state): State<AppState>,
    Json(mut payload): Json<TestLlmRequest>,
) -> Json<TestLlmResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_default();

    let provider = payload.provider.to_lowercase();
    // Blank key means "use the saved credential" — it is never sent to the UI.
    if payload.api_key.is_empty() || payload.api_key == crate::config::KEEP_SECRET {
        payload.api_key = provider_secret_key(&provider)
            .and_then(|key| state.db.get_config(key))
            .unwrap_or_default();
    }
    if payload.api_key.is_empty() {
        return Json(TestLlmResponse {
            success: false,
            latency_ms: 0,
            message: format!("No API key stored for {provider}; save the settings before testing."),
        });
    }

    let started = Instant::now();

    match payload.provider.to_lowercase().as_str() {
        "openai" => {
            let base_url = payload
                .base_url
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let url = format!("{}/models", base_url.trim_end_matches('/'));
            match client.get(&url).bearer_auth(&payload.api_key).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "OpenAI API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("OpenAI returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "anthropic" => {
            let url = "https://api.anthropic.com/v1/messages";
            match client
                .post(url)
                .header("x-api-key", &payload.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": "claude-3-haiku-20240307",
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "ping"}]
                }))
                .send()
                .await
            {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "Anthropic Claude API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Anthropic status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "gemini" => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                payload.api_key
            );
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "Google Gemini API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Gemini returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "deepseek" => {
            let url = "https://api.deepseek.com/models";
            match client.get(url).bearer_auth(&payload.api_key).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "DeepSeek API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("DeepSeek returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "groq" => {
            let url = "https://api.groq.com/openai/v1/models";
            match client.get(url).bearer_auth(&payload.api_key).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    Json(TestLlmResponse {
                        success: resp.status().is_success(),
                        latency_ms: elapsed,
                        message: if resp.status().is_success() {
                            "Groq API key verified successfully!".to_string()
                        } else {
                            format!("Groq returned status: {}", resp.status())
                        },
                    })
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "ollama" => {
            let base_url = payload
                .base_url
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        let configured = payload
                            .model
                            .as_deref()
                            .map(str::trim)
                            .filter(|model| !model.is_empty());
                        let found = match configured {
                            Some(model) => {
                                resp.json::<serde_json::Value>()
                                    .await
                                    .ok()
                                    .is_some_and(|body| {
                                        body["models"].as_array().is_some_and(|models| {
                                            models.iter().any(|item| {
                                                item["name"].as_str().is_some_and(|name| {
                                                    name == model
                                                        || name.strip_suffix(":latest")
                                                            == Some(model)
                                                })
                                            })
                                        })
                                    })
                            }
                            None => true,
                        };
                        Json(TestLlmResponse {
                            success: found,
                            latency_ms: elapsed,
                            message: if found {
                                configured.map_or_else(
                                    || "Local Ollama server is online and responsive.".to_string(),
                                    |model| {
                                        format!(
                                            "Ollama is online and model '{model}' is installed."
                                        )
                                    },
                                )
                            } else {
                                format!(
                                    "Ollama is online, but model '{}' is not installed.",
                                    configured.unwrap()
                                )
                            },
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Ollama returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!(
                        "Could not connect to Ollama (check if running on port 11434): {}",
                        e
                    ),
                }),
            }
        }
        "perplexity" => {
            let url = "https://api.perplexity.ai/chat/completions";
            match client
                .post(url)
                .bearer_auth(&payload.api_key)
                .json(&serde_json::json!({
                    "model": "sonar",
                    "messages": [{"role": "user", "content": "ping"}],
                    "max_tokens": 5
                }))
                .send()
                .await
            {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "Perplexity API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Perplexity returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        _ => Json(TestLlmResponse {
            success: false,
            latency_ms: 0,
            message: "Unsupported provider".to_string(),
        }),
    }
}

// Handler 11b: Test SearXNG Instance
async fn test_searxng_handler(
    Json(payload): Json<crate::models::TestSearxngRequest>,
) -> Json<crate::models::TestSearxngResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .unwrap_or_default();

    let started = Instant::now();
    let clean_base = payload.url.trim_end_matches('/');
    let cat = payload.categories.unwrap_or_else(|| "science".to_string());
    let mut url = format!(
        "{}/search?q=machine+learning&categories={}&format=json",
        clean_base, cat
    );
    if let Some(ref eng) = payload.engines {
        if !eng.trim().is_empty() {
            url.push_str(&format!("&engines={}", urlencoding::encode(eng.trim())));
        }
    }

    match client.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
        .header("Accept", "application/json")
        .send().await {
        Ok(resp) => {
            let elapsed = started.elapsed().as_millis() as u64;
            let status = resp.status();
            if status.is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let count = json.get("results").and_then(|r| r.as_array()).map(|a| a.len()).unwrap_or(0);
                    Json(crate::models::TestSearxngResponse {
                        success: true,
                        latency_ms: elapsed,
                        number_of_results: count,
                        message: format!("SearXNG connected. Received {} results for category '{}'", count, cat),
                    })
                } else {
                    Json(crate::models::TestSearxngResponse {
                        success: false,
                        latency_ms: elapsed,
                        number_of_results: 0,
                        message: "SearXNG responded but not with JSON (check 'formats: [html, json]' in settings.yml)".to_string(),
                    })
                }
            } else if status.as_u16() == 403 || status.as_u16() == 429 {
                Json(crate::models::TestSearxngResponse {
                    success: false,
                    latency_ms: elapsed,
                    number_of_results: 0,
                    message: format!("The SearXNG server refused the query (HTTP {}). The built-in MetaSearch / Europe PMC still works without SearXNG.", status),
                })
            } else {
                Json(crate::models::TestSearxngResponse {
                    success: false,
                    latency_ms: elapsed,
                    number_of_results: 0,
                    message: format!("SearXNG returned HTTP {}", status),
                })
            }
        }
        Err(e) => Json(crate::models::TestSearxngResponse {
            success: false,
            latency_ms: started.elapsed().as_millis() as u64,
            number_of_results: 0,
            message: format!("Cannot connect to SearXNG: {}", e),
        }),
    }
}
