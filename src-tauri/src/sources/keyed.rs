use crate::models::Paper;
use crate::sources::clean_html_text;
use serde_json::Value;

/// Elsevier Scopus Search API
pub async fn search_scopus(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(key) = api_key else {
        return Err(format!(
            "{}scopus: requires SCOPUS_API_KEY",
            crate::sources::NEEDS_SETUP
        ));
    };

    let url = format!(
        "https://api.elsevier.com/content/search/scopus?query={}&count={}",
        urlencoding::encode(query),
        limit.clamp(1, 25)
    );

    let res = client
        .get(&url)
        .header("X-ELS-APIKey", key)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "scopus: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(crate::sources::keyed_http_error("scopus", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("scopus: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let entries = val
        .get("search-results")
        .and_then(|r| r.get("entry"))
        .and_then(|e| e.as_array());

    if let Some(list) = entries {
        for entry in list {
            if papers.len() >= limit {
                break;
            }

            let title = entry
                .get("dc:title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");
            let creator = entry.get("dc:creator").and_then(|v| v.as_str());
            let authors = creator.map(|c| vec![c.to_string()]).unwrap_or_default();

            let venue = entry
                .get("prism:publicationName")
                .and_then(|v| v.as_str())
                .unwrap_or("Scopus");
            let doi = entry
                .get("prism:doi")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let year = entry
                .get("prism:coverDate")
                .and_then(|d| d.as_str())
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let citations = entry
                .get("citedby-count")
                .and_then(|c| c.as_str())
                .and_then(|s| s.parse::<u32>().ok());

            let scopus_id = entry
                .get("dc:identifier")
                .and_then(|i| i.as_str())
                .unwrap_or("unknown");

            let source_url = doi
                .as_ref()
                .map(|d| format!("https://doi.org/{}", d))
                .or_else(|| {
                    entry
                        .get("link")
                        .and_then(|l| l.as_array())
                        .and_then(|arr| {
                            arr.iter()
                                .find(|x| x.get("@ref").and_then(|r| r.as_str()) == Some("scopus"))
                        })
                        .and_then(|l| l.get("@href"))
                        .and_then(|h| h.as_str())
                        .map(|s| s.to_string())
                });

            papers.push(Paper {
                id: format!("scopus:{}", scopus_id),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(venue.to_string()),
                abstract_text: None,
                doi,
                source_url,
                pdf_url: None,
                citations,
                quartile: None,
                source: "scopus".to_string(),
                score: None,
                open_access: false,
            });
        }
    }

    Ok(papers)
}

