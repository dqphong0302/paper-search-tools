//! Search, query preview, source health checks, paper details, citations and trends.

use super::*;

pub(super) static SEARCH_GATE: tokio::sync::Mutex<Option<tokio::time::Instant>> =
    tokio::sync::Mutex::const_new(None);

pub(super) async fn wait_for_search_slot(delay: Duration) {
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
pub(super) struct TrendsQuery {
    geo: Option<String>,
}


#[derive(serde::Deserialize)]
pub(super) struct CitationsQuery {
    direction: Option<String>,
    limit: Option<usize>,
}

/// Query form accepts arbitrary IDs (OpenAlex URLs contain slashes).
#[derive(serde::Deserialize)]
pub(super) struct CitationsByIdQuery {
    id: String,
    direction: Option<String>,
    limit: Option<usize>,
}

#[derive(serde::Deserialize)]
pub(super) struct SourceCheckRequest {
    id: String,
    /// Optional override; the default is a query the source can plausibly answer.
    #[serde(default)]
    query: Option<String>,
}

#[derive(serde::Serialize)]
pub(super) struct SourceCheckResponse {
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
pub(super) struct QueryPreviewRequest {
    query: String,
    #[serde(default)]
    sources: Option<Vec<String>>,
}

#[derive(serde::Serialize)]
pub(super) struct QueryPreviewResponse {
    query: String,
    adapter_version: &'static str,
    sources: Vec<crate::query::QueryPreview>,
}


#[derive(serde::Serialize)]
pub(super) struct TrendItem {
    title: String,
    traffic: Option<String>,
    published_at: Option<String>,
    explore_url: String,
}


// Resolve user defaults once, before hashing or dispatching.
pub(super) fn apply_search_defaults(req: &mut SearchRequest, get: impl Fn(&str) -> Option<String>) {
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

pub(super) fn cached_search(db: &Database, key: &str, ttl: u64) -> Option<SearchResponse> {
    db.get_cache(key, ttl).filter(|response| {
        !response
            .sources
            .iter()
            .any(|source| source.queried && !source.ok)
            || db.get_cache(key, ttl.min(30)).is_some()
    })
}

pub(super) fn search_cache_key(
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


pub(super) async fn query_preview_handler(
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


// Handler 2: Search papers
/// A query the source can plausibly answer, so that "0 results" stays rare
/// enough for the count to mean something. Specialised records (a protein, a
/// CVE) need a matching term; everything else is grouped by discipline.
pub(super) fn probe_query(source: &crate::catalog::Source) -> &'static str {
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
pub(super) async fn source_check_handler(
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
pub(super) async fn paper_handler(
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
pub(super) async fn citations_handler(
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

pub(super) async fn citations_query_handler(
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


// Live daily search topics from the public Google Trends RSS feed. The
// interactive 5-year chart stays on Google Trends because its private widget
// endpoint is intentionally rate-limited and not a stable application API.
pub(super) async fn trends_handler(
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
