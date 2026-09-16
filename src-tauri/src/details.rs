use crate::{models::Paper, server::AppState};
use serde_json::Value;

pub fn normalize_doi(id: &str) -> Option<String> {
    let id = id.trim();
    let lower = id.to_lowercase();
    let doi = [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "doi:",
    ]
    .iter()
    .find_map(|prefix| lower.strip_prefix(prefix))
    .unwrap_or(&lower)
    .trim();
    let (registrant, suffix) = doi.strip_prefix("10.")?.split_once('/')?;
    if !(4..=9).contains(&registrant.len())
        || !registrant.bytes().all(|c| c.is_ascii_digit())
        || suffix.is_empty()
        || doi.len() > 512
        || doi.chars().any(char::is_whitespace)
        || doi.chars().any(char::is_control)
    {
        return None;
    }
    Some(doi.to_string())
}

pub fn matches(paper: &Paper, id: &str) -> bool {
    paper.id == id
        || normalize_doi(id).is_some_and(|doi| {
            paper.doi.as_deref().and_then(normalize_doi).as_ref() == Some(&doi)
                || normalize_doi(&paper.id).as_ref() == Some(&doi)
        })
}

fn crossref_paper(item: &Value) -> Result<Paper, String> {
    let doi = item["DOI"]
        .as_str()
        .and_then(normalize_doi)
        .ok_or("Crossref record has no valid DOI")?;
    let title = item["title"]
        .as_array()
        .and_then(|titles| titles.first())
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .ok_or("Crossref record has no title")?;
    let year = ["published", "published-print", "published-online", "issued"]
        .iter()
        .find_map(|field| item[field]["date-parts"][0][0].as_u64())
        .and_then(|year| u32::try_from(year).ok());
    let authors = item["author"]
        .as_array()
        .map(|authors| {
            authors
                .iter()
                .filter_map(|author| {
                    let name = author["name"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            format!(
                                "{} {}",
                                author["given"].as_str().unwrap_or(""),
                                author["family"].as_str().unwrap_or("")
                            )
                        });
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        None
                    } else {
                        Some(name)
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Paper {
        id: doi.clone(),
        title: title.to_string(),
        authors,
        year,
        venue: item["container-title"][0].as_str().map(str::to_string),
        abstract_text: item["abstract"].as_str().map(str::to_string),
        source_url: Some(format!("https://doi.org/{doi}")),
        doi: Some(doi),
        // Crossref fulltext links alone do not prove open access.
        pdf_url: None,
        open_access: false,
        citations: item["is-referenced-by-count"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok()),
        quartile: None,
        score: None,
        source: "Crossref".into(),
    })
}

pub async fn lookup(state: &AppState, id: &str) -> Result<Paper, (u16, String)> {
    if id.trim().is_empty() || id.len() > 512 {
        return Err((400, "Id must be 1-512 characters".into()));
    }
    if let Some(paper) = state.db.find_paper(id) {
        return Ok(paper);
    }
    let doi = normalize_doi(id).ok_or((
        404,
        "Id not found in the library or cache. For a paper you have not searched yet, pass a valid DOI so Crossref can be queried."
            .into(),
    ))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("ScholarGateway/1.0")
        .build()
        .map_err(|e| (500, e.to_string()))?;
    let mut request = client.get(format!(
        "https://api.crossref.org/works/{}",
        urlencoding::encode(&doi)
    ));
    if let Some(email) = state
        .db
        .get_config("crossref_email")
        .filter(|email| !email.trim().is_empty())
    {
        request = request.query(&[("mailto", email)]);
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| (502, e.without_url().to_string()))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err((404, "DOI not found on Crossref".into()));
    }
    if !response.status().is_success() {
        return Err((502, format!("Crossref HTTP {}", response.status())));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| (502, e.without_url().to_string()))?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err((502, "Metadata exceeds the 2 MiB limit".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| (502, "Crossref did not return valid JSON".into()))?;
    let paper = crossref_paper(&value["message"]).map_err(|error| (502, error))?;
    if paper.doi.as_deref() != Some(doi.as_str()) {
        return Err((502, "Crossref returned a DOI that does not match the request".into()));
    }
    Ok(paper)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    #[ignore = "Requires live Crossref network access"]
    async fn live_crossref_exact_doi() {
        let state = AppState { db: crate::db::Database::in_memory().unwrap(), engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()), port: 0, mcp_sessions: Default::default() };
        let paper = lookup(&state, "10.1038/nature14539").await.unwrap();
        assert_eq!(paper.doi.as_deref(), Some("10.1038/nature14539"));
        assert_eq!(paper.title.to_lowercase(), "deep learning");
        assert_eq!(paper.year, Some(2015));
        assert!(!paper.authors.is_empty());
    }

    #[test]
    fn doi_normalization_and_metadata_preserve_evidence() {
        assert_eq!(
            normalize_doi("https://doi.org/10.1000/ABC"),
            Some("10.1000/abc".into())
        );
        for invalid in [
            "https://evil.example/10.1000/abc",
            "10.x/abc",
            "10.1000/",
            "10.1000/with space",
        ] {
            assert!(normalize_doi(invalid).is_none());
        }
        let paper = crossref_paper(&json!({"DOI":"10.1000/ABC","title":["Economic policy"],"published":{"date-parts":[[2024]]},"author":[{"given":"A","family":"Smith"}],"abstract":"Evidence", "link":[{"content-type":"application/pdf","URL":"https://publisher.example/paywall.pdf"}]})).unwrap();
        assert_eq!(paper.year, Some(2024));
        assert_eq!(paper.authors, vec!["A Smith"]);
        assert!(!paper.open_access);
        assert!(paper.pdf_url.is_none());
        assert!(matches(&paper, "doi:10.1000/abc"));
        assert!(crossref_paper(&json!({"DOI":"10.1000/x"})).is_err());
    }

    #[tokio::test]
    async fn local_details_do_not_require_network() {
        let db = crate::db::Database::in_memory().unwrap();
        let paper = crossref_paper(&json!({"DOI":"10.1000/test","title":["Education"]})).unwrap();
        db.save_paper(&paper).unwrap();
        let state = AppState {
            db,
            engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        assert_eq!(
            lookup(&state, "https://doi.org/10.1000/TEST")
                .await
                .unwrap()
                .title,
            "Education"
        );
        assert_eq!(lookup(&state, "unknown-id").await.unwrap_err().0, 404);
    }
}