/// IEEE Xplore Search API
pub async fn search_ieee(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(key) = api_key else {
        return Err(format!(
            "{}ieee: requires IEEE_API_KEY",
            crate::sources::NEEDS_SETUP
        ));
    };

    let url = format!(
        "https://ieeexploreapi.ieee.org/api/v1/search/articles?querytext={}&max_records={}&apikey={}",
        urlencoding::encode(query),
        limit.clamp(1, 25),
        key
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "ieee: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(crate::sources::keyed_http_error("ieee", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("ieee: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let articles = val.get("articles").and_then(|a| a.as_array());

    if let Some(list) = articles {
        for art in list {
            if papers.len() >= limit {
                break;
            }

            let title = art
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("(untitled)");
            let venue = art
                .get("publication_title")
                .and_then(|v| v.as_str())
                .unwrap_or("IEEE");

            let authors = art
                .get("authors")
                .and_then(|a| a.get("authors"))
                .and_then(|arr| arr.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("full_name").and_then(|n| n.as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = art.get("publication_year").and_then(|y| {
                y.as_u64()
                    .map(|u| u as u32)
                    .or_else(|| y.as_str().and_then(|s| s.parse::<u32>().ok()))
            });

            let doi = art
                .get("doi")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            let citations = art
                .get("citing_paper_count")
                .and_then(|c| c.as_u64())
                .map(|u| u as u32);
            let art_num = art
                .get("article_number")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");

            let source_url = doi
                .as_ref()
                .map(|d| format!("https://doi.org/{}", d))
                .or_else(|| {
                    art.get("pdf_url")
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string())
                })
                .or_else(|| Some(format!("https://ieeexplore.ieee.org/document/{}", art_num)));

            papers.push(Paper {
                id: format!("ieee:{}", art_num),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(venue.to_string()),
                abstract_text: art
                    .get("abstract")
                    .and_then(|a| a.as_str())
                    .map(clean_html_text),
                doi,
                source_url,
                pdf_url: art
                    .get("pdf_url")
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string()),
                citations,
                quartile: None,
                source: "ieee".to_string(),
                score: None,
                open_access: false,
            });
        }
    }

    Ok(papers)
}

/// Springer Nature Search API
pub async fn search_springer(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(key) = api_key else {
        return Err(format!(
            "{}springer: requires SPRINGER_API_KEY",
            crate::sources::NEEDS_SETUP
        ));
    };

    let url = format!(
        "https://api.springernature.com/meta/v1/json?q={}&p={}&api_key={}",
        urlencoding::encode(query),
        limit.clamp(1, 25),
        key
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| {
            format!(
                "springer: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;

    if !res.status().is_success() {
        return Err(crate::sources::keyed_http_error("springer", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("springer: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let records = val.get("records").and_then(|r| r.as_array());

    if let Some(list) = records {
        for rec in list {
            if papers.len() >= limit {
                break;
            }

            let title = rec
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("(untitled)");
            let venue = rec
                .get("publicationName")
                .and_then(|v| v.as_str())
                .unwrap_or("Springer");
            let doi = rec
                .get("doi")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());

            let authors = rec
                .get("creators")
                .and_then(|c| c.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("creator").and_then(|c| c.as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = rec
                .get("publicationDate")
                .and_then(|d| d.as_str())
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let source_url = doi
                .as_ref()
                .map(|d| format!("https://doi.org/{}", d))
                .or_else(|| {
                    rec.get("url")
                        .and_then(|u| u.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|x| x.get("value"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });

            papers.push(Paper {
                id: format!("springer:{}", doi.as_deref().unwrap_or("unknown")),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(venue.to_string()),
                abstract_text: rec
                    .get("abstract")
                    .and_then(|a| a.as_str())
                    .map(clean_html_text),
                doi,
                source_url,
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "springer".to_string(),
                score: None,
                open_access: rec.get("openaccess").and_then(|oa| oa.as_str()) == Some("true"),
            });
        }
    }

    Ok(papers)
}

/// CORE v3 search (open-access aggregator).
pub async fn search_core(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(key) = api_key.filter(|key| !key.trim().is_empty()) else {
        return Err(format!(
            "{}core: requires CORE_API_KEY",
            crate::sources::NEEDS_SETUP
        ));
    };
    let url = format!(
        "https://api.core.ac.uk/v3/search/works?q={}&limit={}",
        urlencoding::encode(query),
        limit.clamp(1, 50)
    );
    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            format!(
                "core: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;
    if !res.status().is_success() {
        return Err(crate::sources::keyed_http_error("core", res.status()));
    }
    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("core: json parse failed: {}", e))?;
    let mut papers = Vec::new();
    if let Some(items) = val.get("results").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(title) = item
                .get("title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let authors = item
                .get("authors")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|a| a.get("name").and_then(Value::as_str))
                        .take(5)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let doi = item.get("doi").and_then(Value::as_str).map(str::to_string);
            let download = item
                .get("downloadUrl")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|u| u.to_lowercase().ends_with(".pdf"));
            let id = item
                .get("id")
                .map(|v| v.to_string())
                .unwrap_or_else(|| title.to_string());
            papers.push(Paper {
                id: format!("core:{id}"),
                title: clean_html_text(title),
                authors,
                year: item
                    .get("yearPublished")
                    .and_then(Value::as_u64)
                    .map(|y| y as u32),
                venue: item
                    .get("publisher")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                abstract_text: item
                    .get("abstract")
                    .and_then(Value::as_str)
                    .map(clean_html_text)
                    .filter(|t| !t.is_empty()),
                source_url: doi.as_ref().map(|d| format!("https://doi.org/{}", d)),
                doi,
                pdf_url: download,
                citations: None,
                quartile: None,
                source: "CORE".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// Dimensions publications search (requires an API key / token).
pub async fn search_dimensions(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(key) = api_key.filter(|key| !key.trim().is_empty()) else {
        return Err(format!(
            "{}dimensions: requires DIMENSIONS_API_KEY",
            crate::sources::NEEDS_SETUP
        ));
    };
    let url = format!(
        "https://api.dimensions.ai/details/publications?search_mode=content&search_text={}&search_type=kws&return_type=publications&per_page={}",
        urlencoding::encode(query),
        limit.clamp(1, 50)
    );
    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            format!(
                "dimensions: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;
    if !res.status().is_success() {
        return Err(crate::sources::keyed_http_error("dimensions", res.status()));
    }
    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("dimensions: json parse failed: {}", e))?;
    let mut papers = Vec::new();
    if let Some(items) = val.get("publications").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(title) = item
                .get("title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let authors = item
                .get("authors")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|a| {
                            let first = a.get("first_name").and_then(Value::as_str).unwrap_or("");
                            let last = a.get("last_name").and_then(Value::as_str).unwrap_or("");
                            let name = format!("{} {}", first, last).trim().to_string();
                            (!name.is_empty()).then_some(name)
                        })
                        .take(5)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let doi = item.get("doi").and_then(Value::as_str).map(str::to_string);
            let id = item
                .get("id")
                .map(|v| v.to_string())
                .unwrap_or_else(|| title.to_string());
            papers.push(Paper {
                id: format!("dimensions:{id}"),
                title: clean_html_text(title),
                authors,
                year: item.get("year").and_then(Value::as_u64).map(|y| y as u32),
                venue: item
                    .get("journal")
                    .and_then(|j| j.get("title"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                abstract_text: item
                    .get("abstract")
                    .and_then(Value::as_str)
                    .map(clean_html_text)
                    .filter(|t| !t.is_empty()),
                source_url: doi.as_ref().map(|d| format!("https://doi.org/{}", d)),
                doi,
                pdf_url: None,
                citations: item
                    .get("times_cited")
                    .and_then(Value::as_u64)
                    .map(|c| c as u32),
                quartile: None,
                source: "Dimensions".to_string(),
                score: None,
                open_access: false,
            });
        }
    }
    Ok(papers)
}

/// Web of Science Starter API.
pub async fn search_web_of_science(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(key) = api_key.filter(|key| !key.trim().is_empty()) else {
        return Err(format!(
            "{}web_of_science: requires WOS_API_KEY",
            crate::sources::NEEDS_SETUP
        ));
    };
    let url = format!(
        "https://api.clarivate.com/apis/wos-starter/v1/documents?q=TS%3D({})&db=WOS&limit={}",
        urlencoding::encode(query),
        limit.clamp(1, 50)
    );
    let res = client
        .get(&url)
        .header("X-ApiKey", key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            format!(
                "web_of_science: request failed — {}",
                crate::sources::transport_reason(&e)
            )
        })?;
    if !res.status().is_success() {
        return Err(crate::sources::keyed_http_error(
            "web_of_science",
            res.status(),
        ));
    }
    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("web_of_science: json parse failed: {}", e))?;
    let mut papers = Vec::new();
    if let Some(items) = val.get("hits").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(title) = item
                .get("title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let authors = item
                .get("names")
                .and_then(|n| n.get("authors"))
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|a| a.get("displayName").and_then(Value::as_str))
                        .take(5)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let doi = item
                .get("identifiers")
                .and_then(|ids| ids.get("doi"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let uid = item.get("uid").and_then(Value::as_str).unwrap_or(title);
            let source_url = doi
                .as_ref()
                .map(|d| format!("https://doi.org/{}", d))
                .or_else(|| {
                    Some(format!(
                        "https://www.webofscience.com/wos/woscc/full-record/{}",
                        uid
                    ))
                });
            papers.push(Paper {
                id: format!("wos:{}", uid),
                title: clean_html_text(title),
                authors,
                year: item
                    .get("dates")
                    .and_then(|d| d.get("publicationDate"))
                    .and_then(Value::as_str)
                    .and_then(|d| d.get(0..4))
                    .and_then(|y| y.parse::<u32>().ok()),
                venue: item
                    .get("source")
                    .and_then(|s| s.get("sourceTitle"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                abstract_text: item
                    .get("abstract")
                    .and_then(Value::as_str)
                    .map(clean_html_text)
                    .filter(|t| !t.is_empty()),
                doi,
                source_url,
                pdf_url: None,
                citations: item
                    .get("citations")
                    .and_then(|c| c.get("count"))
                    .and_then(Value::as_u64)
                    .map(|c| c as u32),
                quartile: None,
                source: "Web of Science".to_string(),
                score: None,
                open_access: false,
            });
        }
    }
    Ok(papers)
}
