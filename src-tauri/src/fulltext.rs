//! Full-text Markdown for downloaded papers.
//!
//! The UI extracts a PDF's text layer (or OCRs a scanned PDF) into Markdown and
//! stores it here, one file per paper under the download directory. Agents read
//! it through MCP `get_paper_fulltext`, or take the whole collection as a folder
//! from `export_collection`.

use crate::server::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS fulltexts (
            paper_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            path TEXT NOT NULL,
            method TEXT NOT NULL,
            pages INTEGER NOT NULL DEFAULT 0,
            chars INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct FulltextInfo {
    pub paper_id: String,
    pub title: String,
    pub path: String,
    /// "text" (PDF text layer), "ocr" or "mixed".
    pub method: String,
    pub pages: u32,
    pub chars: u64,
    pub updated_at: u64,
}

fn row_info(row: &rusqlite::Row) -> rusqlite::Result<FulltextInfo> {
    Ok(FulltextInfo {
        paper_id: row.get(0)?,
        title: row.get(1)?,
        path: row.get(2)?,
        method: row.get(3)?,
        pages: row.get(4)?,
        chars: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub fn info(conn: &Connection, paper_id: &str) -> Option<FulltextInfo> {
    conn.query_row(
        "SELECT paper_id, title, path, method, pages, chars, updated_at FROM fulltexts WHERE paper_id = ?1",
        [paper_id],
        row_info,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn list(conn: &Connection) -> Vec<FulltextInfo> {
    let Ok(mut statement) = conn.prepare(
        "SELECT paper_id, title, path, method, pages, chars, updated_at FROM fulltexts ORDER BY updated_at DESC",
    ) else {
        return Vec::new();
    };
    statement
        .query_map([], row_info)
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// The stored Markdown, if the file still exists.
pub fn read(conn: &Connection, paper_id: &str) -> Option<(FulltextInfo, String)> {
    let info = info(conn, paper_id)?;
    let text = std::fs::read_to_string(&info.path).ok()?;
    Some((info, text))
}

/// A file-system-safe name: letters, digits and dashes, at most 60 characters.
pub fn slug(value: &str) -> String {
    let mut slug = String::new();
    for c in value.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.chars().count() >= 60 {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "paper".into()
    } else {
        slug
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn store(
    state: &AppState,
    paper_id: &str,
    title: &str,
    markdown: &str,
    method: &str,
    pages: u32,
) -> Result<FulltextInfo, String> {
    let dir = crate::config::download_directory(&state.db)?.join("markdown");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let hash = crate::server::content_hash(paper_id.as_bytes());
    let path = dir.join(format!("{}_{}.md", slug(title), &hash[..8]));
    std::fs::write(&path, markdown).map_err(|e| e.to_string())?;
    let info = FulltextInfo {
        paper_id: paper_id.to_string(),
        title: title.to_string(),
        path: path.to_string_lossy().into_owned(),
        method: method.to_string(),
        pages,
        chars: markdown.chars().count() as u64,
        updated_at: now(),
    };
    state
        .db
        .conn()
        .execute(
            "INSERT OR REPLACE INTO fulltexts (paper_id, title, path, method, pages, chars, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![info.paper_id, info.title, info.path, info.method, info.pages, info.chars, info.updated_at],
        )
        .map_err(|e| e.to_string())?;
    Ok(info)
}

// ---- HTTP -----------------------------------------------------------------

#[derive(Deserialize)]
pub struct PaperQuery {
    paper_id: String,
}

#[derive(Deserialize)]
pub struct StoreRequest {
    title: String,
    markdown: String,
    #[serde(default = "default_method")]
    method: String,
    #[serde(default)]
    pages: u32,
}

fn default_method() -> String {
    "text".into()
}

pub async fn put_handler(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
    Json(payload): Json<StoreRequest>,
) -> (StatusCode, Json<Value>) {
    if query.paper_id.trim().is_empty() || payload.markdown.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "paper_id and markdown are required" })));
    }
    if !["text", "ocr", "mixed"].contains(&payload.method.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "method must be text, ocr or mixed" })));
    }
    match store(&state, &query.paper_id, &payload.title, &payload.markdown, &payload.method, payload.pages) {
        Ok(info) => (StatusCode::OK, Json(json!(info))),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": error }))),
    }
}

pub async fn get_handler(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> (StatusCode, Json<Value>) {
    match read(&state.db.conn(), &query.paper_id) {
        Some((info, markdown)) => (StatusCode::OK, Json(json!({ "info": info, "markdown": markdown }))),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "No Markdown full text for this paper yet" }))),
    }
}

