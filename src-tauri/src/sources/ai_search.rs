use crate::models::Paper;
use crate::sources::clean_html_text;
use serde_json::Value;

/// Perplexity Sonar Academic Search
pub async fn search_perplexity(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
    base_url: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(key) = api_key else {
        return Err(format!("{}perplexity: requires PERPLEXITY_API_KEY", crate::sources::NEEDS_SETUP));
    };

    let base = base_url.unwrap_or("https://api.perplexity.ai").trim_end_matches('/');
    let url = format!("{}/chat/completions", base);

    let system_prompt = "You are an academic literature search engine. Return up to 10 relevant peer-reviewed papers for the user's research query as a strict JSON array of objects with keys: title, authors (array), year (number), venue (string), abstract (string), doi (string or null), source_url (string or null). Only output the JSON array inside a json markdown fence.";

    let payload = serde_json::json!({
        "model": "sonar-pro",
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": query }
        ],
        "temperature": 0.2
    });

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("perplexity: request failed — {}", crate::sources::transport_reason(&e)))?;

    if !res.status().is_success() {
        return Err(format!("perplexity: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("perplexity: json parse error: {}", e))?;

    let content = val
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    // Extract JSON array from markdown
    let json_str = if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            &after[..end]
        } else {
            after
        }
    } else if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            &content[start..=end]
        } else {
            content
        }
    } else {
        content
    };

    let parsed: Vec<Value> = serde_json::from_str(json_str.trim())
        .map_err(|e| format!("perplexity: failed to parse structured output: {}", e))?;

    let mut papers = Vec::new();
    for (idx, item) in parsed.into_iter().enumerate() {
        if papers.len() >= limit {
            break;
        }

        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("(untitled)");
        let venue = item.get("venue").and_then(|v| v.as_str()).unwrap_or("Perplexity Sonar Pro");
        let doi = item.get("doi").and_then(|v| v.as_str()).map(|s| s.to_string());
        let source_url = item.get("source_url").and_then(|v| v.as_str()).map(|s| s.to_string())
            .or_else(|| doi.as_ref().map(|d| format!("https://doi.org/{}", d)));

        let authors = item
            .get("authors")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let year = item
            .get("year")
            .and_then(|y| y.as_u64().map(|n| n as u32).or_else(|| y.as_str().and_then(|s| s.parse::<u32>().ok())));

        let abstract_text = item.get("abstract").and_then(|a| a.as_str()).map(clean_html_text);

        papers.push(Paper {
            id: format!("perplexity:{}", doi.as_deref().unwrap_or(&format!("p-{}", idx))),
            title: clean_html_text(title),
            authors,
            year,
            venue: Some(venue.to_string()),
            abstract_text,
            doi,
            source_url,
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "perplexity".to_string(),
            score: None,
            open_access: false,
        });
    }

    Ok(papers)
}

