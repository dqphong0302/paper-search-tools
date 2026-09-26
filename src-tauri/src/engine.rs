use crate::models::{Paper, SearchRequest, SearchResponse, SourceCredentials, SourceStatus};
use crate::sources;
use crate::sources::registry;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex;

// Shared across engines, including engines created for a custom timeout.
static ARXIV_GATE: Mutex<Option<tokio::time::Instant>> = Mutex::const_new(None);
static S2_GATE: Mutex<Option<tokio::time::Instant>> = Mutex::const_new(None);

fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    Some(
        (date.with_timezone(&chrono::Utc) - chrono::Utc::now())
            .to_std()
            .unwrap_or_default(),
    )
}

async fn polite_get(
    request: reqwest::RequestBuilder,
    gate: &Mutex<Option<tokio::time::Instant>>,
    interval: Duration,
    label: &str,
) -> Result<String, String> {
    // Every source in a search is awaited together, so this budget is also the
    // floor on how long a rate-limited source keeps the whole search spinning.
    // Keep it close to the per-request HTTP timeout instead of 30s.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(16);
    let mut next = tokio::time::timeout_at(deadline, gate.lock())
        .await
        .map_err(|_| format!("{label}: request queue busy; retry later"))?;
    let mut last_error = String::new();
    for attempt in 0..3 {
        if let Some(ready) = *next {
            if ready > deadline {
                return Err(format!(
                    "{label}: rate-limit cooldown; retry in {}s",
                    ready
                        .saturating_duration_since(tokio::time::Instant::now())
                        .as_secs()
                        + 1
                ));
            }
            tokio::time::sleep_until(ready).await;
        }
        *next = Some(tokio::time::Instant::now() + interval);
        let attempt_request = request
            .try_clone()
            .ok_or_else(|| format!("{label}: request cannot be retried"))?;
        // Keep the source gate through the body read, not just the headers.
        // Otherwise a slow feed can overlap the next connection.
        let response = tokio::time::timeout_at(deadline, async {
            let response = attempt_request.send().await?;
            let status = response.status();
            let retry = retry_after(response.headers());
            let body = if status.is_success() {
                response.text().await?
            } else {
                String::new()
            };
            Ok::<_, reqwest::Error>((status, retry, body))
        })
        .await;
        let delay = match response {
            Ok(Ok((status, retry, body))) => {
                if status.is_success() {
                    return Ok(body);
                }
                if status.as_u16() != 429 && !status.is_server_error() {
                    return Err(format!("{label}: HTTP {}", status.as_u16()));
                }
                last_error = format!("HTTP {}", status.as_u16());
                retry.unwrap_or(Duration::from_secs(3 * (1 << attempt)))
            }
            Ok(Err(error)) => {
                // Display the transport cause, not just reqwest's URL wrapper.
                let cause = std::error::Error::source(&error)
                    .map(|e| e.to_string())
                    .unwrap_or_default();
                last_error = if error.is_timeout() {
                    "connection timed out".into()
                } else {
                    format!("connection failed: {cause}")
                };
                Duration::from_secs(3 * (1 << attempt))
            }
            Err(_) => {
                *next = Some(tokio::time::Instant::now() + Duration::from_secs(12).max(interval));
                return Err(format!("{label}: exceeded the retry budget; {last_error}"));
            }
        };
        // Never shorten Retry-After. Long cooldowns survive this request without
        // holding the entire multi-source search open for minutes.
        let delay = delay.max(interval);
        let ready = tokio::time::Instant::now()
            .checked_add(delay)
            .unwrap_or(deadline + Duration::from_secs(86400));
        *next = Some(ready);
        if attempt == 2 || ready >= deadline {
            return Err(format!(
                "{label}: {last_error} after {} attempt(s); retry in {}s",
                attempt + 1,
                delay.as_secs() + 1
            ));
        }
    }
    unreachable!()
}

/// Consecutive-failure tracking per source. A site that is blocking us, has
/// retired its endpoint or is rate-limiting will keep failing on every search;
/// re-testing it each time only costs the user the full timeout and fills the
/// result page with the same red errors. After `TRIP_AFTER` consecutive failures
/// the source is skipped for `COOLDOWN` and reported with the reason.
static SOURCE_HEALTH: Mutex<Option<HashMap<String, SourceHealth>>> = Mutex::const_new(None);

const TRIP_AFTER: u32 = 2;

/// Cooldown grows with the run of failures. A source that hit a transient rate
/// limit is back within a minute, while one that is genuinely gone backs off to
/// long pauses instead of being retried on every search.
fn cooldown_for(consecutive_failures: u32) -> Duration {
    match consecutive_failures {
        0..=2 => Duration::from_secs(60),
        3..=4 => Duration::from_secs(300),
        _ => Duration::from_secs(900),
    }
}

#[derive(Default, Clone)]
struct SourceHealth {
    consecutive_failures: u32,
    open_until: Option<tokio::time::Instant>,
    last_error: String,
}

/// Source ids currently in cooldown, with the reason to report for each.
async fn cooling_down(candidates: &[&str]) -> HashMap<String, String> {
    let mut guard = SOURCE_HEALTH.lock().await;
    let health = guard.get_or_insert_with(HashMap::new);
    let now = tokio::time::Instant::now();
    let mut cooling = HashMap::new();
    for id in candidates {
        let Some(entry) = health.get_mut(*id) else {
            continue;
        };
        match entry.open_until {
            Some(until) if until > now => {
                cooling.insert((*id).to_string(), {
                    // The recorded error already starts with the source id;
                    // repeating it inside this message reads as a stutter.
                    let last = entry
                        .last_error
                        .strip_prefix(&format!("{id}: "))
                        .unwrap_or(&entry.last_error);
                    format!(
                        "paused after {} failures · last: {} · retrying in {}s",
                        entry.consecutive_failures,
                        last,
                        until.duration_since(now).as_secs() + 1,
                    )
                });
            }
            Some(_) => {
                // Cooldown elapsed: let the next search probe the source again.
                entry.open_until = None;
                entry.consecutive_failures = 0;
            }
            None => {}
        }
    }
    cooling
}

/// Forget the breaker state for one source. An explicit health check must
/// really reach the source: reporting "paused, retrying in 240s" would answer a
/// question the user did not ask. The probe's own outcome then re-arms or
/// clears the breaker like any other query.
pub async fn reset_source_health(id: &str) {
    let mut guard = SOURCE_HEALTH.lock().await;
    if let Some(health) = guard.as_mut() {
        health.remove(id);
    }
}

/// Records the outcome of every source that was actually queried.
async fn record_source_health(statuses: &[SourceStatus]) {
    let mut guard = SOURCE_HEALTH.lock().await;
    let health = guard.get_or_insert_with(HashMap::new);
    for status in statuses {
        // A source awaiting credentials is not failing, and one that was skipped
        // was never tested — neither should move the breaker.
        if !status.queried || status.needs_setup || status.cooling_down {
            continue;
        }
        let entry = health.entry(status.id.clone()).or_default();
        if status.ok {
            *entry = SourceHealth::default();
            continue;
        }
        entry.consecutive_failures += 1;
        entry.last_error = status.error.clone().unwrap_or_default();
        if entry.consecutive_failures >= TRIP_AFTER {
            entry.open_until =
                Some(tokio::time::Instant::now() + cooldown_for(entry.consecutive_failures));
        }
    }
}

/// One HTTP client per distinct `(timeout, proxy)` setting, reused for the life
/// of the process.
///
/// A `reqwest::Client` owns the connection pool, so building a fresh one per
/// request threw that pool away: a search fanning out to dozens of hosts paid a
/// new TCP connection and TLS handshake for every one of them, every time. The
/// handful of settings a user can pick makes this map tiny and effectively
/// fixed after the first search. `Client` is internally reference-counted, so
/// cloning one out of the map is cheap.
static CLIENT_POOL: std::sync::OnceLock<
    std::sync::Mutex<HashMap<(u64, Option<String>), reqwest::Client>>,
> = std::sync::OnceLock::new();

pub(crate) fn pooled_client(seconds: u64, proxy_url: Option<&str>) -> reqwest::Client {
    let key = (seconds, proxy_url.map(str::to_string));
    let pool = CLIENT_POOL.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut pool = pool.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(client) = pool.get(&key) {
        return client.clone();
    }
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(seconds))
        .user_agent("ScholarGate-Desktop/1.0 (mailto:dqphong0302@gmail.com)");
    if let Some(proxy_url) = proxy_url {
        if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let client = builder.build().unwrap_or_default();
    pool.insert(key, client.clone());
    client
}

/// Strips markup a source left in its text.
///
/// Europe PMC returns structured abstracts with `<h4>Background</h4>` section
/// headings, which reached the UI verbatim. Sanitising here rather than in each
/// fetcher means no source — present or future — can leak markup into a result.
///
/// The guard matters: `clean_html_text` drops everything between `<` and `>`,
/// so running it unconditionally would eat the middle of an abstract that says
/// "p<0.05 and n>30". Only text that really contains a tag is cleaned.
fn strip_markup(value: &str) -> Option<String> {
    static TAG: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let tag = TAG.get_or_init(|| regex::Regex::new(r"</?[a-zA-Z][^>]*>").unwrap());
    tag.is_match(value)
        .then(|| sources::clean_html_text(value))
}

fn normalize_paper_text(paper: &mut Paper) {
    if let Some(cleaned) = strip_markup(&paper.title) {
        paper.title = cleaned;
    }
    if let Some(cleaned) = paper.abstract_text.as_deref().and_then(strip_markup) {
        paper.abstract_text = Some(cleaned);
    }
}

/// Turns one source's outcome into the row the caller sees.
///
/// `None` means the source was not selected for this search: it is reported as
/// present but not queried, so a caller can always tell "no hits" apart from
/// "never asked".
fn classify_source_outcome(
    id: &str,
    name: &str,
    outcome: Option<Result<Vec<Paper>, String>>,
) -> (Option<Vec<Paper>>, SourceStatus) {
    let base = |queried, ok, count, error, needs_setup| SourceStatus {
        id: id.to_string(),
        name: name.to_string(),
        queried,
        ok,
        count,
        error,
        needs_setup,
        cooling_down: false,
    };
    match outcome {
        None => (None, base(false, true, 0, None, false)),
        Some(Ok(mut papers)) => {
            for paper in &mut papers {
                normalize_paper_text(paper);
            }
            let status = base(true, true, papers.len(), None, false);
            let papers = (!papers.is_empty()).then_some(papers);
            (papers, status)
        }
        Some(Err(error)) => {
            let needs_setup = error.starts_with(sources::NEEDS_SETUP);
            let error = error
                .strip_prefix(sources::NEEDS_SETUP)
                .unwrap_or(&error)
                .to_string();
            (None, base(true, false, 0, Some(error), needs_setup))
        }
    }
}

pub struct AcademicEngine {
    client: reqwest::Client,
    /// Per-request HTTP timeout this engine was built with. The whole-search
    /// budget is derived from it so the Settings value governs wall-clock time
    /// rather than only the timeout of one HTTP call.
    request_timeout_seconds: u64,
}

impl AcademicEngine {
    pub fn new() -> Self {
        Self::with_options(12, None)
    }

    #[cfg(test)]
    pub fn with_timeout(seconds: u64) -> Self {
        Self::with_options(seconds, None)
    }

    pub fn with_options(seconds: u64, proxy_url: Option<&str>) -> Self {
        let seconds = seconds.clamp(1, 120);
        Self {
            client: pooled_client(seconds, proxy_url),
            request_timeout_seconds: seconds,
        }
    }

    /// How long the whole fan-out may take.
    ///
    /// Sources run concurrently, so this is the wall clock of a search rather
    /// than the sum of its parts. Twice the per-request timeout leaves room for
    /// the one retry `polite_get` may take and for the server-rendered journal
    /// portals, which need far longer than the JSON APIs; the floor keeps a very
    /// short configured timeout from cutting off every slow source, and the
    /// ceiling is the promise that a search always returns.
    pub(crate) fn search_budget(&self) -> Duration {
        Duration::from_secs((self.request_timeout_seconds * 2).clamp(10, 40))
    }

