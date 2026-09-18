//! Citation-graph lookups via OpenAlex. Given a paper ID (DOI/PMID/OpenAlex),
//! return the works it references or the works that cite it.
use crate::{engine::openalex_paper, models::Paper, server::AppState};
use serde_json::{json, Value};
use std::time::Duration;

const MAX_BYTES: usize = 4 * 1024 * 1024;

/// Resolve an app paper ID to an OpenAlex works path.
pub fn work_path(id: &str) -> Option<String> {
    let id = id.trim();
    if id.is_empty() || id.len() > 512 {
        return None;
    }
    if let Some(rest) = id.strip_prefix("vn:") {
        return work_path(rest);
    }
    let lower = id.to_lowercase();
    if lower.starts_with("https://openalex.org/w") || lower.starts_with("http://openalex.org/w") {
        return Some(id.to_string());
    }
    if let Some(pmid) = id.strip_prefix("pmid:") {
        if !pmid.is_empty() && pmid.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("https://api.openalex.org/works/pmid:{pmid}"));
        }
    }
    if let Some(rest) = id.strip_prefix('W') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("https://api.openalex.org/works/{id}"));
        }
    }
    if let Some(path) = from_arxiv(id) {
        return Some(path);
    }
    if let Some(path) = from_source_prefix(id) {
        return Some(path);
    }
    crate::details::normalize_doi(id).map(|doi| format!("https://api.openalex.org/works/doi:{doi}"))
}

/// arXiv ids carry no DOI of their own, but every arXiv preprint has a
/// registered DataCite DOI of the form `10.48550/arXiv.<id>`, which OpenAlex
/// indexes. The version suffix is not part of the DOI.
fn from_arxiv(id: &str) -> Option<String> {
    let rest = id
        .strip_prefix("http://arxiv.org/abs/")
        .or_else(|| id.strip_prefix("https://arxiv.org/abs/"))
        .or_else(|| id.strip_prefix("arxiv:"))
        .or_else(|| id.strip_prefix("arXiv:"))?;
    let base = rest.split('v').next().filter(|value| !value.is_empty())?;
    if !base.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '/') {
        return None;
    }
    Some(format!(
        "https://api.openalex.org/works/doi:10.48550/arXiv.{base}"
    ))
}

/// Recovers an identifier OpenAlex understands from a source-qualified app id.
///
/// The app mints ids like `epmc:MED:37117020` or `biorxiv:10.1101/xyz`, which
/// OpenAlex knows nothing about — so every result from those sources answered
/// "a DOI, PMID or OpenAlex id is required" even when the identifier was right
/// there inside the id. Europe PMC's `MED:` prefix carries a PMID and its
/// `PMC:` prefix a PMC id; every other prefix wraps something `work_path`
/// already handles.
fn from_source_prefix(id: &str) -> Option<String> {
    let (prefix, rest) = id.split_once(':')?;
    if prefix.is_empty() || rest.is_empty() || prefix.contains('/') {
        return None;
    }
    if prefix.eq_ignore_ascii_case("epmc") {
        if let Some((kind, value)) = rest.split_once(':') {
            if kind.eq_ignore_ascii_case("MED")
                && !value.is_empty()
                && value.chars().all(|c| c.is_ascii_digit())
            {
                return Some(format!("https://api.openalex.org/works/pmid:{value}"));
            }
            if kind.eq_ignore_ascii_case("PMC") {
                let pmcid = value.trim_start_matches(['P', 'M', 'C', 'p', 'm', 'c']);
                if !pmcid.is_empty() && pmcid.chars().all(|c| c.is_ascii_digit()) {
                    return Some(format!("https://api.openalex.org/works/pmcid:PMC{pmcid}"));
                }
            }
            return work_path(value);
        }
    }
    work_path(rest)
}

fn short_id(openalex_url: &str) -> Option<&str> {
    openalex_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| value.starts_with('W') && value.len() > 1)
}