pub async fn index_handler(State(state): State<AppState>) -> Json<Value> {
    Json(json!(list(&state.db.conn())))
}

/// Where OCR language models are cached: next to the app database.
fn tessdata_dir() -> Result<PathBuf, String> {
    let base = match std::env::var("SCHOLARGATE_DATA_DIR") {
        Ok(path) => PathBuf::from(path),
        Err(_) => PathBuf::from(
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map_err(|_| "Cannot locate user data directory")?,
        )
        .join(".scholargate"),
    };
    let dir = base.join("tessdata");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// OCR language models ("vie.traineddata.gz"), downloaded once from the
/// tesseract.js data CDN and then served locally, so OCR works offline and the
/// app's content policy never has to allow a third-party origin.
pub async fn ocr_language_handler(
    Path(file): Path<String>,
    State(state): State<AppState>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let Some(lang) = file.strip_suffix(".traineddata.gz").filter(|lang| {
        (3..=12).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let dir = match tessdata_dir() {
        Ok(dir) => dir,
        Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    };
    let path = dir.join(&file);
    if !path.exists() {
        let client = crate::engine::pooled_client(300, crate::config::outbound_proxy(&state.db).as_deref());
        let mut fetched = None;
        for url in [
            format!("https://cdn.jsdelivr.net/npm/@tesseract.js-data/{lang}/4.0.0_best_int/{lang}.traineddata.gz"),
            format!("https://unpkg.com/@tesseract.js-data/{lang}@1.0.0/4.0.0_best_int/{lang}.traineddata.gz"),
        ] {
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    if let Ok(bytes) = response.bytes().await {
                        fetched = Some(bytes);
                        break;
                    }
                }
            }
        }
        let Some(bytes) = fetched else {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Could not download the OCR model for '{lang}'. Check the internet connection and try again."),
            )
                .into_response();
        };
        // Write-then-rename so a half-finished download is never served.
        let partial = dir.join(format!("{file}.part"));
        if std::fs::write(&partial, &bytes).and_then(|_| std::fs::rename(&partial, &path)).is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Could not cache the OCR model").into_response();
        }
    }
    match std::fs::read(&path) {
        Ok(bytes) => ([(axum::http::header::CONTENT_TYPE, "application/gzip")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
pub struct ExportRequest {
    /// Generated files to place at the bundle root (index.md, references.ris, …).
    #[serde(default)]
    files: std::collections::BTreeMap<String, String>,
}

/// Writes a collection as a folder an agent can read directly:
/// the generated files at the root, plus `pdf/` and `markdown/` copies of every
/// paper in the collection that has them.
pub async fn export_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ExportRequest>,
) -> (StatusCode, Json<Value>) {
    let Some(workspace) = state.db.list_workspaces().into_iter().find(|w| w.id == id) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Collection not found" })));
    };
    match export(&state, &workspace, &payload.files) {
        Ok(result) => (StatusCode::OK, Json(result)),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": error }))),
    }
}

pub fn export(
    state: &AppState,
    workspace: &crate::models::Workspace,
    files: &std::collections::BTreeMap<String, String>,
) -> Result<Value, String> {
    let root = crate::config::download_directory(&state.db)?
        .join("collections")
        .join(slug(&workspace.name));
    let pdf_dir = root.join("pdf");
    let md_dir = root.join("markdown");
    for dir in [&root, &pdf_dir, &md_dir] {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    for (name, content) in files {
        // Only plain file names: a generated file must never escape the bundle.
        let safe = std::path::Path::new(name)
            .file_name()
            .filter(|n| n.to_string_lossy() == *name && !name.starts_with('.'));
        let Some(file_name) = safe else {
            return Err(format!("Invalid bundle file name: {name}"));
        };
        std::fs::write(root.join(file_name), content).map_err(|e| e.to_string())?;
    }

    let papers = state.db.workspace_papers(&workspace.id)?;
    if !files.contains_key("index.md") {
        std::fs::write(root.join("index.md"), index_markdown(workspace, &papers)).map_err(|e| e.to_string())?;
    }
    let downloads = state.db.get_download_history(None);
    let (mut pdfs, mut markdown) = (0, 0);
    for (index, item) in papers.iter().enumerate() {
        let base = format!("{:03}-{}", index + 1, slug(&item.paper.title));
        let pdf = downloads
            .iter()
            .map(|d| (d, PathBuf::from(&d.local_path)))
            .find(|(d, path)| d.paper_id == item.paper.id && path.exists())
            .map(|(_, path)| path);
        if let Some(pdf) = pdf {
            if std::fs::copy(&pdf, pdf_dir.join(format!("{base}.pdf"))).is_ok() {
                pdfs += 1;
            }
        }
        if let Some((_, text)) = read(&state.db.conn(), &item.paper.id) {
            if std::fs::write(md_dir.join(format!("{base}.md")), text).is_ok() {
                markdown += 1;
            }
        }
    }
    Ok(json!({
        "path": root.to_string_lossy(),
        "papers": papers.len(),
        "pdfs": pdfs,
        "markdown": markdown,
    }))
}

/// A Markdown overview of a collection: one row per paper with the metadata an
/// agent needs to pick what to read, and where its full text lives in the bundle.
pub fn index_markdown(workspace: &crate::models::Workspace, papers: &[crate::models::WorkspacePaper]) -> String {
    let cell = |value: &str| value.replace('|', "\\|").replace(['\n', '\r'], " ");
    let mut out = format!("# {}\n\n", workspace.name);
    if let Some(description) = workspace.description.as_deref().filter(|d| !d.trim().is_empty()) {
        out.push_str(&format!("{description}\n\n"));
    }
    out.push_str(&format!(
        "Collection ID: `{}` · {} papers. Full texts are in `markdown/`, PDFs in `pdf/`, numbered as below.\n\n",
        workspace.id,
        papers.len()
    ));
    out.push_str("| # | Title | Authors | Year | Journal | Q | Topic | DOI |\n|---|---|---|---|---|---|---|---|\n");
    for (index, item) in papers.iter().enumerate() {
        let paper = &item.paper;
        let authors = match paper.authors.len() {
            0 => String::new(),
            1..=3 => paper.authors.join(", "),
            _ => format!("{} et al.", paper.authors[..3].join(", ")),
        };
        let topic = paper.biblio.as_ref().and_then(|b| b.topic.clone()).unwrap_or_default();
        out.push_str(&format!(
            "| {:03} | {} | {} | {} | {} | {} | {} | {} |\n",
            index + 1,
            cell(&paper.title),
            cell(&authors),
            paper.year.map(|y| y.to_string()).unwrap_or_default(),
            cell(paper.venue.as_deref().unwrap_or("")),
            paper.quartile.as_deref().unwrap_or(""),
            cell(&topic),
            paper.doi.as_deref().unwrap_or(""),
        ));
    }
    let notes: Vec<_> = papers
        .iter()
        .enumerate()
        .filter_map(|(i, item)| item.note.as_deref().filter(|n| !n.trim().is_empty()).map(|n| (i, n)))
        .collect();
    if !notes.is_empty() {
        out.push_str("\n## Notes\n\n");
        for (index, note) in notes {
            out.push_str(&format!("- **{:03}**: {}\n", index + 1, note.replace('\n', " ")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_markdown_and_exports_a_collection_bundle() {
        let dir = std::env::temp_dir().join(format!("sg-fulltext-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = AppState {
            db: crate::db::Database::in_memory().unwrap(),
            engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        state.db.set_config("download_directory", &dir.to_string_lossy()).unwrap();
        let workspace = state.db.create_workspace("My Review", None).unwrap();
        let paper: crate::models::Paper = serde_json::from_value(json!({
            "id": "doi:10.1/x", "title": "A paper", "authors": [], "source": "test", "open_access": false
        }))
        .unwrap();
        state.db.add_workspace_paper(&workspace.id, &paper, None).unwrap();

        let info = store(&state, "doi:10.1/x", "A paper", "# A paper\n\nBody", "ocr", 3).unwrap();
        assert_eq!(info.method, "ocr");
        let (_, text) = read(&state.db.conn(), "doi:10.1/x").unwrap();
        assert!(text.contains("Body"));

        let mut files = std::collections::BTreeMap::new();
        files.insert("index.md".to_string(), "# My Review".to_string());
        let result = export(&state, &workspace, &files).unwrap();
        let root = PathBuf::from(result["path"].as_str().unwrap());
        assert_eq!(result["markdown"], 1);
        assert!(root.join("index.md").exists());
        assert!(root.join("markdown/001-a-paper.md").exists());

        files.insert("../escape.md".to_string(), String::new());
        assert!(export(&state, &workspace, &files).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slug("CRISPR/Cas9: a review!"), "crispr-cas9-a-review");
        assert_eq!(slug("Tổng quan về đái tháo đường"), "tổng-quan-về-đái-tháo-đường");
        assert_eq!(slug("../../etc"), "etc");
        assert_eq!(slug("   "), "paper");
    }
}