/// Consensus.app — AI Evidence Search (RCT, Meta-analysis, Study Types)
pub async fn search_consensus(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    session: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(sess) = session.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(format!("{}consensus: requires consensus_session (sign in from Settings)", crate::sources::NEEDS_SETUP));
    };

    let url = "https://consensus.app/api/paper_search/";
    let payload = serde_json::json!({
        "query": query,
        "page": 1,
        "page_size": limit.min(30),
        "product_feature": "quick_search"
    });

    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .header("Referer", "https://consensus.app/search/")
        .header("Origin", "https://consensus.app")
        .json(&payload);

    if sess.contains('=') || sess.contains(';') {
        req = req.header("Cookie", sess);
    } else {
        req = req.header("Cookie", format!("__session={}", sess))
                 .header("Authorization", format!("Bearer {}", sess));
    }

    let res = req.send().await.map_err(|e| format!("consensus: request failed — {}", crate::sources::transport_reason(&e)))?;

    if res.status() == reqwest::StatusCode::UNAUTHORIZED || res.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("consensus: session expired or invalid, please sign in again".to_string());
    }

    if !res.status().is_success() {
        return Err(format!("consensus: HTTP {}", res.status()));
    }

    let body: Value = res.json().await.map_err(|e| format!("consensus: json error: {}", e))?;

    let items = body.get("papers")
        .or_else(|| body.get("results"))
        .or_else(|| body.get("claims"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut papers = Vec::new();
    for (idx, raw_item) in items.into_iter().enumerate() {
        if papers.len() >= limit {
            break;
        }

        let p = if raw_item.get("paper").is_some() {
            raw_item.get("paper").unwrap().clone()
        } else {
            raw_item.clone()
        };

        let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("(untitled)");
        let doi = p.get("doi").and_then(|v| v.as_str()).map(str::to_string);
        let mut source_url = p.get("url")
            .or_else(|| p.get("doi_url"))
            .or_else(|| p.get("paper_url"))
            .or_else(|| p.get("open_access_pdf_url"))
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if source_url.is_none() {
            if let (Some(slug), Some(id)) = (p.get("url_slug").and_then(|v| v.as_str()), p.get("paper_id").and_then(|v| v.as_str())) {
                source_url = Some(format!("https://consensus.app/papers/{}/{}", slug, id));
            } else if let Some(ref d) = doi {
                source_url = Some(format!("https://doi.org/{}", d));
            }
        }

        let journal = p.get("journal")
            .or_else(|| p.get("source_title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Consensus.app");

        let study_type = p.get("badges")
            .and_then(|b| b.get("study_type"))
            .or_else(|| p.get("study_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let venue = if !study_type.is_empty() {
            format!("{} [{}]", journal, study_type.to_uppercase())
        } else {
            journal.to_string()
        };

        let claim = p.get("display_text")
            .or_else(|| raw_item.get("claim"))
            .or_else(|| raw_item.get("text"))
            .or_else(|| p.get("claim"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mut abstract_text = p.get("abstract")
            .or_else(|| p.get("summary"))
            .and_then(|v| v.as_str())
            .map(clean_html_text);

        if abstract_text.as_deref().unwrap_or("").is_empty() && !claim.is_empty() {
            abstract_text = Some(clean_html_text(claim));
        }

        let year = p.get("year")
            .or_else(|| p.get("publication_year"))
            .and_then(|y| y.as_u64().map(|n| n as u32).or_else(|| y.as_str().and_then(|s| s.parse::<u32>().ok())));

        let citations = p.get("citation_count")
            .or_else(|| p.get("citations"))
            .and_then(|c| c.as_u64().map(|n| n as u32));

        let authors = p.get("authors")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| {
                        if let Some(s) = x.as_str() {
                            Some(s.to_string())
                        } else if let Some(name) = x.get("name").and_then(|n| n.as_str()) {
                            Some(name.to_string())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let id = doi.as_ref()
            .map(|d| format!("doi:{}", d))
            .unwrap_or_else(|| format!("consensus:{}", idx));

        papers.push(Paper {
            id,
            title: clean_html_text(title),
            authors,
            year,
            venue: Some(venue),
            abstract_text,
            doi,
            source_url,
            pdf_url: None,
            citations,
            quartile: None,
            source: "consensus".to_string(),
            score: None,
            open_access: false,
        });
    }

    Ok(papers)
}

/// OpenEvidence — Clinical Evidence Assistant
pub async fn search_openevidence(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    session: Option<&str>,
) -> Result<Vec<Paper>, String> {
    let Some(sess) = session.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(format!("{}openevidence: requires openevidence_session (sign in from Settings)", crate::sources::NEEDS_SETUP));
    };

    let url = "https://www.openevidence.com/api/chat";
    let payload = serde_json::json!({
        "message": query
    });

    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .header("Referer", "https://www.openevidence.com/")
        .header("Origin", "https://www.openevidence.com")
        .json(&payload);

    if sess.contains('=') || sess.contains(';') {
        req = req.header("Cookie", sess);
    } else {
        req = req.header("Authorization", format!("Bearer {}", sess))
                 .header("x-openevidence-key", sess);
    }

    let res = req.send().await.map_err(|e| format!("openevidence: request failed — {}", crate::sources::transport_reason(&e)))?;

    if res.status() == reqwest::StatusCode::UNAUTHORIZED || res.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("openevidence: session expired or invalid, please sign in again".to_string());
    }

    if !res.status().is_success() {
        return Err(format!("openevidence: HTTP {}", res.status()));
    }

    let body: Value = res.json().await.map_err(|e| format!("openevidence: json error: {}", e))?;

    // Collect citations from possible buckets
    let buckets = [
        body.get("citations"),
        body.get("references"),
        body.get("sources"),
        body.pointer("/data/citations"),
        body.pointer("/result/citations"),
        body.pointer("/message/citations"),
    ];

    let items = buckets.into_iter()
        .flatten()
        .find_map(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut papers = Vec::new();
    for (idx, item) in items.into_iter().enumerate() {
        if papers.len() >= limit {
            break;
        }

        let title = item.get("title")
            .or_else(|| item.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("(untitled citation)");

        let doi = item.get("doi").and_then(|v| v.as_str()).map(str::to_string);
        let source_url = item.get("url")
            .or_else(|| item.get("link"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| doi.as_ref().map(|d| format!("https://doi.org/{}", d)));

        let venue = item.get("journal")
            .or_else(|| item.get("venue"))
            .or_else(|| item.get("source"))
            .and_then(|v| v.as_str())
            .unwrap_or("OpenEvidence Clinical Evidence");

        let abstract_text = item.get("snippet")
            .or_else(|| item.get("text"))
            .or_else(|| item.get("abstract"))
            .and_then(|v| v.as_str())
            .map(clean_html_text);

        let year = item.get("year")
            .and_then(|y| y.as_u64().map(|n| n as u32).or_else(|| y.as_str().and_then(|s| s.parse::<u32>().ok())));

        let authors = item.get("authors")
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>())
            .unwrap_or_default();

        let id = doi.as_ref()
            .map(|d| format!("doi:{}", d))
            .unwrap_or_else(|| format!("openevidence:{}", idx));

        papers.push(Paper {
            id,
            title: clean_html_text(title),
            authors,
            year,
            venue: Some(venue.to_string()),
            abstract_text,
            doi,
            source_url,
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "openevidence".to_string(),
            score: None,
            open_access: false,
        });
    }

    Ok(papers)
}