    pub async fn search_candidates(
        &self,
        req: &SearchRequest,
        creds: &SourceCredentials,
    ) -> SearchResponse {
        let started = Instant::now();
        // A bounded, page-independent candidate pool. The gateway caches this
        // complete ranking and slices pages from it, never re-ranking a larger
        // upstream sample for each offset.
        let fetch_limit = 100;
        let query = req.query.trim().to_string();

        let resolved_sources = resolve_sources(req.sources.as_deref(), &query);

        let selected: Vec<&str> = crate::catalog::catalog()
            .sources
            .iter()
            .filter(|source| source.available)
            .map(|source| source.id.as_str())
            .filter(|id| match &resolved_sources {
                None => true,
                Some(list) => list.iter().any(|x| x.eq_ignore_ascii_case(id)),
            })
            .collect();
        let cooling = cooling_down(&selected).await;

        let has_source = |name: &str| -> bool {
            if !crate::catalog::catalog()
                .sources
                .iter()
                .any(|source| source.id == name && source.available)
            {
                return false;
            }
            if cooling.contains_key(name) {
                return false;
            }
            match &resolved_sources {
                None => true,
                Some(list) => list.iter().any(|x| x.eq_ignore_ascii_case(name)),
            }
        };

        let query_metasearch = has_source("metasearch");
        let query_searxng = has_source("searxng");
        // A Vietnam-only selection has no other source to fill the page.
        let only_vn = has_source("vietnam")
            && !has_source("openalex")
            && !has_source("pubmed")
            && !has_source("arxiv")
            && !has_source("crossref");

        let searxng_url = req.searxng_url.clone();
        let searxng_categories = req.searxng_categories.clone();
        let searxng_engines = req.searxng_engines.clone();
        let use_external_searxng = (query_metasearch || query_searxng)
            && searxng_url
                .as_deref()
                .map(str::trim)
                .is_some_and(|url| !url.is_empty());

        let ctx = registry::SearchCtx {
            engine: self,
            client: &self.client,
            query: &query,
            limit: fetch_limit,
            year_min: req.year_min,
            year_max: req.year_max,
            creds,
            only_vn,
            budget: self.search_budget(),
        };

        // Every selected source runs concurrently, but the search as a whole is
        // capped: `join!`-ing all of them meant the slowest source set the
        // latency of every query, and the slowest are the server-rendered
        // journal portals that legitimately take half a minute. Sources that
        // answer inside the budget are used; the rest are reported as having run
        // out of time, so the caller can see exactly which coverage is missing
        // instead of waiting for it.
        let deadline = tokio::time::Instant::now() + ctx.budget;
        let budget_secs = ctx.budget.as_secs();

        // Captured by shared reference: every driver future reads the same ctx.
        let ctx = &ctx;
        let driver_results = async move {
            let mut running: futures::stream::FuturesUnordered<_> = registry::drivers()
                .iter()
                .filter(|driver| has_source(driver.id))
                .map(|driver| async move {
                    let outcome = match tokio::time::timeout_at(deadline, driver.run(&ctx)).await {
                        Ok(outcome) => outcome,
                        Err(_) => Err(format!(
                            "{}: no answer within the {budget_secs}s search budget",
                            driver.id
                        )),
                    };
                    (driver.id, driver.name, outcome)
                })
                .collect();
            let mut collected = Vec::new();
            while let Some(finished) = futures::StreamExt::next(&mut running).await {
                collected.push(finished);
            }
            collected
        };

        // The metasearch slot is one status whose id and name depend on whether
        // an external SearXNG is configured, so it cannot be a registry row.
        let metasearch_future = async {
            let outcome = match searxng_url {
                Some(ref url) if use_external_searxng => Some(
                    self.fetch_searxng(
                        &crate::query::adapt("searxng", &query),
                        fetch_limit,
                        url,
                        searxng_categories.as_deref(),
                        searxng_engines.as_deref(),
                    )
                    .await,
                ),
                _ if query_metasearch => Some(
                    self.fetch_europe_pmc(&crate::query::adapt("metasearch", &query), fetch_limit)
                        .await,
                ),
                _ if query_searxng => Some(Err(format!(
                    "{}SearXNG is not configured; set its URL in Settings",
                    sources::NEEDS_SETUP
                ))),
                _ => None,
            };
            match outcome {
                None => None,
                Some(outcome) => Some(
                    match tokio::time::timeout_at(deadline, std::future::ready(outcome)).await {
                        Ok(outcome) => outcome,
                        Err(_) => Err(format!(
                            "metasearch: no answer within the {budget_secs}s search budget"
                        )),
                    },
                ),
            }
        };

        let (driver_outcomes, metasearch_res) = tokio::join!(driver_results, metasearch_future);

        let mut ranked_lists: Vec<Vec<Paper>> = Vec::new();
        let mut statuses: Vec<SourceStatus> = Vec::new();

        let mut collect = |id: &str, name: &str, outcome: Option<Result<Vec<Paper>, String>>| {
            let (papers, status) = classify_source_outcome(id, name, outcome);
            statuses.push(status);
            if let Some(papers) = papers {
                ranked_lists.push(papers);
            }
        };

        // `FuturesUnordered` yields in completion order; report in catalog order
        // so the source list a user sees does not reshuffle between searches.
        let mut by_id: HashMap<&str, Result<Vec<Paper>, String>> = driver_outcomes
            .into_iter()
            .map(|(id, _, outcome)| (id, outcome))
            .collect();
        // Unselected sources are reported too, with `queried: false`, so the
        // caller can always distinguish a source that found nothing from one
        // this search never asked.
        for driver in registry::drivers() {
            collect(driver.id, driver.name, by_id.remove(driver.id));
        }

        collect(
            if query_searxng && !query_metasearch {
                "searxng"
            } else {
                "metasearch"
            },
            if use_external_searxng {
                "SearXNG (external, optional)"
            } else if query_searxng && !query_metasearch {
                "SearXNG (not configured)"
            } else {
                "MetaSearch (built-in) / Europe PMC"
            },
            metasearch_res,
        );

        drop(collect);

        // Update the breaker from what actually happened, then surface the sources
        // this search skipped so the caller never silently loses one.
        record_source_health(&statuses).await;
        let names: HashMap<&str, &str> = crate::catalog::catalog()
            .sources
            .iter()
            .map(|source| (source.id.as_str(), source.name.as_str()))
            .collect();
        for (id, reason) in cooling {
            let name = names
                .get(id.as_str())
                .copied()
                .unwrap_or(id.as_str())
                .to_string();
            statuses.push(SourceStatus {
                id,
                name,
                queried: true,
                ok: false,
                count: 0,
                error: Some(reason),
                needs_setup: false,
                cooling_down: true,
            });
        }

        // Apply Reciprocal Rank Fusion (RRF k=60).
        // Fuse everything first so a paper counts as open access when *any* source has a
        // free full text for it, then filter, then cut to `limit` so the caller still gets
        // a full page of results.
        let mut merged_papers = self.reciprocal_rank_fusion(ranked_lists, 60.0, usize::MAX);

        // Unpaywall is a DOI resolver rather than a search index. When an email is
        // configured, use it to fill missing legal open-access PDF links.
        if let Some(email) = SourceCredentials::clean(creds.unpaywall_email.clone()) {
            self.enrich_with_unpaywall(&mut merged_papers, &email).await;
        }

        merged_papers.retain(|paper| matches_year_range(paper.year, req.year_min, req.year_max));
        if req.open_access_only.unwrap_or(false) {
            merged_papers.retain(|p| p.open_access || p.pdf_url.is_some());
        }
        let available_total = merged_papers.len();
        SearchResponse {
            query,
            total: available_total,
            available_total,
            elapsed_ms: started.elapsed().as_millis() as u64,
            cache_hit: false,
            papers: merged_papers,
            sources: statuses,
        }
    }

