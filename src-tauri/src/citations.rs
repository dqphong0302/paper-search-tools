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
    crate::details::normalize_doi(id).map(|doi| format!("https://api.openalex.org/works/doi:{doi}"))
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
    while let Some(chunk) = response.chunk().await.map_err(|error| (502, error.to_string()))? {
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
    let path = work_path(id)
        .ok_or((404, "A DOI, PMID or OpenAlex id is required to look up citations".to_string()))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("ScholarGateway-Desktop/1.0 (mailto:dqphong0302@gmail.com)")
        .build()
        .map_err(|error| (500, error.to_string()))?;
    let limit = limit.clamp(1, 50);

    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(email) = state.db.get_config("openalex_email").filter(|value| !value.trim().is_empty()) {
        query.push(("mailto", email));
    }
    if let Some(key) = state.db.get_config("openalex_api_key").filter(|value| !value.trim().is_empty()) {
        query.push(("api_key", key));
    }

    let work = get_json(&client, &format!("{path}{}", build_query(&query))).await?;
    let Some(work_id) = work.get("id").and_then(Value::as_str).and_then(short_id).map(str::to_string) else {
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
        let json = get_json(&client, &format!("https://api.openalex.org/works{}", build_query(&params))).await?;
        parse_items(&json, limit)
    } else {
        let field = if direction == "related" { "related_works" } else { "referenced_works" };
        let ids: Vec<String> = work
            .get(field)
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).filter_map(short_id).map(str::to_string).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            Vec::new()
        } else {
            let mut params = query.clone();
            let joined = ids.into_iter().take(limit).collect::<Vec<_>>().join("|");
            params.push(("filter", format!("openalex_id:{joined}")));
            params.push(("per-page", limit.to_string()));
            let json = get_json(&client, &format!("https://api.openalex.org/works{}", build_query(&params))).await?;
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
mod tests {
    use super::*;

    #[test]
    fn resolves_known_identifier_forms_only() {
        assert_eq!(work_path("10.1038/NATURE14539").as_deref(), Some("https://api.openalex.org/works/doi:10.1038/nature14539"));
        assert_eq!(work_path("pmid:26017442").as_deref(), Some("https://api.openalex.org/works/pmid:26017442"));
        assert_eq!(work_path("W2154910403").as_deref(), Some("https://api.openalex.org/works/W2154910403"));
        assert_eq!(work_path("vn:https://openalex.org/W123").as_deref(), Some("https://openalex.org/W123"));
        assert!(work_path("unknown-id").is_none());
    }

    #[test]
    fn extracts_short_work_ids() {
        assert_eq!(short_id("https://openalex.org/W123"), Some("W123"));
        assert_eq!(short_id("https://openalex.org/W123/"), Some("W123"));
        assert_eq!(short_id("https://openalex.org/authors/A123"), None);
    }
}
