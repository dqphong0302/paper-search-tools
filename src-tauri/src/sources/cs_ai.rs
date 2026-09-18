use crate::models::Paper;
use crate::sources::clean_html_text;
use serde_json::Value;

/// DBLP Computer Science Bibliography
pub async fn search_dblp(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://dblp.org/search/publ/api?q={}&format=json&h={}",
        urlencoding::encode(query),
        limit.clamp(1, 50)
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) ScholarGate/1.0",
        )
        .send()
        .await
        .map_err(|e| format!("dblp: request failed — {}", crate::sources::transport_reason(&e)))?;

    if !res.status().is_success() {
        return Err(format!("dblp: HTTP {}", res.status()));
    }

    let text = res
        .text()
        .await
        .map_err(|e| format!("dblp: read text failed: {}", e))?;

    if text.trim_start().starts_with('<') {
        // The legacy CompleteSearch endpoint may put automated clients behind
        // a JavaScript challenge. dblp's official SPARQL endpoint exposes the
        // same current knowledge graph without requiring a browser challenge.
        return search_dblp_sparql(client, query, limit).await;
    }

    let val: Value =
        serde_json::from_str(&text).map_err(|e| format!("dblp: json parse error: {}", e))?;

    let mut papers = Vec::new();
    let hits = val
        .get("result")
        .and_then(|r| r.get("hits"))
        .and_then(|h| h.get("hit"))
        .and_then(|a| a.as_array());

    if let Some(list) = hits {
        for hit in list {
            if papers.len() >= limit {
                break;
            }
            let info = hit.get("info");
            let title = info
                .and_then(|i| i.get("title"))
                .and_then(|t| t.as_str())
                .unwrap_or("(untitled)");

            let venue = info
                .and_then(|i| i.get("venue"))
                .and_then(|v| v.as_str())
                .unwrap_or("DBLP");

            let year = info
                .and_then(|i| i.get("year"))
                .and_then(|y| y.as_str())
                .and_then(|s| s.parse::<u32>().ok());

            let doi = info
                .and_then(|i| i.get("doi"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());

            let url_str = info
                .and_then(|i| i.get("url"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
                .or_else(|| doi.as_ref().map(|d| format!("https://doi.org/{}", d)));

            let mut authors = Vec::new();
            if let Some(author_field) = info
                .and_then(|i| i.get("authors"))
                .and_then(|a| a.get("author"))
            {
                if let Some(arr) = author_field.as_array() {
                    for a in arr {
                        let name = if let Some(s) = a.as_str() {
                            s
                        } else {
                            a.get("text").and_then(|t| t.as_str()).unwrap_or("")
                        };
                        if !name.is_empty() {
                            authors.push(name.to_string());
                        }
                    }
                } else if let Some(s) = author_field.as_str() {
                    authors.push(s.to_string());
                } else if let Some(t) = author_field.get("text").and_then(|t| t.as_str()) {
                    authors.push(t.to_string());
                }
            }

            let dblp_id = hit
                .get("@id")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| doi.as_deref().unwrap_or("unknown"));

            papers.push(Paper {
                id: format!("dblp:{}", dblp_id),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(venue.to_string()),
                abstract_text: None,
                doi,
                source_url: url_str,
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "dblp".to_string(),
                score: None,
                open_access: false,
            });
        }
    }

    Ok(papers)
}

async fn search_dblp_sparql(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let needle = query
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ");
    let sparql = format!(
        r#"PREFIX dblp: <https://dblp.org/rdf/schema#>
SELECT ?publication ?title ?year ?venue ?doi WHERE {{
  ?publication dblp:title ?title .
  FILTER(CONTAINS(LCASE(STR(?title)), LCASE("{needle}")))
  OPTIONAL {{ ?publication dblp:yearOfPublication ?year }}
  OPTIONAL {{ ?publication dblp:publishedIn ?venue }}
  OPTIONAL {{ ?publication dblp:doi ?doi }}
}} LIMIT {}"#,
        limit.clamp(1, 50)
    );
    let value: Value = client
        .post("https://sparql.dblp.org/sparql")
        .header("Accept", "application/sparql-results+json")
        .header("Content-Type", "application/sparql-query")
        .body(sparql)
        .send()
        .await
        .map_err(|error| {
            format!(
                "dblp: SPARQL fallback failed — {}",
                crate::sources::transport_reason(&error)
            )
        })?
        .error_for_status()
        .map_err(|error| format!("dblp: SPARQL fallback HTTP error — {error}"))?
        .json()
        .await
        .map_err(|error| format!("dblp: SPARQL fallback returned invalid JSON — {error}"))?;

    let bindings = value
        .pointer("/results/bindings")
        .and_then(Value::as_array)
        .ok_or("dblp: SPARQL fallback response is missing bindings")?;
    Ok(bindings
        .iter()
        .filter_map(|binding| {
            let field = |name: &str| {
                binding
                    .get(name)
                    .and_then(|value| value.get("value"))
                    .and_then(Value::as_str)
            };
            let source_url = field("publication")?.to_string();
            let title = field("title")?;
            let doi = field("doi").map(|value| {
                value
                    .trim_start_matches("https://doi.org/")
                    .trim_start_matches("http://doi.org/")
                    .to_string()
            });
            Some(Paper {
                id: format!(
                    "dblp:{}",
                    source_url.trim_start_matches("https://dblp.org/rec/")
                ),
                title: clean_html_text(title),
                authors: Vec::new(),
                year: field("year").and_then(|value| value.parse().ok()),
                venue: field("venue").map(str::to_string),
                abstract_text: None,
                doi,
                source_url: Some(source_url),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "dblp".to_string(),
                score: None,
                open_access: false,
            })
        })
        .collect())
}

/// Hugging Face Daily Papers / Papers With Code
pub async fn search_huggingface(
    client: &reqwest::Client,
    source_id: &str, // "huggingface" or "papers_with_code"
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = "https://huggingface.co/api/daily_papers?limit=50";

    let res = client
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGate-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "{}: request failed — {}",
                source_id,
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(format!("{}: HTTP {}", source_id, res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("{}: json parse failed: {}", source_id, e))?;

    let q_terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let mut papers = Vec::new();
    if let Some(list) = val.as_array() {
        for item in list {
            if papers.len() >= limit {
                break;
            }

            let paper_obj = item.get("paper").unwrap_or(item);
            let title = paper_obj
                .get("title")
                .or_else(|| item.get("title"))
                .and_then(|t| t.as_str())
                .unwrap_or("(untitled)");

            let summary = paper_obj
                .get("summary")
                .and_then(|s| s.as_str())
                .unwrap_or("");

            let title_lower = title.to_lowercase();
            let summary_lower = summary.to_lowercase();

            // This endpoint is a recency feed, not a search index, so the filtering
            // happens here. Matching on *any* term let a single common word like
            // "model" pull in nearly the whole feed, and those loosely related
            // papers then outranked real hits from the search indexes.
            let matched = if q_terms.is_empty() {
                true
            } else {
                q_terms
                    .iter()
                    .all(|t| title_lower.contains(t) || summary_lower.contains(t))
            };

            if !matched {
                continue;
            }

            let arxiv_id = paper_obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let published_at = paper_obj.get("publishedAt").and_then(|p| p.as_str());

            let year = published_at
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let authors = paper_obj
                .get("authors")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| {
                            if let Some(s) = x.as_str() {
                                Some(s.to_string())
                            } else {
                                x.get("name")
                                    .and_then(|n| n.as_str())
                                    .map(|s| s.to_string())
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let citations = item
                .get("upvotes")
                .and_then(|v| v.as_u64())
                .map(|u| u as u32);
            let id_str = arxiv_id.clone().unwrap_or_else(|| {
                format!("hf-{}", title_lower.chars().take(20).collect::<String>())
            });
            let pdf_url = arxiv_id
                .as_ref()
                .map(|id| format!("https://arxiv.org/pdf/{}.pdf", id));
            let source_url = format!("https://huggingface.co/papers/{}", id_str);

            papers.push(Paper {
                id: format!("{}:{}", source_id, id_str),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some("Hugging Face Daily Papers / arXiv".to_string()),
                abstract_text: if summary.is_empty() {
                    None
                } else {
                    Some(clean_html_text(summary))
                },
                doi: None,
                source_url: Some(source_url),
                pdf_url,
                citations,
                quartile: None,
                source: source_id.to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}

/// OpenReview Notes API
pub async fn search_openreview(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api2.openreview.net/notes/search?term={}&limit={}",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGate-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "openreview: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(format!("openreview: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("openreview: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let notes = val.get("notes").and_then(|n| n.as_array());

    if let Some(list) = notes {
        for note in list {
            if papers.len() >= limit {
                break;
            }

            let note_id = note.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let content = note.get("content");

            // OpenReview mixes reviews and comments into the same feed; those notes
            // carry no title and are not papers, so drop them instead of listing a
            // placeholder row.
            let Some(title) = content
                .and_then(|c| c.get("title"))
                .and_then(|t| t.get("value").or(Some(t)))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            let authors = content
                .and_then(|c| c.get("authors"))
                .and_then(|a| a.get("value").or(Some(a)))
                .and_then(|arr| arr.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let summary = content
                .and_then(|c| c.get("abstract"))
                .and_then(|a| a.get("value").or(Some(a)))
                .and_then(|v| v.as_str())
                .map(clean_html_text);

            let year = note
                .get("cdate")
                .or_else(|| note.get("pdate"))
                .or_else(|| note.get("mdate"))
                .and_then(|d| d.as_i64())
                .and_then(|ms| {
                    chrono::DateTime::from_timestamp_millis(ms).map(|dt| {
                        use chrono::Datelike;
                        dt.year() as u32
                    })
                });

            let pdf_url = format!("https://openreview.net/pdf?id={}", note_id);
            let source_url = format!("https://openreview.net/forum?id={}", note_id);

            papers.push(Paper {
                id: format!("openreview:{}", note_id),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some("OpenReview (ICLR / NeurIPS / ICML)".to_string()),
                abstract_text: summary,
                doi: None,
                source_url: Some(source_url),
                pdf_url: Some(pdf_url),
                citations: None,
                quartile: None,
                source: "openreview".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}
