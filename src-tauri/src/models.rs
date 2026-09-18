use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paper {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub venue: Option<String>,
    #[serde(rename = "abstract")]
    pub abstract_text: Option<String>,
    pub doi: Option<String>,
    /// Landing page at the originating source (OpenAlex/PubMed/arXiv/…), so a
    /// record is always traceable even when it has no DOI.
    #[serde(default)]
    pub source_url: Option<String>,
    pub pdf_url: Option<String>,
    pub citations: Option<u32>,
    pub quartile: Option<String>,
    pub source: String,
    pub score: Option<f64>,
    pub open_access: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub sources: Option<Vec<String>>,
    pub limit: Option<usize>,
    pub year_min: Option<u32>,
    pub year_max: Option<u32>,
    pub open_access_only: Option<bool>,
    /// Pagination offset into the cached, bounded candidate pool.
    #[serde(default)]
    pub offset: Option<usize>,
    pub searxng_url: Option<String>,
    pub searxng_categories: Option<String>,
    pub searxng_engines: Option<String>,
    /// When set, the query and any saved papers are grouped under this workspace.
    #[serde(default)]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub paper_count: usize,
    #[serde(default)]
    pub query_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePaper {
    pub paper: Paper,
    pub note: Option<String>,
    pub added_at: u64,
    /// "unread" | "reading" | "read"
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkspaceRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePaperRequest {
    pub paper: Paper,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkspacePaperRequest {
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub favorite: Option<bool>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// Per-source outcome of one search fan-out, so the UI can say which sources
/// actually answered instead of presenting a partial result as a complete one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceStatus {
    pub id: String,
    pub name: String,
    /// False when the current scope excludes this source.
    pub queried: bool,
    pub ok: bool,
    pub count: usize,
    pub error: Option<String>,
    /// True when the source returned nothing because it still needs an API key,
    /// a signed-in session or a URL — not because it failed to respond.
    #[serde(default)]
    pub needs_setup: bool,
    /// True when the source was skipped this time because it failed repeatedly
    /// and is in its cooldown window.
    #[serde(default)]
    pub cooling_down: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    /// Papers actually returned (capped by `limit`).
    pub total: usize,
    /// Papers that matched before the `limit` cut, so callers can paginate.
    #[serde(default)]
    pub available_total: usize,
    pub elapsed_ms: u64,
    pub cache_hit: bool,
    pub papers: Vec<Paper>,
    #[serde(default)]
    pub sources: Vec<SourceStatus>,
}

impl SearchResponse {
    pub fn into_page(mut self, offset: usize, limit: usize) -> Self {
        self.available_total = self.papers.len();
        self.papers = self.papers.into_iter().skip(offset).take(limit).collect();
        self.total = self.papers.len();
        self
    }
}

/// Credentials the user supplies in Settings. Anonymous requests get throttled
/// hardest, so every source that accepts an identifier is given one.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceCredentials {
    pub openalex_email: Option<String>,
    pub openalex_api_key: Option<String>,
    pub semantic_scholar_api_key: Option<String>,
    pub ncbi_api_key: Option<String>,
    pub ncbi_email: Option<String>,
    pub crossref_email: Option<String>,
    pub unpaywall_email: Option<String>,
    pub scopus_api_key: Option<String>,
    pub ieee_api_key: Option<String>,
    pub springer_api_key: Option<String>,
    pub perplexity_api_key: Option<String>,
    pub core_api_key: Option<String>,
    pub dimensions_api_key: Option<String>,
    pub wos_api_key: Option<String>,
    pub consensus_session: Option<String>,
    pub openevidence_session: Option<String>,
}

impl SourceCredentials {
    /// Treats blank strings as absent so a cleared settings field stops being sent.
    pub fn clean(value: Option<String>) -> Option<String> {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLog {
    pub id: String,
    pub timestamp: String,
    pub agent_name: String,
    pub method: String,
    pub query: String,
    pub result_count: usize,
    pub latency_ms: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryStats {
    pub gateway_status: String,
    pub port: u16,
    pub total_queries: usize,
    pub cache_hit_rate: f64,
    pub avg_latency_ms: u64,
    pub recent_logs: Vec<AgentLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub paper_id: String,
    pub title: String,
    pub pdf_url: String,
    pub source: Option<String>,
    pub year: Option<u32>,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResponse {
    pub success: bool,
    pub local_path: Option<String>,
    pub file_size_bytes: Option<u64>,
    /// True when the publisher refused the fetch rather than the fetch failing.
    /// The UI offers the article page instead of reporting a dead end.
    #[serde(default)]
    pub blocked: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistoryItem {
    pub id: String,
    pub query: String,
    pub sources: Option<String>,
    pub result_count: usize,
    pub elapsed_ms: u64,
    pub created_at: u64,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub paper_id: String,
    pub title: String,
    pub pdf_url: String,
    pub local_path: String,
    pub file_size_bytes: u64,
    pub source: Option<String>,
    pub year: Option<u32>,
    pub downloaded_at: u64,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFileRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestLlmRequest {
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
    /// UI leaves the key field blank because stored secrets are never returned;
    /// this tells the backend to use the credential already saved in Settings.
    #[serde(default)]
    pub use_saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestLlmResponse {
    pub success: bool,
    pub latency_ms: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSearxngRequest {
    pub url: String,
    pub categories: Option<String>,
    pub engines: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSearxngResponse {
    pub success: bool,
    pub latency_ms: u64,
    pub number_of_results: usize,
    pub message: String,
}