fn build_query(params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let mut query = String::from("?");
    for (index, (key, value)) in params.iter().enumerate() {
        if index > 0 {
            query.push('&');
        }
        query.push_str(key);
        query.push('=');
        query.push_str(&urlencoding::encode(value));
    }
    query
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value, (u16, String)> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| (502, error.without_url().to_string()))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err((404, "Not found on OpenAlex".into()));
    }
    if !response.status().is_success() {
        return Err((502, format!("OpenAlex HTTP {}", response.status())));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| (502, error.to_string()))?
    {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err((502, "Metadata exceeds the 4 MiB limit".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| (502, "OpenAlex did not return valid JSON".into()))
}

fn parse_items(json: &Value, limit: usize) -> Vec<Paper> {
    json.get("results")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .take(limit)
                .map(|item| openalex_paper(item, "", "OpenAlex"))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn lookup(
    state: &AppState,
    id: &str,
    direction: &str,
    limit: usize,
) -> Result<Value, (u16, String)> {
    // Two things can go wrong, and the paper's own DOI fixes both: the app id
    // may carry no identifier OpenAlex understands, and OpenAlex may simply not
    // have indexed the paper under the identifier the id does carry — common
    // for very recent articles, which are findable by DOI before PMID. So try
    // the id first, then the DOI from the library or the recent-search cache.
    let mut candidates: Vec<String> = work_path(id).into_iter().collect();
    if let Some(doi_path) = state
        .db
        .find_paper(id)
        .and_then(|paper| paper.doi)
        .and_then(|doi| work_path(&doi))
    {
        if !candidates.contains(&doi_path) {
            candidates.push(doi_path);
        }
    }
    if candidates.is_empty() {
        return Err((
            404,
            "A DOI, PMID or OpenAlex id is required to look up citations".to_string(),
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("ScholarGate-Desktop/1.0 (mailto:dqphong0302@gmail.com)")
        .build()
        .map_err(|error| (500, error.to_string()))?;
    let limit = limit.clamp(1, 50);

    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(email) = state
        .db
        .get_config("openalex_email")
        .filter(|value| !value.trim().is_empty())
    {
        query.push(("mailto", email));
    }
    if let Some(key) = state
        .db
        .get_config("openalex_api_key")
        .filter(|value| !value.trim().is_empty())
    {
        query.push(("api_key", key));
    }

    let suffix = build_query(&query);
    let mut work = None;
    let mut last_error = (404, "Not found on OpenAlex".to_string());
    for path in &candidates {
        match get_json(&client, &format!("{path}{suffix}")).await {
            Ok(value) => {
                work = Some(value);
                break;
            }
            Err(error) => last_error = error,
        }
    }
    let work = work.ok_or(last_error)?;
    let Some(work_id) = work
        .get("id")
        .and_then(Value::as_str)
        .and_then(short_id)
        .map(str::to_string)
    else {
        return Err((404, "Could not resolve an OpenAlex work id".into()));
    };

    let direction = match direction.to_lowercase().as_str() {
        "cited_by" => "cited_by",
        "related" => "related",
        _ => "references",
    };

    let items = if direction == "cited_by" {
        let mut params = query.clone();
        params.push(("filter", format!("cites:{work_id}")));
        params.push(("per-page", limit.to_string()));
        params.push(("sort", "cited_by_count:desc".into()));
        let json = get_json(
            &client,
            &format!("https://api.openalex.org/works{}", build_query(&params)),
        )
        .await?;
        parse_items(&json, limit)
    } else {
        let field = if direction == "related" {
            "related_works"
        } else {
            "referenced_works"
        };
        let ids: Vec<String> = work
            .get(field)
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(short_id)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            Vec::new()
        } else {
            let mut params = query.clone();
            let joined = ids.into_iter().take(limit).collect::<Vec<_>>().join("|");
            params.push(("filter", format!("openalex_id:{joined}")));
            params.push(("per-page", limit.to_string()));
            let json = get_json(
                &client,
                &format!("https://api.openalex.org/works{}", build_query(&params)),
            )
            .await?;
            parse_items(&json, limit)
        }
    };

    Ok(json!({
        "source": "openalex",
        "direction": direction,
        "work_id": work_id,
        "items": items,
    }))
}

#[cfg(test)]
mod work_path_tests {
    use super::work_path;

    /// Every result from Europe PMC and the preprint servers used to answer
    /// "a DOI, PMID or OpenAlex id is required" — the identifier was inside the
    /// app's own id and nothing looked for it. A live run found 0 of 40 results
    /// could load their citation graph.
    #[test]
    fn source_qualified_ids_resolve_to_openalex() {
        for (id, expected) in [
            ("epmc:MED:37117020", "https://api.openalex.org/works/pmid:37117020"),
            ("epmc:PMC:PMC5299513", "https://api.openalex.org/works/pmcid:PMC5299513"),
            (
                "biorxiv:10.1101/cshperspect.a033191",
                "https://api.openalex.org/works/doi:10.1101/cshperspect.a033191",
            ),
            ("pmid:12345", "https://api.openalex.org/works/pmid:12345"),
            (
                "http://arxiv.org/abs/2209.15001v3",
                "https://api.openalex.org/works/doi:10.48550/arXiv.2209.15001",
            ),
        ] {
            assert_eq!(work_path(id).as_deref(), Some(expected), "{id}");
        }
    }

    #[test]
    fn an_id_carrying_no_usable_identifier_is_still_rejected() {
        for id in ["epmc:PPR:PPR123456", "", "stackexchange:questions"] {
            assert_eq!(work_path(id), None, "{id}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_identifier_forms_only() {
        assert_eq!(
            work_path("10.1038/NATURE14539").as_deref(),
            Some("https://api.openalex.org/works/doi:10.1038/nature14539")
        );
        assert_eq!(
            work_path("pmid:26017442").as_deref(),
            Some("https://api.openalex.org/works/pmid:26017442")
        );
        assert_eq!(
            work_path("W2154910403").as_deref(),
            Some("https://api.openalex.org/works/W2154910403")
        );
        assert_eq!(
            work_path("vn:https://openalex.org/W123").as_deref(),
            Some("https://openalex.org/W123")
        );
        assert!(work_path("unknown-id").is_none());
    }

    #[test]
    fn extracts_short_work_ids() {
        assert_eq!(short_id("https://openalex.org/W123"), Some("W123"));
        assert_eq!(short_id("https://openalex.org/W123/"), Some("W123"));
        assert_eq!(short_id("https://openalex.org/authors/A123"), None);
    }
}
