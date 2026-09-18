use crate::models::Paper;
use crate::sources::clean_html_text;
use serde_json::Value;

/// INSPIRE-HEP (High-Energy Physics - CERN/DESY/SLAC)
pub async fn search_inspire_hep(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://inspirehep.net/api/literature?q={}&size={}&sort=bestmatch&fields=titles,authors,earliest_date,dois,arxiv_eprints,abstracts,citation_count,publication_info,documents,control_number",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "inspire_hep: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(format!("inspire_hep: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("inspire_hep: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let hits = val
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|a| a.as_array());

    if let Some(list) = hits {
        for hit in list {
            if papers.len() >= limit {
                break;
            }

            let m = hit.get("metadata");
            let title = m
                .and_then(|meta| meta.get("titles"))
                .and_then(|t| t.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");

            let authors = m
                .and_then(|meta| meta.get("authors"))
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.get("full_name").and_then(|n| n.as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = m
                .and_then(|meta| meta.get("earliest_date"))
                .and_then(|d| d.as_str())
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let doi = m
                .and_then(|meta| meta.get("dois"))
                .and_then(|d| d.as_array())
                .and_then(|arr| arr.first())
                .and_then(|x| x.get("value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let arxiv_id = m
                .and_then(|meta| meta.get("arxiv_eprints"))
                .and_then(|a| a.as_array())
                .and_then(|arr| arr.first())
                .and_then(|x| x.get("value"))
                .and_then(|v| v.as_str());

            let abstract_text = m
                .and_then(|meta| meta.get("abstracts"))
                .and_then(|a| a.as_array())
                .and_then(|arr| arr.first())
                .and_then(|x| x.get("value"))
                .and_then(|v| v.as_str())
                .map(clean_html_text);

            let citations = m
                .and_then(|meta| meta.get("citation_count"))
                .and_then(|c| c.as_u64())
                .map(|u| u as u32);

            let ctrl_num = m
                .and_then(|meta| meta.get("control_number"))
                .and_then(|n| n.as_i64())
                .map(|n| n.to_string())
                .unwrap_or_else(|| "0".to_string());

            let source_url = doi
                .as_ref()
                .map(|d| format!("https://doi.org/{}", d))
                .unwrap_or_else(|| format!("https://inspirehep.net/literature/{}", ctrl_num));

            let pdf_url = arxiv_id.map(|id| format!("https://arxiv.org/pdf/{}.pdf", id));

            papers.push(Paper {
                id: format!("inspire_hep:{}", ctrl_num),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some("INSPIRE-HEP (CERN / DESY)".to_string()),
                abstract_text,
                doi,
                source_url: Some(source_url),
                pdf_url,
                citations,
                quartile: None,
                source: "inspire_hep".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}

/// DataCite Datasets & Software DOI Registry
pub async fn search_datacite(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.datacite.org/dois?query={}&page[size]={}",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "datacite: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(format!("datacite: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("datacite: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let data = val.get("data").and_then(|d| d.as_array());

    if let Some(list) = data {
        for item in list {
            if papers.len() >= limit {
                break;
            }

            let attrs = item.get("attributes");
            let doi = attrs
                .and_then(|a| a.get("doi"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let title = attrs
                .and_then(|a| a.get("titles"))
                .and_then(|t| t.as_array())
                .and_then(|arr| arr.first())
                .and_then(|x| x.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled DataCite record)");

            let authors = attrs
                .and_then(|a| a.get("creators"))
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = attrs
                .and_then(|a| a.get("publicationYear"))
                .and_then(|y| y.as_u64())
                .map(|y| y as u32);

            let publisher = attrs
                .and_then(|a| a.get("publisher"))
                .and_then(|p| p.as_str())
                .unwrap_or("DataCite");

            let source_url = format!("https://doi.org/{}", doi);

            papers.push(Paper {
                id: format!("datacite:{}", doi),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(format!("DataCite ({})", publisher)),
                abstract_text: None,
                doi: Some(doi.to_string()),
                source_url: Some(source_url),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "datacite".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}

/// EconBiz / RePEc Economics Search API
pub async fn search_econbiz(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.econbiz.de/v1/search?q={}&size={}",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "econbiz: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(format!("econbiz: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("econbiz: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let hits = val
        .get("hits")
        .and_then(|h| h.get("hits").or(Some(h)))
        .and_then(|v| v.as_array());

    if let Some(list) = hits {
        for hit in list {
            if papers.len() >= limit {
                break;
            }

            let title = hit
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("(untitled)");
            let authors = hit
                .get("creator")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = hit
                .get("date")
                .and_then(|d| d.as_str())
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let source_url = hit
                .get("source_url")
                .or_else(|| hit.get("url"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string());

            let id_str = hit.get("id").and_then(|i| i.as_str()).unwrap_or("unknown");

            papers.push(Paper {
                id: format!("econbiz:{}", id_str),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some("EconBiz (ZBW / RePEc)".to_string()),
                abstract_text: None,
                doi: None,
                source_url,
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "econbiz".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}

/// ERIC - Education Resources Information Center
pub async fn search_eric(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.ies.ed.gov/eric/?search={}&format=json&rows={}",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "eric: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(format!("eric: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("eric: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let docs = val
        .get("response")
        .and_then(|r| r.get("docs"))
        .and_then(|d| d.as_array());

    if let Some(list) = docs {
        for doc in list {
            if papers.len() >= limit {
                break;
            }

            let eric_id = doc.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let title = doc
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("(untitled)");

            let authors = doc
                .get("author")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = doc.get("publicationdateyear").and_then(|y| {
                y.as_str()
                    .and_then(|s| s.parse::<u32>().ok())
                    .or_else(|| y.as_u64().map(|n| n as u32))
            });

            let summary = doc
                .get("description")
                .and_then(|d| d.as_str())
                .map(clean_html_text);
            let source_url = format!("https://eric.ed.gov/?id={}", eric_id);

            papers.push(Paper {
                id: format!("eric:{}", eric_id),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some("ERIC (U.S. Dept of Education)".to_string()),
                abstract_text: summary,
                doi: None,
                source_url: Some(source_url),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "eric".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}
