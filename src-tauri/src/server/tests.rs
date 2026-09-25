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