    // 1. OpenAlex API
    pub(crate) async fn fetch_openalex(
        &self,
        query: &str,
        limit: usize,
        year_min: Option<u32>,
        year_max: Option<u32>,
        creds: &SourceCredentials,
    ) -> Result<Vec<Paper>, String> {
        let mut url = format!(
            "https://api.openalex.org/works?search={}&per-page={}",
            urlencoding::encode(query),
            limit
        );
        // `mailto` joins the polite pool; `api_key` lifts the daily budget.
        if let Some(email) = SourceCredentials::clean(creds.openalex_email.clone()) {
            url.push_str(&format!("&mailto={}", urlencoding::encode(&email)));
        }
        if let Some(key) = SourceCredentials::clean(creds.openalex_api_key.clone()) {
            url.push_str(&format!("&api_key={}", urlencoding::encode(&key)));
        }
        if let Some(ymin) = year_min {
            url.push_str(&format!("&filter=from_publication_date:{}-01-01", ymin));
        }
        if let Some(ymax) = year_max {
            url.push_str(&format!(",to_publication_date:{}-12-31", ymax));
        }

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("OpenAlex: {}", e)),
        };

        // A 429/5xx body still parses as JSON, so without this check an exhausted
        // rate limit would be indistinguishable from "no results found".
        if !resp.status().is_success() {
            return Err(format!("OpenAlex returned HTTP {}", resp.status().as_u16()));
        }

        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => return Err(format!("OpenAlex: {}", e)),
        };

        let mut papers = Vec::new();
        if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
            for item in results {
                papers.push(openalex_paper(item, "", "OpenAlex"));
            }
        }
        Ok(papers)
    }

    // 2. PubMed API
    pub(crate) async fn fetch_pubmed(
        &self,
        query: &str,
        limit: usize,
        creds: &SourceCredentials,
    ) -> Result<Vec<Paper>, String> {
        // NCBI asks every client to identify itself; an api_key also lifts the
        // anonymous 3 req/s ceiling to 10 req/s.
        let mut ncbi_params = String::from("&tool=ScholarGate");
        if let Some(email) = SourceCredentials::clean(creds.ncbi_email.clone()) {
            ncbi_params.push_str(&format!("&email={}", urlencoding::encode(&email)));
        }
        if let Some(key) = SourceCredentials::clean(creds.ncbi_api_key.clone()) {
            ncbi_params.push_str(&format!("&api_key={}", urlencoding::encode(&key)));
        }

        let esearch_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmode=json&retmax={}{}",
            urlencoding::encode(query),
            limit,
            ncbi_params
        );

        let esearch_resp = match self.client.get(&esearch_url).send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("PubMed: {}", e)),
        };

        if !esearch_resp.status().is_success() {
            return Err(format!(
                "PubMed (esearch) returned HTTP {}",
                esearch_resp.status().as_u16()
            ));
        }

        let json: serde_json::Value = match esearch_resp.json().await {
            Ok(j) => j,
            Err(e) => return Err(format!("PubMed: {}", e)),
        };

        let id_list: Vec<String> = json
            .get("esearchresult")
            .and_then(|es| es.get("idlist"))
            .and_then(|l| l.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if id_list.is_empty() {
            return Ok(Vec::new());
        }

        let ids_joined = id_list.join(",");
        let esummary_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json{}",
            ids_joined,
            ncbi_params
        );

        let esummary_resp = match self.client.get(&esummary_url).send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("PubMed: {}", e)),
        };

        if !esummary_resp.status().is_success() {
            return Err(format!(
                "PubMed (esummary) returned HTTP {}",
                esummary_resp.status().as_u16()
            ));
        }

        let sum_json: serde_json::Value = match esummary_resp.json().await {
            Ok(j) => j,
            Err(e) => return Err(format!("PubMed: {}", e)),
        };

        let mut papers = Vec::new();
        if let Some(result_obj) = sum_json.get("result") {
            for pmid in &id_list {
                if let Some(item) = result_obj.get(pmid) {
                    let title = item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Untitled")
                        .trim_end_matches('.');
                    let pubdate = item.get("pubdate").and_then(|v| v.as_str()).unwrap_or("");
                    let year = pubdate
                        .split_whitespace()
                        .next()
                        .and_then(|y| y.parse::<u32>().ok());
                    let venue = item
                        .get("source")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let mut authors = Vec::new();
                    if let Some(auth_list) = item.get("authors").and_then(|a| a.as_array()) {
                        for a in auth_list.iter().take(crate::models::MAX_AUTHORS) {
                            if let Some(name) = a.get("name").and_then(|n| n.as_str()) {
                                authors.push(name.to_string());
                            }
                        }
                    }

                    // DOI + PMCID extraction from articleids
                    let mut doi = None;
                    let mut pmcid = None;
                    if let Some(aids) = item.get("articleids").and_then(|a| a.as_array()) {
                        for aid in aids {
                            match aid.get("idtype").and_then(|t| t.as_str()) {
                                Some("doi") => {
                                    doi = aid
                                        .get("value")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                }
                                Some("pmcid") => {
                                    pmcid = aid
                                        .get("value")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                }
                                _ => {}
                            }
                        }
                    }

                    // esummary exposes no downloadable full-text URL: a doi.org link is a
                    // publisher landing page, and PMC's /pdf/ route answers with HTML. Rather
                    // than invent a link that always fails, leave it empty — OpenAlex runs in
                    // the same fan-out and carries a verified oa_url, which the fusion step
                    // merges into this record when both sources return the same paper.
                    let _ = &pmcid;
                    let pdf_url: Option<String> = None;
                    let is_oa = false;

                    papers.push(Paper {
                        biblio: {
                            let text = |key: &str| {
                                item[key].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
                            };
                            crate::models::Biblio {
                                volume: text("volume"),
                                issue: text("issue"),
                                pages: text("pages"),
                                issn: text("issn").or_else(|| text("essn")),
                                ..Default::default()
                            }
                            .non_empty()
                        },
                        id: format!("pmid:{}", pmid),
                        title: title.to_string(),
                        authors,
                        year,
                        venue,
                        // esummary carries no abstract; a placeholder here would render
                        // in the UI as if it were the paper's abstract.
                        abstract_text: None,
                        doi,
                        source_url: Some(format!("https://pubmed.ncbi.nlm.nih.gov/{}/", pmid)),
                        pdf_url,
                        citations: None,
                        quartile: None,
                        source: "PubMed".to_string(),
                        score: None,
                        open_access: is_oa,
                    });
                }
            }
        }
        Ok(papers)
    }

    // 3. arXiv API
    pub(crate) async fn fetch_arxiv(&self, query: &str, limit: usize) -> Result<Vec<Paper>, String> {
        let url = format!(
            "https://export.arxiv.org/api/query?search_query=all:{}&start=0&max_results={}",
            urlencoding::encode(query),
            limit
        );

        let xml_text = polite_get(
            self.client.get(&url),
            &ARXIV_GATE,
            Duration::from_millis(3100),
            "arXiv",
        )
        .await?;

        parse_arxiv_feed(&xml_text)
    }

    pub(crate) async fn fetch_crossref(
        &self,
        query: &str,
        limit: usize,
        creds: &SourceCredentials,
    ) -> Result<Vec<Paper>, String> {
        // Crossref's polite pool keys off `mailto`; prefer whatever the user configured.
        let mailto = SourceCredentials::clean(creds.crossref_email.clone())
            .unwrap_or_else(|| "dqphong0302@gmail.com".to_string());
        let url = format!(
            "https://api.crossref.org/works?query={}&rows={}&mailto={}",
            urlencoding::encode(query),
            limit,
            urlencoding::encode(&mailto)
        );

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("Crossref: {}", e)),
        };

        if !resp.status().is_success() {
            return Err(format!("Crossref returned HTTP {}", resp.status().as_u16()));
        }

        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => return Err(format!("Crossref: {}", e)),
        };

        let mut papers = Vec::new();
        if let Some(items) = json
            .get("message")
            .and_then(|m| m.get("items"))
            .and_then(|i| i.as_array())
        {
            for item in items {
                let title = item
                    .get("title")
                    .and_then(|t| t.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");

                let doi = item
                    .get("DOI")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let year = item
                    .get("published-print")
                    .or_else(|| item.get("published-online"))
                    .and_then(|p| p.get("date-parts"))
                    .and_then(|dp| dp.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|y| y.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_u64())
                    .map(|y| y as u32);

                let venue = item
                    .get("container-title")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let mut authors = Vec::new();
                if let Some(auth_list) = item.get("author").and_then(|a| a.as_array()) {
                    for a in auth_list.iter().take(crate::models::MAX_AUTHORS) {
                        let given = a.get("given").and_then(|v| v.as_str()).unwrap_or("");
                        let family = a.get("family").and_then(|v| v.as_str()).unwrap_or("");
                        if !family.is_empty() {
                            authors.push(format!("{} {}", given, family).trim().to_string());
                        }
                    }
                }

                // Crossref advertises a real full text only through `link`; the bare
                // doi.org URL is a landing page and is frequently paywalled.
                let pdf_url = item.get("link").and_then(|l| l.as_array()).and_then(|arr| {
                    arr.iter()
                        .find(|l| {
                            l.get("content-type").and_then(|c| c.as_str())
                                == Some("application/pdf")
                        })
                        .and_then(|l| l.get("URL"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
                let is_oa = pdf_url.is_some();

                papers.push(Paper {
                    biblio: crossref_biblio(item),
                    id: doi.clone().unwrap_or_else(|| title.to_string()),
                    title: title.to_string(),
                    authors,
                    year,
                    venue,
                    abstract_text: None,
                    source_url: doi.as_ref().map(|value| format!("https://doi.org/{value}")),
                    doi,
                    pdf_url,
                    citations: None,
                    quartile: None,
                    source: "Crossref".to_string(),
                    score: None,
                    open_access: is_oa,
                });
            }
        }
        Ok(papers)
    }

    // 4b. Semantic Scholar Graph API (optional API key lifts the shared rate limit).
    pub(crate) async fn fetch_semantic_scholar(
        &self,
        query: &str,
        limit: usize,
        creds: &SourceCredentials,
    ) -> Result<Vec<Paper>, String> {
        let url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/search?query={}&limit={}&fields=title,authors,year,venue,abstract,externalIds,citationCount,openAccessPdf,url",
            urlencoding::encode(query),
            limit.min(100)
        );
        let mut request = self.client.get(&url);
        if let Some(key) = SourceCredentials::clean(creds.semantic_scholar_api_key.clone()) {
            request = request.header("x-api-key", key);
        }
        let response = polite_get(
            request,
            &S2_GATE,
            Duration::from_millis(1200),
            "Semantic Scholar",
        )
        .await?;
        let json: serde_json::Value = serde_json::from_str(&response)
            .map_err(|error| format!("Semantic Scholar: {}", error))?;

        let mut papers = Vec::new();
        if let Some(items) = json.get("data").and_then(|value| value.as_array()) {
            for item in items.iter().take(limit) {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                let paper_id = item.get("paperId").and_then(|v| v.as_str()).unwrap_or("");
                if paper_id.is_empty() && title == "Untitled" {
                    continue;
                }
                let doi = item
                    .get("externalIds")
                    .and_then(|ids| ids.get("DOI"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let authors = item
                    .get("authors")
                    .and_then(|value| value.as_array())
                    .map(|list| {
                        list.iter()
                            .filter_map(|author| author.get("name").and_then(|v| v.as_str()))
                            .take(crate::models::MAX_AUTHORS)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let pdf_url = item
                    .get("openAccessPdf")
                    .and_then(|pdf| pdf.get("url"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let source_url = item
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        (!paper_id.is_empty())
                            .then(|| format!("https://www.semanticscholar.org/paper/{paper_id}"))
                    });
                let open_access = pdf_url.is_some();

                papers.push(Paper {
                    biblio: None,
                    id: if paper_id.is_empty() {
                        doi.clone().unwrap_or_else(|| title.to_string())
                    } else {
                        format!("s2:{paper_id}")
                    },
                    title: title.to_string(),
                    authors,
                    year: item.get("year").and_then(|v| v.as_u64()).map(|y| y as u32),
                    venue: item
                        .get("venue")
                        .and_then(|v| v.as_str())
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    abstract_text: item
                        .get("abstract")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    doi,
                    source_url,
                    pdf_url,
                    citations: item
                        .get("citationCount")
                        .and_then(|v| v.as_u64())
                        .map(|c| c as u32),
                    quartile: None,
                    source: "Semantic Scholar".to_string(),
                    score: None,
                    open_access,
                });
            }
        }
        Ok(papers)
    }

    // 4c. DOAJ — peer-reviewed open-access journals.
    pub(crate) async fn fetch_doaj(&self, query: &str, limit: usize) -> Result<Vec<Paper>, String> {
        let url = format!(
            "https://doaj.org/api/search/articles/{}?pageSize={}",
            urlencoding::encode(query),
            limit
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|error| format!("DOAJ: {}", error))?;
        if !response.status().is_success() {
            return Err(format!("DOAJ returned HTTP {}", response.status().as_u16()));
        }
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|error| format!("DOAJ: {}", error))?;

        let mut papers = Vec::new();
        if let Some(items) = json.get("results").and_then(|value| value.as_array()) {
            for item in items.iter().take(limit) {
                let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let bib = item.get("bibjson");
                let title = bib
                    .and_then(|b| b.get("title"))
                    .and_then(|v| v.as_str())
                    .filter(|value| !value.trim().is_empty());
                let Some(title) = title else { continue };
                let authors = bib
                    .and_then(|b| b.get("author"))
                    .and_then(|value| value.as_array())
                    .map(|list| {
                        list.iter()
                            .filter_map(|author| author.get("name").and_then(|v| v.as_str()))
                            .take(crate::models::MAX_AUTHORS)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let year = bib
                    .and_then(|b| b.get("year"))
                    .and_then(|v| v.as_str())
                    .and_then(|value| value.parse::<u32>().ok());
                let venue = bib
                    .and_then(|b| b.get("journal"))
                    .and_then(|j| j.get("title"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let doi = bib
                    .and_then(|b| b.get("identifier"))
                    .and_then(|value| value.as_array())
                    .and_then(|ids| {
                        ids.iter().find_map(|entry| {
                            (entry.get("type").and_then(|v| v.as_str()) == Some("doi"))
                                .then(|| {
                                    entry.get("id").and_then(|v| v.as_str()).map(str::to_string)
                                })
                                .flatten()
                        })
                    });
                let pdf_url = bib
                    .and_then(|b| b.get("link"))
                    .and_then(|value| value.as_array())
                    .and_then(|links| {
                        links.iter().find_map(|link| {
                            let fulltext = link
                                .get("type")
                                .and_then(|v| v.as_str())
                                .is_some_and(|kind| kind.contains("fulltext"));
                            fulltext
                                .then(|| {
                                    link.get("url").and_then(|v| v.as_str()).map(str::to_string)
                                })
                                .flatten()
                        })
                    });
                papers.push(Paper {
                    biblio: None,
                    id: format!("doaj:{id}"),
                    title: title.to_string(),
                    authors,
                    year,
                    venue,
                    abstract_text: bib
                        .and_then(|b| b.get("abstract"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    doi,
                    source_url: (!id.is_empty()).then(|| format!("https://doaj.org/article/{id}")),
                    pdf_url,
                    citations: None,
                    quartile: None,
                    source: "DOAJ".to_string(),
                    score: None,
                    open_access: true,
                });
            }
        }
        Ok(papers)
    }

    // 4d. Zenodo — open-science repository (records, preprints, data, software).
    pub(crate) async fn fetch_zenodo(&self, query: &str, limit: usize) -> Result<Vec<Paper>, String> {
        // Unauthenticated Zenodo requests reject size > 25 with HTTP 400.
        let url = format!(
            "https://zenodo.org/api/records?q={}&size={}",
            urlencoding::encode(query),
            limit.clamp(1, 25)
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|error| format!("Zenodo: {}", error))?;
        if !response.status().is_success() {
            return Err(format!(
                "Zenodo returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|error| format!("Zenodo: {}", error))?;

        let mut papers = Vec::new();
        if let Some(hits) = json
            .get("hits")
            .and_then(|value| value.get("hits"))
            .and_then(|value| value.as_array())
        {
            for hit in hits.iter().take(limit) {
                let metadata = hit.get("metadata");
                let title = metadata
                    .and_then(|m| m.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                let record_id = hit.get("id").and_then(|v| v.as_u64());
                let authors = metadata
                    .and_then(|m| m.get("creators"))
                    .and_then(|value| value.as_array())
                    .map(|list| {
                        list.iter()
                            .filter_map(|creator| creator.get("name").and_then(|v| v.as_str()))
                            .take(crate::models::MAX_AUTHORS)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let year = metadata
                    .and_then(|m| m.get("publication_date"))
                    .and_then(|v| v.as_str())
                    .and_then(|date| date.get(0..4))
                    .and_then(|year| year.parse::<u32>().ok());
                let pdf_url = hit
                    .get("files")
                    .and_then(|value| value.as_array())
                    .and_then(|files| {
                        files.iter().find_map(|file| {
                            let is_pdf = file
                                .get("key")
                                .and_then(|v| v.as_str())
                                .is_some_and(|key| key.to_lowercase().ends_with(".pdf"));
                            is_pdf
                                .then(|| {
                                    file.get("links")
                                        .and_then(|l| l.get("self"))
                                        .and_then(|v| v.as_str())
                                        .map(str::to_string)
                                })
                                .flatten()
                        })
                    });
                let source_url = hit
                    .get("links")
                    .and_then(|links| links.get("self_html"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or_else(|| record_id.map(|id| format!("https://zenodo.org/records/{id}")));
                papers.push(Paper {
                    biblio: None,
                    id: record_id
                        .map(|id| format!("zenodo:{id}"))
                        .unwrap_or_else(|| title.to_string()),
                    title: title.to_string(),
                    authors,
                    year,
                    venue: metadata
                        .and_then(|m| m.get("resource_type"))
                        .and_then(|rt| rt.get("title"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .or_else(|| Some("Zenodo".to_string())),
                    abstract_text: metadata
                        .and_then(|m| m.get("description"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    doi: metadata
                        .and_then(|m| m.get("doi"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    source_url,
                    pdf_url,
                    citations: None,
                    quartile: None,
                    source: "Zenodo".to_string(),
                    score: None,
                    open_access: true,
                });
            }
        }
        Ok(papers)
    }

    // 4e. HAL — French open archive, strong in humanities and social sciences.
    pub(crate) async fn fetch_hal(&self, query: &str, limit: usize) -> Result<Vec<Paper>, String> {
        let url = format!(
            "https://api.archives-ouvertes.fr/search/?q={}&fl=title_s,authFullName_s,producedDateY_i,doiId_s,uri_s,journalTitle_s,conferenceTitle_s,abstract_s,fileMain_s,halId_s&rows={}&wt=json",
            urlencoding::encode(query),
            limit
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|error| format!("HAL: {}", error))?;
        if !response.status().is_success() {
            return Err(format!("HAL returned HTTP {}", response.status().as_u16()));
        }
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|error| format!("HAL: {}", error))?;

        let mut papers = Vec::new();
        if let Some(docs) = json
            .get("response")
            .and_then(|value| value.get("docs"))
            .and_then(|value| value.as_array())
        {
            for doc in docs.iter().take(limit) {
                // HAL returns some fields as a string and others as an array.
                let field = |key: &str| -> Option<String> {
                    match doc.get(key) {
                        Some(serde_json::Value::String(value)) => Some(value.clone()),
                        Some(serde_json::Value::Array(list)) => {
                            list.first().and_then(|v| v.as_str()).map(str::to_string)
                        }
                        _ => None,
                    }
                };
                let Some(title) = field("title_s") else {
                    continue;
                };
                let authors = match doc.get("authFullName_s") {
                    Some(serde_json::Value::Array(list)) => list
                        .iter()
                        .filter_map(|v| v.as_str())
                        .take(crate::models::MAX_AUTHORS)
                        .map(str::to_string)
                        .collect::<Vec<_>>(),
                    Some(serde_json::Value::String(name)) => vec![name.clone()],
                    _ => Vec::new(),
                };
                let hal_id = field("halId_s");
                let uri = field("uri_s");
                let pdf_url =
                    field("fileMain_s").filter(|url| url.to_lowercase().ends_with(".pdf"));
                papers.push(Paper {
                    biblio: None,
                    id: format!("hal:{}", hal_id.clone().unwrap_or_else(|| title.clone())),
                    title,
                    authors,
                    year: doc
                        .get("producedDateY_i")
                        .and_then(|v| v.as_u64())
                        .map(|y| y as u32),
                    venue: field("journalTitle_s").or_else(|| field("conferenceTitle_s")),
                    abstract_text: field("abstract_s"),
                    doi: field("doiId_s"),
                    source_url: uri
                        .or_else(|| hal_id.map(|id| format!("https://hal.science/{id}"))),
                    pdf_url,
                    citations: None,
                    quartile: None,
                    source: "HAL".to_string(),
                    score: None,
                    open_access: true,
                });
            }
        }
        Ok(papers)
    }

    // 5. OpenAlex filtered to works affiliated with Vietnamese institutions.
    pub(crate) async fn fetch_vietnam_openalex(
        &self,
        query: &str,
        limit: usize,
        creds: &SourceCredentials,
    ) -> Result<Vec<Paper>, String> {
        let mut url = format!(
            "https://api.openalex.org/works?search={}&filter=institutions.country_code:VN&per-page={}",
            urlencoding::encode(query),
            limit
        );
        // Same API as OpenAlex, so the same quota and the same credentials apply.
        if let Some(email) = SourceCredentials::clean(creds.openalex_email.clone()) {
            url.push_str(&format!("&mailto={}", urlencoding::encode(&email)));
        }
        if let Some(key) = SourceCredentials::clean(creds.openalex_api_key.clone()) {
            url.push_str(&format!("&api_key={}", urlencoding::encode(&key)));
        }

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("OpenAlex Vietnam: {}", e)),
        };

        if !resp.status().is_success() {
            return Err(format!(
                "OpenAlex Vietnam returned HTTP {}",
                resp.status().as_u16()
            ));
        }

        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => return Err(format!("OpenAlex Vietnam: {}", e)),
        };

        let mut papers = Vec::new();
        if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
            for item in results {
                papers.push(openalex_paper(item, "vn:", "OpenAlex Vietnam"));
            }
        }
        Ok(papers)
    }

    async fn enrich_with_unpaywall(&self, papers: &mut [Paper], email: &str) {
        use futures::stream::{self, StreamExt};

        let lookups = papers
            .iter()
            .enumerate()
            .filter_map(|(index, paper)| {
                if paper.pdf_url.is_some() {
                    None
                } else {
                    paper.doi.as_ref().map(|doi| (index, doi.clone()))
                }
            })
            .take(8)
            .collect::<Vec<_>>();

        let client = self.client.clone();
        let email = email.to_string();
        let resolved = stream::iter(lookups.into_iter().map(|(index, doi)| {
            let client = client.clone();
            let email = email.clone();
            async move {
                let url = format!(
                    "https://api.unpaywall.org/v2/{}?email={}",
                    urlencoding::encode(doi.trim()),
                    urlencoding::encode(&email)
                );
                let json = client
                    .get(url)
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                    .ok()?
                    .error_for_status()
                    .ok()?
                    .json::<serde_json::Value>()
                    .await
                    .ok()?;
                Some((index, unpaywall_pdf_url(&json)?))
            }
        }))
        .buffer_unordered(4)
        .filter_map(|result| async move { result })
        .collect::<Vec<_>>()
        .await;

        for (index, pdf_url) in resolved {
            if let Some(paper) = papers.get_mut(index) {
                paper.pdf_url = Some(pdf_url);
                paper.open_access = true;
            }
        }
    }

    // Reciprocal Rank Fusion algorithm (RRF k=60)
    fn reciprocal_rank_fusion(
        &self,
        ranked_lists: Vec<Vec<Paper>>,
        k: f64,
        max_limit: usize,
    ) -> Vec<Paper> {
        let mut score_map: HashMap<String, f64> = HashMap::new();
        let mut paper_map: HashMap<String, Paper> = HashMap::new();

        for list in ranked_lists {
            for (rank, paper) in list.into_iter().enumerate() {
                let key = normalize_paper_key(&paper);
                let rrf_score = 1.0 / (k + rank as f64 + 1.0);

                *score_map.entry(key.clone()).or_insert(0.0) += rrf_score;

                // Keep the most informative metadata: when several sources return the same
                // paper, fill every field the winning record is missing. Inserting only the
                // first record would discard a verified open-access URL, abstract or citation
                // count just because a sparser source happened to be ranked earlier.
                match paper_map.entry(key.clone()) {
                    Entry::Occupied(mut existing) => {
                        merge_paper_metadata(existing.get_mut(), paper)
                    }
                    Entry::Vacant(slot) => {
                        slot.insert(paper);
                    }
                }
            }
        }

        let mut sorted_keys: Vec<(String, f64)> = score_map.into_iter().collect();
        sorted_keys.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut final_papers = Vec::new();
        for (key, score) in sorted_keys.into_iter().take(max_limit) {
            if let Some(mut paper) = paper_map.remove(&key) {
                paper.score = Some((score * 1000.0).round() / 1000.0);
                final_papers.push(paper);
            }
        }

        final_papers
    }

    // 6. Bundled meta-search adapter. It runs inside the Rust gateway and uses
    // Europe PMC's public API, so packaged builds need neither Docker nor Python.
    pub(crate) async fn fetch_europe_pmc(&self, query: &str, limit: usize) -> Result<Vec<Paper>, String> {
        let url = format!(
            "https://www.ebi.ac.uk/europepmc/webservices/rest/search?query={}&format=json&pageSize={}&resultType=core",
            urlencoding::encode(query),
            limit
        );
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| format!("Europe PMC: {}", error))?;
        if !response.status().is_success() {
            return Err(format!(
                "Europe PMC returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|error| format!("Europe PMC: {}", error))?;

        let mut papers = Vec::new();
        if let Some(results) = json
            .get("resultList")
            .and_then(|list| list.get("result"))
            .and_then(|value| value.as_array())
        {
            for item in results.iter().take(limit) {
                let title = item
                    .get("title")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Untitled")
                    .trim_end_matches('.');
                let external_id = item
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or(title);
                let source_id = item
                    .get("source")
                    .and_then(|value| value.as_str())
                    .unwrap_or("EPMC");
                let authors = item
                    .get("authorList")
                    .and_then(|list| list.get("author"))
                    .and_then(|value| value.as_array())
                    .map(|authors| {
                        authors
                            .iter()
                            .filter_map(|author| {
                                author.get("fullName").and_then(|value| value.as_str())
                            })
                            .take(crate::models::MAX_AUTHORS)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let pdf_url = item
                    .get("fullTextUrlList")
                    .and_then(|list| list.get("fullTextUrl"))
                    .and_then(|value| value.as_array())
                    .and_then(|urls| {
                        urls.iter().find_map(|entry| {
                            let free = entry
                                .get("availabilityCode")
                                .and_then(|value| value.as_str())
                                == Some("F");
                            let pdf = entry.get("documentStyle").and_then(|value| value.as_str())
                                == Some("pdf");
                            (free && pdf)
                                .then(|| {
                                    entry
                                        .get("url")
                                        .and_then(|value| value.as_str())
                                        .map(str::to_string)
                                })
                                .flatten()
                        })
                    });
                // Europe PMC separates "open access" from "free to read on our
                // site". Treating any free PDF link as open access put an OPEN
                // ACCESS badge on subscription articles whose PDF link is behind
                // bot protection and cannot be fetched at all. The link is still
                // worth keeping — it opens in a browser — but the badge has to
                // report what the source actually says.
                let is_open_access = item
                    .get("isOpenAccess")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| value == "Y");

                papers.push(Paper {
                    biblio: {
                        let text = |value: &serde_json::Value| {
                            value.as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
                        };
                        let journal = &item["journalInfo"];
                        crate::models::Biblio {
                            volume: text(&journal["volume"]),
                            issue: text(&journal["issue"]),
                            pages: text(&item["pageInfo"]),
                            issn: text(&journal["journal"]["issn"])
                                .or_else(|| text(&journal["journal"]["essn"])),
                            keywords: item["keywordList"]["keyword"]
                                .as_array()
                                .map(|list| list.iter().filter_map(text).collect())
                                .unwrap_or_default(),
                            ..Default::default()
                        }
                        .non_empty()
                    },
                    id: format!("epmc:{}:{}", source_id, external_id),
                    title: title.to_string(),
                    authors,
                    year: item
                        .get("pubYear")
                        .and_then(|value| value.as_str())
                        .and_then(|value| value.parse::<u32>().ok()),
                    venue: item
                        .get("journalTitle")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    abstract_text: item
                        .get("abstractText")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    doi: item
                        .get("doi")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    source_url: Some(format!(
                        "https://europepmc.org/article/{}/{}",
                        source_id, external_id
                    )),
                    pdf_url,
                    citations: item
                        .get("citedByCount")
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32),
                    quartile: None,
                    source: "Europe PMC".to_string(),
                    score: None,
                    open_access: is_open_access,
                });
            }
        }
        Ok(papers)
    }

    // Optional external SearXNG adapter for users who already operate one.
    pub async fn fetch_searxng(
        &self,
        query: &str,
        limit: usize,
        base_url: &str,
        categories: Option<&str>,
        engines: Option<&str>,
    ) -> Result<Vec<Paper>, String> {
        let clean_base = base_url.trim_end_matches('/');
        let cat = categories.unwrap_or("science");
        let mut url = format!(
            "{}/search?q={}&categories={}&format=json",
            clean_base,
            urlencoding::encode(query),
            cat
        );
        if let Some(eng) = engines {
            if !eng.trim().is_empty() {
                url.push_str(&format!("&engines={}", urlencoding::encode(eng.trim())));
            }
        }

        let resp = match self.client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
            .header("Accept", "application/json")
            .send()
            .await {
            Ok(r) => r,
            Err(e) => return Err(format!("SearXNG: {}", e)),
        };

        if !resp.status().is_success() {
            return Err(format!("SearXNG returned HTTP {}", resp.status().as_u16()));
        }

        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => return Err(format!("SearXNG: {}", e)),
        };

        let mut papers = Vec::new();
        if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
            for item in results.iter().take(limit) {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if title.is_empty() {
                    continue;
                }

                let url_str = item
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let content = item
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let engine = item
                    .get("engine")
                    .and_then(|v| v.as_str())
                    .unwrap_or("searxng")
                    .to_string();

                let id = format!(
                    "searxng_{:x}",
                    md5::compute(format!("{}:{}", title, url_str))
                );
                let is_pdf = url_str.ends_with(".pdf") || content.contains("[PDF]");

                // Extract year if available
                let published = item
                    .get("publishedDate")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let year = if published.len() >= 4 {
                    published[..4].parse::<u32>().ok()
                } else {
                    None
                };

                let doi = item
                    .get("doi")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                papers.push(Paper {
                    biblio: None,
                    id,
                    title,
                    authors: vec!["SearXNG Federated".to_string()],
                    year,
                    venue: Some(format!("SearXNG ({})", engine)),
                    abstract_text: if content.is_empty() {
                        None
                    } else {
                        Some(content)
                    },
                    doi,
                    source_url: (!url_str.is_empty()).then(|| url_str.clone()),
                    pdf_url: if is_pdf { Some(url_str.clone()) } else { None },
                    citations: None,
                    quartile: None,
                    source: format!("SearXNG / {}", engine),
                    score: None,
                    open_access: is_pdf,
                });
            }
        }
        Ok(papers)
    }
}

// Helpers
/// Map one OpenAlex work object into a `Paper`. Shared by the search engine and
/// the citation-graph lookups so both stay in sync.
pub(crate) fn openalex_paper(item: &serde_json::Value, id_prefix: &str, source: &str) -> Paper {
    let raw_id = item.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    let title = item
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled");
    let doi = item
        .get("doi")
        .and_then(|v| v.as_str())
        .map(|d| d.replace("https://doi.org/", ""));
    let year = item
        .get("publication_year")
        .and_then(|v| v.as_u64())
        .map(|y| y as u32);
    let citations = item
        .get("cited_by_count")
        .and_then(|v| v.as_u64())
        .map(|c| c as u32);
    let venue = item
        .get("primary_location")
        .and_then(|l| l.get("source"))
        .and_then(|s| s.get("display_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let pdf_url = item
        .get("best_oa_location")
        .and_then(|location| location.get("pdf_url"))
        .or_else(|| {
            item.get("primary_location")
                .and_then(|location| location.get("pdf_url"))
        })
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let open_access = item
        .get("open_access")
        .and_then(|oa| oa.get("is_oa"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let authors = item
        .get("authorships")
        .and_then(|a| a.as_array())
        .map(|list| {
            list.iter()
                .take(crate::models::MAX_AUTHORS)
                .filter_map(|a| {
                    a.get("author")
                        .and_then(|au| au.get("display_name"))
                        .and_then(|n| n.as_str())
                })
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let abstract_text = reconstruct_abstract(item.get("abstract_inverted_index"));
    let text = |value: &serde_json::Value| value.as_str().map(str::to_string);
    let source_meta = &item["primary_location"]["source"];
    let biblio = crate::models::Biblio {
        volume: text(&item["biblio"]["volume"]),
        issue: text(&item["biblio"]["issue"]),
        pages: crate::models::page_range(
            item["biblio"]["first_page"].as_str(),
            item["biblio"]["last_page"].as_str(),
        ),
        issn: text(&source_meta["issn_l"]),
        publisher: text(&source_meta["host_organization_name"]),
        keywords: item["keywords"]
            .as_array()
            .map(|list| list.iter().filter_map(|k| text(&k["display_name"])).collect())
            .unwrap_or_default(),
        topic: text(&item["primary_topic"]["display_name"]),
        field: text(&item["primary_topic"]["field"]["display_name"]),
    }
    .non_empty();

    Paper {
        biblio,
        id: format!("{id_prefix}{raw_id}"),
        title: title.to_string(),
        authors,
        year,
        venue,
        abstract_text,
        doi,
        source_url: (!raw_id.is_empty()).then(|| raw_id.to_string()),
        pdf_url,
        citations,
        quartile: None,
        source: source.to_string(),
        score: None,
        open_access,
    }
}

/// Fill in every field `target` is missing from a duplicate record of the same paper.
fn merge_paper_metadata(target: &mut Paper, other: Paper) {
    if target.pdf_url.is_none() {
        target.pdf_url = other.pdf_url;
    }
    // Free full text found by any source makes the paper open access.
    target.open_access = target.open_access || other.open_access;
    if target.abstract_text.is_none() {
        target.abstract_text = other.abstract_text;
    }
    if target.doi.is_none() {
        target.doi = other.doi;
    }
    if target.source_url.is_none() {
        target.source_url = other.source_url;
    }
    if target.year.is_none() {
        target.year = other.year;
    }
    if target.venue.is_none() {
        target.venue = other.venue;
    }
    if target.citations.is_none() {
        target.citations = other.citations;
    }
    if target.quartile.is_none() {
        target.quartile = other.quartile;
    }
    if target.authors.is_empty() {
        target.authors = other.authors;
    }
    match (&mut target.biblio, other.biblio) {
        (Some(mine), Some(theirs)) => mine.fill_from(theirs),
        (slot @ None, theirs) => *slot = theirs,
        _ => {}
    }
}

/// Crossref's volume/issue/page/ISSN/publisher/subject, shared by search and DOI lookup.
pub(crate) fn crossref_biblio(item: &serde_json::Value) -> Option<crate::models::Biblio> {
    let text = |value: &serde_json::Value| {
        value.as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
    };
    crate::models::Biblio {
        volume: text(&item["volume"]),
        issue: text(&item["issue"]),
        pages: text(&item["page"]).or_else(|| text(&item["article-number"])),
        issn: text(&item["ISSN"][0]),
        publisher: text(&item["publisher"]),
        keywords: item["subject"]
            .as_array()
            .map(|list| list.iter().filter_map(text).collect())
            .unwrap_or_default(),
        ..Default::default()
    }
    .non_empty()
}

fn normalize_paper_key(paper: &Paper) -> String {
    if let Some(doi) = &paper.doi {
        if !doi.is_empty() {
            return format!("doi:{}", doi.to_lowercase().trim());
        }
    }
    paper
        .title
        .to_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn reconstruct_abstract(val: Option<&serde_json::Value>) -> Option<String> {
    let obj = val?.as_object()?;
    let mut words_with_pos: Vec<(usize, String)> = Vec::new();

    for (word, pos_val) in obj {
        if let Some(positions) = pos_val.as_array() {
            for p in positions {
                if let Some(pos) = p.as_u64() {
                    words_with_pos.push((pos as usize, word.clone()));
                }
            }
        }
    }

    if words_with_pos.is_empty() {
        return None;
    }

    words_with_pos.sort_by_key(|w| w.0);
    let abstract_str = words_with_pos
        .into_iter()
        .map(|w| w.1)
        .collect::<Vec<_>>()
        .join(" ");
    Some(abstract_str)
}

/// The best location often has only a landing page while a repository copy
/// (PMC, arXiv, an institutional archive) carries the PDF, so fall back to
/// the other open-access locations before giving up.
pub(crate) fn unpaywall_pdf_urls(json: &serde_json::Value) -> Vec<String> {
    let others = json.get("oa_locations").and_then(|v| v.as_array());
    std::iter::once(json.get("best_oa_location"))
        .chain(others.into_iter().flatten().map(Some))
        .flatten()
        .filter_map(|location| location.get("url_for_pdf")?.as_str())
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .collect()
}

fn unpaywall_pdf_url(json: &serde_json::Value) -> Option<String> {
    unpaywall_pdf_urls(json).into_iter().next()
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    let start = xml.find(&start_tag)? + start_tag.len();
    let end = xml[start..].find(&end_tag)? + start;
    Some(xml[start..end].trim().to_string())
}

/// Expand each preset independently so explicit source selections are never lost.
/// None means all sources; an explicit empty list means no sources.
fn matches_year_range(year: Option<u32>, min: Option<u32>, max: Option<u32>) -> bool {
    if min.is_none() && max.is_none() {
        return true;
    }
    year.is_some_and(|year| min.is_none_or(|min| year >= min) && max.is_none_or(|max| year <= max))
}

pub(crate) fn resolve_sources(sources: Option<&[String]>, query: &str) -> Option<Vec<String>> {
    let sources = sources?;
    let mut resolved = Vec::new();
    for source in sources {
        let normalized = source.trim().to_lowercase();
        let canonical = match normalized.as_str() {
            // Vietnam aliases
            "vietnam_medical"
            | "vietnam_academic"
            | "vietnam_science"
            | "vietnam_engineering"
            | "vietnam_agriculture"
            | "vietnam_economics"
            | "vietnam_education"
            | "vietnam_social" => "vietnam",
            // Biomedical aliases
            "biomedical_full"
            | "clinical_trials"
            | "pharma_clinical"
            | "pharma_deep"
            | "bioinformatics"
            | "global_health"
            | "fulltext_biomedical"
            | "medical" => "biomedical",
            // CS/AI aliases
            "cs_ai" | "ml_ai_deep" | "nlp_llm" | "nlp_acl" | "cv_vision" | "code_knowledge"
            | "cyber_security" | "datasets" => "ai_cs",
            // STEM aliases
            "physics" | "chemistry" | "energy_materials" | "aerospace" | "agriculture"
            | "engineering" | "natural_sciences" => "stem_nature",
            // Social / Humanities aliases
            "economics" | "education" | "humanities" | "books" | "theses" | "social_sciences"
            | "law" | "environment" => "social_humanities",
            // Evidence Review aliases
            "systematic_review" | "evidence_based" => "evidence_review",
            // Patents & Gov aliases
            "patents" | "us_gov" | "funding" => "patents_gov",
            // Regional aliases
            "asia_pacific" | "global_south" | "africa" | "latin_america" => "global_regional",
            other => other,
        };

        let expanded = match canonical {
            "all" | "exhaustive" | "international" => return None,
            "auto" => detect_sources_for_query(query),
            "searxng" => vec!["searxng"],
            other => crate::catalog::catalog()
                .presets
                .iter()
                .find(|preset| preset.id == other)
                .map(|preset| preset.sources.iter().map(String::as_str).collect())
                .unwrap_or_else(|| vec![other]),
        };
        for item in expanded {
            if !resolved.iter().any(|existing| existing == item) {
                resolved.push(item.to_string());
            }
        }
    }
    Some(resolved)
}

fn parse_arxiv_feed(xml_text: &str) -> Result<Vec<Paper>, String> {
    if !xml_text.contains("<feed") || !xml_text.contains("</feed>") {
        return Err("arXiv: response is not a complete Atom feed".into());
    }
    if xml_text.contains("arxiv.org/api/errors") {
        return Err("arXiv: API returned an error entry, not paper results".into());
    }
    let mut papers = Vec::new();
    let entries: Vec<&str> = xml_text.split("<entry>").skip(1).collect();

    for entry in entries {
        let title = extract_xml_tag(entry, "title")
            .map(|t| t.trim().replace("\n", " "))
            .unwrap_or_else(|| "Untitled".to_string());
        let summary = extract_xml_tag(entry, "summary").map(|s| s.trim().replace("\n", " "));
        let id = extract_xml_tag(entry, "id").unwrap_or_default();
        let published = extract_xml_tag(entry, "published").unwrap_or_default();
        let year = published
            .split('-')
            .next()
            .and_then(|y| y.parse::<u32>().ok());

        // Each <entry> carries one <name> per author.
        let mut authors: Vec<String> = Vec::new();
        let mut rest: &str = entry;
        while let Some(start) = rest.find("<name>") {
            let after = &rest[start + "<name>".len()..];
            match after.find("</name>") {
                Some(end) => {
                    let name = after[..end].trim();
                    if !name.is_empty() {
                        authors.push(name.to_string());
                    }
                    rest = &after[end..];
                }
                None => break,
            }
        }
        authors.truncate(5);

        let arxiv_id = id.split("/abs/").nth(1).unwrap_or("");
        let pdf_url = if !arxiv_id.is_empty() {
            Some(format!("https://arxiv.org/pdf/{}.pdf", arxiv_id))
        } else {
            None
        };
        // The Atom feed still reports entry ids over plain http; hand the reader
        // the canonical https link instead.
        let landing = if !arxiv_id.is_empty() {
            format!("https://arxiv.org/abs/{arxiv_id}")
        } else {
            id.replacen("http://", "https://", 1)
        };

        papers.push(Paper {
            biblio: None,
            id: id.clone(),
            title,
            authors,
            year,
            venue: Some("arXiv Preprint".to_string()),
            abstract_text: summary,
            doi: None,
            source_url: (!landing.is_empty()).then_some(landing),
            pdf_url,
            citations: None,
            quartile: None,
            source: "arXiv".to_string(),
            score: None,
            open_access: true,
        });
    }

    Ok(papers)
}

fn detect_sources_for_query(query: &str) -> Vec<&'static str> {
    let q = query.to_lowercase();
    let is_medical = q.contains("cancer")
        || q.contains("ung thư")
        || q.contains("tim mạch")
        || q.contains("diabetes")
        || q.contains("đái tháo đường")
        || q.contains("bệnh")
        || q.contains("y học")
        || q.contains("y tế")
        || q.contains("dược")
        || q.contains("thuốc")
        || q.contains("clinical")
        || q.contains("patient")
        || q.contains("điều trị")
        || q.contains("vaccine")
        || q.contains("virus")
        || q.contains("surgery")
        || q.contains("glp-1")
        || q.contains("cardio")
        || q.contains("liver")
        || q.contains("kidney")
        || q.contains("therapy");

    let is_cs_ai = q
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == "ai")
        || q.contains("llm")
        || q.contains("machine learning")
        || q.contains("deep learning")
        || q.contains("neural")
        || q.contains("transformer")
        || q.contains("algorithm")
        || q.contains("mamba")
        || q.contains("trí tuệ nhân tạo")
        || q.contains("language model")
        || q.contains("vision")
        || q.contains("code")
        || q.contains("computer")
        || q.contains("robot")
        || q.contains("gpu");

    let is_vn = q
        .chars()
        .any(|c| "àáạảãâầấậẩẫăằắặẳẵèéẹẻẽêềếệểễìíịỉĩòóọỏõôồốộổỗơờớợởỡùúụủũưừứựửữỳýỵỷỹđ".contains(c));

    if is_medical {
        if is_vn {
            vec!["pubmed", "vietnam", "openalex", "crossref", "doaj"]
        } else {
            vec!["pubmed", "openalex", "crossref", "doaj"]
        }
    } else if is_cs_ai {
        if is_vn {
            vec![
                "arxiv",
                "vietnam",
                "openalex",
                "crossref",
                "semantic_scholar",
            ]
        } else {
            vec!["arxiv", "openalex", "crossref", "semantic_scholar"]
        }
    } else if is_vn {
        vec!["vietnam", "openalex", "crossref"]
    } else {
        vec!["openalex", "arxiv", "crossref", "semantic_scholar", "hal"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_supports_seconds_and_http_dates() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "120".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(120)));
        headers.insert(
            "retry-after",
            (chrono::Utc::now() + chrono::Duration::seconds(120))
                .to_rfc2822()
                .parse()
                .unwrap(),
        );
        assert!(retry_after(&headers).unwrap().as_secs() >= 118);
        headers.insert("retry-after", "invalid".parse().unwrap());
        assert_eq!(retry_after(&headers), None);
    }

    #[tokio::test]
    async fn source_retries_recover_and_long_cooldowns_are_shared() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new()
            .route(
                "/recover",
                axum::routing::get(move || {
                    let n = seen.fetch_add(1, Ordering::SeqCst);
                    async move {
                        (
                            if n == 0 {
                                axum::http::StatusCode::TOO_MANY_REQUESTS
                            } else if n == 1 {
                                axum::http::StatusCode::SERVICE_UNAVAILABLE
                            } else {
                                axum::http::StatusCode::OK
                            },
                            [("retry-after", "0")],
                            "response",
                        )
                    }
                }),
            )
            .route(
                "/cooldown",
                axum::routing::get(|| async {
                    (
                        axum::http::StatusCode::TOO_MANY_REQUESTS,
                        [("retry-after", "120")],
                        "limited",
                    )
                }),
            )
            .route(
                "/invalid",
                axum::routing::get(|| async { axum::http::StatusCode::UNAUTHORIZED }),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = AcademicEngine::new().client;
        let gate = Mutex::new(None);
        let response = polite_get(
            client.get(format!("{url}/recover")),
            &gate,
            Duration::ZERO,
            "test",
        )
        .await
        .unwrap();
        assert_eq!(response, "response");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let error = polite_get(
            client.get(format!("{url}/invalid")),
            &gate,
            Duration::ZERO,
            "test",
        )
        .await
        .unwrap_err();
        assert!(error.contains("HTTP 401"));
        let error = polite_get(
            client.get(format!("{url}/cooldown")),
            &gate,
            Duration::ZERO,
            "test",
        )
        .await
        .unwrap_err();
        assert!(error.contains("after 1 attempt"));
        let error = polite_get(
            client.get(format!("{url}/recover")),
            &gate,
            Duration::ZERO,
            "test",
        )
        .await
        .unwrap_err();
        assert!(error.contains("cooldown"));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        server.abort();
    }

    #[tokio::test]
    async fn source_gate_serializes_complete_response_bodies() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let active = Arc::new(AtomicUsize::new(0));
        let counter = active.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/",
            axum::routing::get(move || {
                let counter = counter.clone();
                async move {
                    assert_eq!(
                        counter.fetch_add(1, Ordering::SeqCst),
                        0,
                        "overlapping requests"
                    );
                    axum::body::Body::from_stream(futures::stream::once(async move {
                        tokio::time::sleep(Duration::from_millis(60)).await;
                        counter.fetch_sub(1, Ordering::SeqCst);
                        Ok::<_, std::io::Error>("complete body")
                    }))
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = AcademicEngine::new().client;
        let gate = Mutex::new(None);
        let (first, second) = tokio::join!(
            polite_get(client.get(&url), &gate, Duration::ZERO, "test"),
            polite_get(client.get(&url), &gate, Duration::ZERO, "test")
        );
        assert_eq!(first.unwrap(), "complete body");
        assert_eq!(second.unwrap(), "complete body");
        assert_eq!(active.load(Ordering::SeqCst), 0);
        server.abort();
    }

    /// The whole fan-out is capped, and a source that runs past the cap is
    /// reported rather than silently dropped. Before the budget existed, every
    /// search waited for its slowest source — the server-rendered journal
    /// portals, which are allowed half a minute each — so one of them set the
    /// latency of every query.
    #[test]
    fn the_search_budget_is_derived_from_the_configured_timeout_and_bounded() {
        // Default settings.
        assert_eq!(
            AcademicEngine::new().search_budget(),
            Duration::from_secs(24)
        );
        // A very short timeout still leaves slow sources a usable floor.
        assert_eq!(
            AcademicEngine::with_timeout(1).search_budget(),
            Duration::from_secs(10)
        );
        // A very long one is still capped: a search always returns.
        assert_eq!(
            AcademicEngine::with_timeout(120).search_budget(),
            Duration::from_secs(40)
        );
    }

    /// Two sources behind one upstream rate limit must queue behind one gate.
    /// Every NCBI E-utilities endpoint counts against the same per-IP budget,
    /// so spacing them individually would still have exceeded it.
    /// Europe PMC returns structured abstracts whose section headings are HTML,
    /// and they reached the UI as literal `<h4>Background</h4>` text.
    #[test]
    fn markup_from_a_source_never_reaches_a_result() {
        let mut paper = paper("europe_pmc", "10.1/x");
        paper.title = "CRISPR <i>in vivo</i> editing".into();
        paper.abstract_text = Some("<h4>Background</h4>Hereditary angioedema is rare.".into());
        normalize_paper_text(&mut paper);
        assert_eq!(paper.title, "CRISPR in vivo editing");
        assert_eq!(
            paper.abstract_text.as_deref(),
            Some("BackgroundHereditary angioedema is rare.")
        );
    }

    /// Stripping everything between angle brackets would eat the middle of an
    /// abstract that states an inequality, so text without a real tag is left
    /// exactly as the source sent it.
    #[test]
    fn inequalities_are_not_mistaken_for_markup() {
        let mut paper = paper("pubmed", "10.1/y");
        let stats = "Mortality fell (p<0.05, n>300) across both arms.";
        paper.abstract_text = Some(stats.into());
        paper.title = "Outcome at p<0.05".into();
        normalize_paper_text(&mut paper);
        assert_eq!(paper.abstract_text.as_deref(), Some(stats));
        assert_eq!(paper.title, "Outcome at p<0.05");
    }

    #[test]
    fn sources_sharing_an_upstream_limit_share_one_gate() {
        let ncbi: Vec<_> = registry::drivers()
            .iter()
            .filter(|driver| driver.gate.map(|(group, _)| group) == Some("ncbi_eutils"))
            .map(|driver| driver.id)
            .collect();
        assert!(
            ncbi.contains(&"pubmed") && ncbi.contains(&"pmc") && ncbi.len() >= 3,
            "expected the NCBI endpoints to share a gate, got {ncbi:?}"
        );

        // OpenAlex and its Vietnam view are the same API behind one limit.
        let openalex: Vec<_> = registry::drivers()
            .iter()
            .filter(|driver| driver.gate.map(|(group, _)| group) == Some("openalex"))
            .map(|driver| driver.id)
            .collect();
        assert_eq!(openalex, vec!["openalex", "vietnam"]);

        // arXiv and Semantic Scholar are gated by `polite_get`, which also
        // retries; a second gate here would delay them twice.
        for id in ["arxiv", "semantic_scholar"] {
            let driver = registry::drivers()
                .iter()
                .find(|driver| driver.id == id)
                .unwrap();
            assert!(driver.gate.is_none(), "{id} would be double-gated");
        }
    }

    #[tokio::test]
    async fn configured_http_timeout_is_enforced() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/",
            axum::routing::get(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                "delayed"
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let result = AcademicEngine::with_timeout(1).client.get(url).send().await;
        assert!(result.unwrap_err().is_timeout());
        server.abort();
    }

    #[test]
    fn arxiv_error_documents_are_not_successful_searches() {
        assert!(parse_arxiv_feed("<html>Unavailable</html>").is_err());
        assert!(parse_arxiv_feed(
            "<feed><entry><id>http://arxiv.org/api/errors#incorrect_id_format</id></entry></feed>"
        )
        .is_err());
        assert!(parse_arxiv_feed("<feed></feed>").unwrap().is_empty());
        let papers = parse_arxiv_feed("<feed><entry><id>http://arxiv.org/abs/2401.00001v1</id><title>Learning systems</title><summary>Abstract</summary><published>2024-01-01</published><author><name>A Researcher</name></author></entry></feed>").unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].year, Some(2024));
        assert_eq!(papers[0].authors, vec!["A Researcher"]);
        // The feed reports ids over http; readers must get the https link.
        assert_eq!(
            papers[0].source_url.as_deref(),
            Some("https://arxiv.org/abs/2401.00001v1")
        );
        assert_eq!(
            papers[0].pdf_url.as_deref(),
            Some("https://arxiv.org/pdf/2401.00001v1.pdf")
        );
    }

    #[tokio::test]
    #[ignore = "Requires live arXiv access"]
    async fn live_arxiv_https() {
        let papers = AcademicEngine::new()
            .fetch_arxiv("transformer", 1)
            .await
            .unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].id.contains("arxiv.org/abs/"));
        assert!(papers[0].abstract_text.is_some());
    }

    #[tokio::test]
    async fn unconfigured_searxng_never_falls_back_to_biomedical_search() {
        let request: SearchRequest = serde_json::from_value(
            serde_json::json!({"query":"large language models","sources":["searxng"],"limit":3}),
        )
        .unwrap();
        let result = AcademicEngine::new()
            .search_candidates(&request, &SourceCredentials::default())
            .await;
        assert!(result.papers.is_empty());
        let queried: Vec<_> = result
            .sources
            .iter()
            .filter(|source| source.queried)
            .collect();
        assert_eq!(queried.len(), 1);
        assert_eq!(queried[0].id, "searxng");
        assert!(!queried[0].ok);
        let preset = resolve_sources(Some(&["cs_ai".into()]), "large language models").unwrap();
        assert!(preset.contains(&"searxng".into()));
        assert!(!preset.contains(&"metasearch".into()));
    }

    #[tokio::test]
    #[ignore = "Requires live academic search providers"]
    async fn live_multidisciplinary_search() {
        let engine = AcademicEngine::new();
        for (domain, query) in [
            ("cs_ai", "large language models"),
            ("economics", "monetary policy"),
            ("education", "formative assessment"),
            ("medical", "cancer immunotherapy"),
        ] {
            let request: SearchRequest = serde_json::from_value(serde_json::json!({"query":query,"sources":[domain],"limit":3,"year_min":2020,"year_max":2025})).unwrap();
            let result = engine
                .search_candidates(&request, &SourceCredentials::default())
                .await
                .into_page(0, 3);
            eprintln!(
                "{domain}: {} papers; {}",
                result.papers.len(),
                result
                    .sources
                    .iter()
                    .filter(|source| source.queried)
                    .map(|source| format!(
                        "{}:{} ({})",
                        source.id,
                        if source.ok { "ok" } else { "failed" },
                        source.count
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            assert!(!result.papers.is_empty(), "No results for {domain}");
            assert!(result.papers.len() <= 3);
            assert!(result
                .papers
                .iter()
                .all(|paper| paper.year.is_some_and(|year| (2020..=2025).contains(&year))));
            if domain != "medical" {
                assert!(!result.sources.iter().any(|source| source.queried
                    && ["pubmed", "metasearch"].contains(&source.id.as_str())));
            }
        }
    }

    #[test]
    fn year_filters_exclude_unknown_and_out_of_range_dates() {
        assert!(matches_year_range(None, None, None));
        assert!(!matches_year_range(None, Some(2020), None));
        assert!(!matches_year_range(Some(2019), Some(2020), None));
        assert!(!matches_year_range(Some(2025), None, Some(2024)));
        assert!(matches_year_range(Some(2024), Some(2024), Some(2024)));
    }

    #[test]
    fn discipline_routing_preserves_explicit_sources() {
        let selected = vec!["vjol".into(), "openalex".into(), "crossref".into()];
        assert_eq!(resolve_sources(Some(&selected), ""), Some(selected));
        assert_eq!(resolve_sources(Some(&[]), ""), Some(vec![]));
        assert_eq!(resolve_sources(None, ""), None);
        for domain in [
            "ai_cs",
            "cs_ai",
            "stem_nature",
            "engineering",
            "natural_sciences",
            "social_humanities",
            "economics",
            "law",
            "education",
            "humanities",
            "environment",
            "agriculture",
            "social_sciences",
        ] {
            let resolved = resolve_sources(Some(&[domain.into()]), "").unwrap();
            assert!(resolved.contains(&"openalex".to_string()));
            assert!(!resolved.contains(&"pubmed".to_string()));
            assert!(!resolved.contains(&"metasearch".to_string()));
        }
    }

    #[test]
    fn automatic_routing_does_not_confuse_substrings_with_ai() {
        assert_eq!(
            detect_sources_for_query("retail supply chains"),
            vec!["openalex", "arxiv", "crossref", "semantic_scholar", "hal"]
        );
        assert_eq!(
            detect_sources_for_query("AI agents"),
            vec!["arxiv", "openalex", "crossref", "semantic_scholar"]
        );
        assert!(detect_sources_for_query("cancer therapy").contains(&"pubmed"));
    }

    fn paper(source: &str, doi: &str) -> Paper {
        Paper {
            id: format!("{}:{}", source, doi),
            title: "Screening for gestational diabetes".to_string(),
            authors: Vec::new(),
            year: None,
            venue: None,
            abstract_text: None,
            doi: Some(doi.to_string()),
            source_url: None,
            pdf_url: None,
            citations: None,
            quartile: None,
            source: source.to_string(),
            score: None,
            open_access: false,
            biblio: None,
        }
    }

    #[test]
    fn merge_fills_missing_bibliographic_fields() {
        let mut winner = paper("OpenAlex", "10.1/abc");
        winner.biblio = Some(crate::models::Biblio {
            volume: Some("12".into()),
            ..Default::default()
        });
        let mut other = paper("Crossref", "10.1/abc");
        other.biblio = Some(crate::models::Biblio {
            volume: Some("99".into()),
            pages: Some("1-9".into()),
            ..Default::default()
        });
        merge_paper_metadata(&mut winner, other);
        let biblio = winner.biblio.unwrap();
        assert_eq!(biblio.volume.as_deref(), Some("12"));
        assert_eq!(biblio.pages.as_deref(), Some("1-9"));
    }

    #[test]
    fn openalex_and_crossref_expose_volume_issue_and_pages() {
        let openalex = openalex_paper(
            &serde_json::json!({
                "id": "https://openalex.org/W1", "title": "T",
                "biblio": {"volume": "5", "issue": "2", "first_page": "10", "last_page": "20"},
                "primary_location": {"source": {"issn_l": "1234-5678", "host_organization_name": "Elsevier"}}
            }),
            "",
            "OpenAlex",
        );
        let biblio = openalex.biblio.unwrap();
        assert_eq!(biblio.pages.as_deref(), Some("10-20"));
        assert_eq!(biblio.issn.as_deref(), Some("1234-5678"));
        assert_eq!(biblio.publisher.as_deref(), Some("Elsevier"));

        let crossref = crossref_biblio(&serde_json::json!({
            "volume": "7", "issue": "1", "page": "e123", "ISSN": ["1111-2222"], "publisher": "PLOS"
        }))
        .unwrap();
        assert_eq!(crossref.volume.as_deref(), Some("7"));
        assert_eq!(crossref.pages.as_deref(), Some("e123"));
        assert!(crossref_biblio(&serde_json::json!({})).is_none());
    }

    #[test]
    fn unpaywall_accepts_only_explicit_pdf_urls() {
        let direct = serde_json::json!({
            "best_oa_location": {
                "url_for_pdf": "https://example.org/article.pdf",
                "url": "https://example.org/article"
            }
        });
        let landing_only = serde_json::json!({
            "best_oa_location": {
                "url_for_pdf": null,
                "url": "https://doi.org/10.1234/example"
            }
        });

        assert_eq!(
            unpaywall_pdf_url(&direct).as_deref(),
            Some("https://example.org/article.pdf")
        );
        assert_eq!(unpaywall_pdf_url(&landing_only), None);

        let repository_copy = serde_json::json!({
            "best_oa_location": { "url_for_pdf": null, "url": "https://doi.org/10.1234/example" },
            "oa_locations": [
                { "url_for_pdf": null },
                { "url_for_pdf": "https://europepmc.org/articles/PMC1/pdf" }
            ]
        });
        assert_eq!(
            unpaywall_pdf_url(&repository_copy).as_deref(),
            Some("https://europepmc.org/articles/PMC1/pdf")
        );
    }

    #[test]
    fn fusion_breaks_ties_deterministically_across_runs_and_source_order() {
        let engine = AcademicEngine::new();
        let lists: Vec<Vec<Paper>> = (0..24)
            .map(|i| vec![paper("fixture", &format!("10.1/{i:02}"))])
            .collect();
        let expected: Vec<String> = (0..24).map(|i| format!("fixture:10.1/{i:02}")).collect();
        for turn in 0..32 {
            let mut reordered = lists.clone();
            reordered.rotate_left(turn % lists.len());
            let result = engine.reciprocal_rank_fusion(reordered, 60.0, usize::MAX);
            assert_eq!(
                result.into_iter().map(|paper| paper.id).collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn merge_fills_missing_fields_from_a_duplicate() {
        // A sparse PubMed record, as esummary returns it.
        let mut pubmed = paper("PubMed", "10.1/abc");

        // The same paper from OpenAlex, carrying the verified open-access URL.
        let mut openalex = paper("OpenAlex", "10.1/abc");
        openalex.pdf_url = Some("https://example.org/free.pdf".to_string());
        openalex.open_access = true;
        openalex.abstract_text = Some("Real abstract".to_string());
        openalex.citations = Some(42);
        openalex.authors = vec!["Nguyen A".to_string()];
        openalex.year = Some(2024);

        merge_paper_metadata(&mut pubmed, openalex);

        assert_eq!(
            pubmed.pdf_url.as_deref(),
            Some("https://example.org/free.pdf")
        );
        assert!(
            pubmed.open_access,
            "free full text from any source makes it open access"
        );
        assert_eq!(pubmed.abstract_text.as_deref(), Some("Real abstract"));
        assert_eq!(pubmed.citations, Some(42));
        assert_eq!(pubmed.authors, vec!["Nguyen A".to_string()]);
        assert_eq!(pubmed.year, Some(2024));
        assert_eq!(
            pubmed.source, "PubMed",
            "the winning record keeps its own identity"
        );
    }

    #[test]
    fn merge_never_overwrites_data_the_winner_already_has() {
        let mut winner = paper("OpenAlex", "10.1/abc");
        winner.pdf_url = Some("https://example.org/first.pdf".to_string());
        winner.citations = Some(10);

        let mut other = paper("Crossref", "10.1/abc");
        other.pdf_url = Some("https://example.org/second.pdf".to_string());
        other.citations = Some(99);

        merge_paper_metadata(&mut winner, other);

        assert_eq!(
            winner.pdf_url.as_deref(),
            Some("https://example.org/first.pdf")
        );
        assert_eq!(winner.citations, Some(10));
    }

    #[test]
    fn fusion_merges_duplicates_instead_of_keeping_only_the_first() {
        let engine = AcademicEngine::new();

        let sparse = paper("PubMed", "10.1/abc");
        let mut rich = paper("OpenAlex", "10.1/abc");
        rich.pdf_url = Some("https://example.org/free.pdf".to_string());
        rich.open_access = true;

        // PubMed is ranked first, so before the fix its empty pdf_url won.
        let fused = engine.reciprocal_rank_fusion(vec![vec![sparse], vec![rich]], 60.0, usize::MAX);

        assert_eq!(fused.len(), 1, "same DOI must collapse into one result");
        assert_eq!(
            fused[0].pdf_url.as_deref(),
            Some("https://example.org/free.pdf")
        );
        assert!(fused[0].open_access);
    }

    #[test]
    fn fusion_keys_distinct_papers_separately() {
        let engine = AcademicEngine::new();
        let a = paper("PubMed", "10.1/aaa");
        let b = paper("PubMed", "10.1/bbb");
        let fused = engine.reciprocal_rank_fusion(vec![vec![a, b]], 60.0, usize::MAX);
        assert_eq!(fused.len(), 2);
    }

    #[tokio::test]
    #[ignore = "Diagnostic: probes every available catalog source live. Narrow it with PROBE_SOURCES=id1,id2"]
    async fn live_probe_all_sources() {
        let engine = AcademicEngine::new();
        let catalog = crate::catalog::catalog();
        let mut failed: Vec<String> = Vec::new();
        let mut empty: Vec<String> = Vec::new();
        let mut ok: Vec<String> = Vec::new();
        let only: Option<Vec<String>> = std::env::var("PROBE_SOURCES").ok().map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        });
        let probe_query = std::env::var("PROBE_QUERY").ok();
        for source in &catalog.sources {
            if let Some(list) = &only {
                if !list.contains(&source.id) {
                    continue;
                }
            } else if !source.available {
                continue;
            }
            let query = if let Some(query) = probe_query.as_deref() {
                query
            } else if source.group.contains("Biomedical") {
                "diabetes"
            } else {
                "machine learning"
            };
            let request: SearchRequest = serde_json::from_value(
                serde_json::json!({"query":query,"sources":[source.id.clone()],"limit":3}),
            )
            .unwrap();
            let result = engine
                .search_candidates(&request, &SourceCredentials::default())
                .await;
            let status = result.sources.iter().find(|s| s.id == source.id);
            match status {
                None => failed.push(format!("{} -> NOT DISPATCHED (no status row)", source.id)),
                Some(s) if !s.queried => failed.push(format!("{} -> NOT QUERIED", source.id)),
                Some(s) if !s.ok => failed.push(format!(
                    "{} -> ERROR: {}",
                    source.id,
                    s.error.clone().unwrap_or_default()
                )),
                Some(s) if s.count == 0 => empty.push(format!("{} -> 0 results", source.id)),
                Some(s) => ok.push(format!("{}={}", source.id, s.count)),
            }
        }
        eprintln!("\n===== OK ({}) =====\n{}", ok.len(), ok.join(", "));
        eprintln!(
            "\n===== EMPTY ({}) =====\n{}",
            empty.len(),
            empty.join("\n")
        );
        eprintln!(
            "\n===== FAILED ({}) =====\n{}",
            failed.len(),
            failed.join("\n")
        );
    }

    // Sources that were never given an API key are not broken sources. Reporting
    // them as failures made a normal search look like half the catalog was down.
    #[tokio::test]
    async fn unconfigured_sources_report_needs_setup_not_failure() {
        let request: SearchRequest = serde_json::from_value(serde_json::json!({
            "query": "large language models",
            "sources": ["scopus", "ieee", "searxng"],
            "limit": 3
        }))
        .unwrap();
        let result = AcademicEngine::new()
            .search_candidates(&request, &SourceCredentials::default())
            .await;
        let queried: Vec<_> = result
            .sources
            .iter()
            .filter(|source| source.queried)
            .collect();
        assert_eq!(queried.len(), 3);
        for source in queried {
            assert!(!source.ok, "{} should not report success", source.id);
            assert!(
                source.needs_setup,
                "{} should be flagged as needing setup",
                source.id
            );
            let error = source.error.as_deref().unwrap_or_default();
            assert!(
                !error.starts_with(sources::NEEDS_SETUP),
                "marker must be stripped: {error}"
            );
            assert!(!error.is_empty());
        }
    }

    #[tokio::test]
    async fn real_source_failures_are_not_flagged_as_needing_setup() {
        let statuses = AcademicEngine::new()
            .search_candidates(
                &serde_json::from_value(serde_json::json!({"query":"x","sources":[],"limit":1}))
                    .unwrap(),
                &SourceCredentials::default(),
            )
            .await
            .sources;
        assert!(statuses.iter().all(|source| !source.needs_setup));
    }

    // The design rule is one shared registry: every source the engine can pick on
    // its own must still exist in the catalog and still be available. Without this
    // the hand-written routing lists rot silently when a source is retired.
    #[test]
    fn engine_routing_only_emits_available_catalog_sources() {
        let catalog = crate::catalog::catalog();
        let available: std::collections::HashSet<&str> = catalog
            .sources
            .iter()
            .filter(|source| source.available)
            .map(|source| source.id.as_str())
            .collect();

        let queries = [
            "cancer immunotherapy",
            "machine learning",
            "nghiên cứu Việt Nam",
            "monetary policy",
            "formative assessment",
            "",
            "ung thư và AI",
        ];
        for query in queries {
            for id in detect_sources_for_query(query) {
                assert!(available.contains(id), "auto-routing emits '{id}' for '{query}', which is not an available catalog source");
            }
        }

        for preset in &catalog.presets {
            let resolved = resolve_sources(Some(&[preset.id.clone()]), "").unwrap_or_default();
            for id in resolved {
                assert!(
                    available.contains(id.as_str()),
                    "preset '{}' resolves to unavailable source '{id}'",
                    preset.id
                );
            }
        }
    }

    // A source that is blocking us or has retired its endpoint fails on every
    // search. Re-testing it each time costs the user a full timeout and repeats the
    // same error, so it is skipped for a cooldown once it has failed twice.
    #[tokio::test]
    async fn repeated_failures_put_a_source_into_cooldown() {
        let id = "cooldown_probe_source";
        {
            let mut guard = SOURCE_HEALTH.lock().await;
            guard.get_or_insert_with(HashMap::new).remove(id);
        }
        let failure = |error: &str| SourceStatus {
            id: id.to_string(),
            name: "Probe".into(),
            queried: true,
            ok: false,
            count: 0,
            error: Some(error.to_string()),
            needs_setup: false,
            cooling_down: false,
        };

        // Sources prefix their own errors, e.g. "sljol: HTTP 403 Forbidden".
        let raw = format!("{id}: HTTP 403 Forbidden");

        // One failure is not enough to stop querying a source.
        record_source_health(&[failure(&raw)]).await;
        assert!(cooling_down(&[id]).await.is_empty());

        record_source_health(&[failure(&raw)]).await;
        let cooling = cooling_down(&[id]).await;
        let reason = cooling.get(id).expect("source should be cooling down");
        assert!(reason.contains("paused after 2 failures"), "{reason}");
        assert!(
            reason.contains("HTTP 403 Forbidden"),
            "the reason must name the last error: {reason}"
        );
        assert!(
            !reason.contains(&format!("{id}:")),
            "the source id must not be repeated inside the reason: {reason}"
        );

        // A later success clears the breaker immediately.
        record_source_health(&[SourceStatus {
            id: id.to_string(),
            name: "Probe".into(),
            queried: true,
            ok: true,
            count: 3,
            error: None,
            needs_setup: false,
            cooling_down: false,
        }])
        .await;
        assert!(cooling_down(&[id]).await.is_empty());
    }

    #[tokio::test]
    async fn cooldown_ignores_sources_that_only_need_setup_or_were_skipped() {
        let missing_key = "cooldown_needs_setup_source";
        let skipped = "cooldown_skipped_source";
        {
            let mut guard = SOURCE_HEALTH.lock().await;
            let health = guard.get_or_insert_with(HashMap::new);
            health.remove(missing_key);
            health.remove(skipped);
        }
        let statuses = vec![
            SourceStatus {
                id: missing_key.into(),
                name: "Keyed".into(),
                queried: true,
                ok: false,
                count: 0,
                error: Some("requires KEY".into()),
                needs_setup: true,
                cooling_down: false,
            },
            SourceStatus {
                id: skipped.into(),
                name: "Skipped".into(),
                queried: true,
                ok: false,
                count: 0,
                error: Some("paused".into()),
                needs_setup: false,
                cooling_down: true,
            },
        ];
        record_source_health(&statuses).await;
        record_source_health(&statuses).await;
        assert!(cooling_down(&[missing_key, skipped]).await.is_empty());
    }

    // arXiv is a primary source for CS/AI and it rate-limits in bursts. A flat long
    // cooldown meant one transient 429 cost the user arXiv for the next five
    // minutes, so the first trip has to be short and only a persistent run of
    // failures may back off hard.
    #[test]
    fn cooldown_starts_short_and_escalates_only_when_failures_persist() {
        assert_eq!(cooldown_for(2), Duration::from_secs(60));
        assert_eq!(cooldown_for(3), Duration::from_secs(300));
        assert_eq!(cooldown_for(5), Duration::from_secs(900));
        // Never shorter as failures pile up.
        let mut previous = Duration::ZERO;
        for failures in 2..10 {
            let current = cooldown_for(failures);
            assert!(
                current >= previous,
                "cooldown shrank at {failures} failures"
            );
            previous = current;
        }
    }

    // `papers_with_code` and `huggingface` queried the same Hugging Face feed, so
    // every paper in it entered the fusion twice and outranked real index hits —
    // arXiv and OpenAlex results were pushed off the first page entirely.
    #[test]
    fn no_two_available_sources_share_one_upstream_feed() {
        let catalog = crate::catalog::catalog();
        let live: Vec<&str> = catalog
            .sources
            .iter()
            .filter(|source| source.available)
            .map(|source| source.id.as_str())
            .collect();
        assert!(live.contains(&"huggingface"));
        assert!(
            !live.contains(&"papers_with_code"),
            "papers_with_code duplicates the huggingface feed and must not be fused twice"
        );
        for preset in &catalog.presets {
            assert!(
                !preset.sources.iter().any(|id| id == "papers_with_code"),
                "preset {} still selects the duplicated feed",
                preset.id
            );
        }
    }
}
