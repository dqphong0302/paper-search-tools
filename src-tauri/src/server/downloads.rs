//! PDF downloads (with open-access fallbacks), download history and opening files.

use super::*;

// Handler 4: Download PDF
pub(super) async fn download_handler(
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

pub(super) enum PdfFetchError {
    /// Publishers put bot protection in front of many PDF links, so this is a
    /// routine outcome rather than a fault.
    Blocked(u16),
    Status(u16),
    NotPdf(&'static str),
    Network(String),
}

pub(super) async fn fetch_pdf(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, PdfFetchError> {
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
pub(super) async fn alternate_pdf_urls(state: &AppState, client: &reqwest::Client, doi: &str) -> Vec<String> {
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

pub(super) async fn get_json(client: &reqwest::Client, url: &str) -> Option<serde_json::Value> {
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
pub(super) async fn download_content_handler(
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


pub(super) async fn clear_download_history_handler(
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


// Handler 7: Download History (optionally scoped to a workspace)
pub(super) async fn get_download_history_handler(
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> Json<Vec<DownloadRecord>> {
    Json(
        state
            .db
            .get_download_history(params.workspace_id.as_deref()),
    )
}

pub(super) async fn delete_download_handler(
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
pub(super) async fn open_file_handler(Json(payload): Json<OpenFileRequest>) -> Json<serde_json::Value> {
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
