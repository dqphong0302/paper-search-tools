use crate::models::Paper;
use crate::sources::clean_html_text;
use serde_json::Value;

/// ClinicalTrials.gov REST API v2
pub async fn search_clinicaltrials(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let page_size = limit.clamp(1, 30);
    let url = format!(
        "https://clinicaltrials.gov/api/v2/studies?query.term={}&pageSize={}&format=json",
        urlencoding::encode(query),
        page_size
    );

    let res = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| format!("clinicaltrials: request failed — {}", crate::sources::transport_reason(&e)))?;

    if !res.status().is_success() {
        return Err(format!("clinicaltrials: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("clinicaltrials: json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let studies = val.get("studies").and_then(|v| v.as_array());

    if let Some(list) = studies {
        for study in list {
            if papers.len() >= limit {
                break;
            }
            let protocol = study.get("protocolSection");
            let ident = protocol.and_then(|p| p.get("identificationModule"));
            let nct_id = ident
                .and_then(|i| i.get("nctId"))
                .and_then(|v| v.as_str())
                .unwrap_or("NCT00000000");

            let brief_title = ident
                .and_then(|i| i.get("briefTitle"))
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled study)");

            let lead_sponsor = protocol
                .and_then(|p| p.get("sponsorCollaboratorsModule"))
                .and_then(|s| s.get("leadSponsor"))
                .and_then(|l| l.get("name"))
                .and_then(|v| v.as_str());

            let authors = if let Some(sp) = lead_sponsor {
                vec![sp.to_string()]
            } else {
                vec!["ClinicalTrials.gov Investigator".to_string()]
            };

            let start_date = protocol
                .and_then(|p| p.get("statusModule"))
                .and_then(|s| s.get("startDateStruct"))
                .and_then(|d| d.get("date"))
                .and_then(|v| v.as_str());

            let year = start_date
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let summary = protocol
                .and_then(|p| p.get("descriptionModule"))
                .and_then(|d| d.get("briefSummary"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let phases = protocol
                .and_then(|p| p.get("designModule"))
                .and_then(|d| d.get("phases"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "Not Applicable".to_string());

            let status = protocol
                .and_then(|p| p.get("statusModule"))
                .and_then(|s| s.get("overallStatus"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let title = format!("[Clinical Trial {}] {}", nct_id, brief_title);
            let venue = format!("ClinicalTrials.gov ({}) - {}", phases, status);
            let source_url = format!("https://clinicaltrials.gov/study/{}", nct_id);

            papers.push(Paper {
                id: format!("nct:{}", nct_id),
                title,
                authors,
                year,
                venue: Some(venue),
                abstract_text: summary,
                doi: None,
                source_url: Some(source_url),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "clinicaltrials".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}

/// bioRxiv / medRxiv via Crossref filter or Europe PMC
pub async fn search_biorxiv_medrxiv(
    client: &reqwest::Client,
    source_id: &str, // "biorxiv" or "medrxiv"
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    // Crossref filter by prefix: 10.1101 is Cold Spring Harbor Laboratory (bioRxiv/medRxiv)
    let url = format!(
        "https://api.crossref.org/works?query={}&filter=prefix:10.1101&rows={}&mailto=scholar-gateway@desktop.local",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&url)
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| format!("{}: request failed — {}", source_id, crate::sources::transport_reason(&e)))?;

    if !res.status().is_success() {
        return Err(format!("{}: HTTP {}", source_id, res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("{}: json parse failed: {}", source_id, e))?;

    let mut papers = Vec::new();
    let items = val
        .get("message")
        .and_then(|m| m.get("items"))
        .and_then(|i| i.as_array());

    if let Some(list) = items {
        for item in list {
            if papers.len() >= limit {
                break;
            }
            let title = item
                .get("title")
                .and_then(|t| t.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled preprint)");

            let doi = item
                .get("DOI")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());

            let mut authors = Vec::new();
            if let Some(auth_list) = item.get("author").and_then(|a| a.as_array()) {
                for auth in auth_list {
                    let given = auth.get("given").and_then(|v| v.as_str()).unwrap_or("");
                    let family = auth.get("family").and_then(|v| v.as_str()).unwrap_or("");
                    let name = format!("{} {}", given, family).trim().to_string();
                    if !name.is_empty() {
                        authors.push(name);
                    }
                }
            }

            let year = item
                .get("published-print")
                .or_else(|| item.get("published-online"))
                .or_else(|| item.get("created"))
                .and_then(|p| p.get("date-parts"))
                .and_then(|dp| dp.as_array())
                .and_then(|arr| arr.first())
                .and_then(|parts| parts.as_array())
                .and_then(|p| p.first())
                .and_then(|y| y.as_u64())
                .map(|y| y as u32);

            let pdf_url = doi.as_ref().map(|d| format!("https://www.biorxiv.org/content/{}.full.pdf", d));
            let source_url = doi
                .as_ref()
                .map(|d| format!("https://doi.org/{}", d))
                .or_else(|| item.get("URL").and_then(|v| v.as_str()).map(|s| s.to_string()));

            let venue = if source_id == "medrxiv" {
                "medRxiv Preprints"
            } else {
                "bioRxiv Preprints"
            };

            papers.push(Paper {
                id: format!("{}:{}", source_id, doi.as_deref().unwrap_or("unknown")),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(venue.to_string()),
                abstract_text: None,
                doi,
                source_url,
                pdf_url,
                citations: None,
                quartile: None,
                source: source_id.to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}

/// PLOS (Public Library of Science) Search API
pub async fn search_plos(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.plos.org/search?q={}&fl=id,title,author_display,publication_date,journal,abstract&wt=json&rows={}",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&url)
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| format!("plos: request failed — {}", crate::sources::transport_reason(&e)))?;

    if !res.status().is_success() {
        return Err(format!("plos: HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("plos: json parse failed: {}", e))?;

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

            let doi = doc.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let title = doc.get("title").and_then(|v| v.as_str()).unwrap_or("(untitled)");
            let venue = doc.get("journal").and_then(|v| v.as_str()).unwrap_or("PLOS");

            let authors = doc
                .get("author_display")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = doc
                .get("publication_date")
                .and_then(|d| d.as_str())
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let abstract_text = doc
                .get("abstract")
                .and_then(|a| a.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .map(clean_html_text);

            let source_url = format!("https://doi.org/{}", doi);
            let pdf_url = format!("https://journals.plos.org/plosone/article/file?id={}&type=printable", doi);

            papers.push(Paper {
                id: format!("plos:{}", doi),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(venue.to_string()),
                abstract_text,
                doi: Some(doi.to_string()),
                source_url: Some(source_url),
                pdf_url: Some(pdf_url),
                citations: None,
                quartile: None,
                source: "plos".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}

/// PMC (PubMed Central) Open Access Search via E-Utilities
pub async fn search_pmc(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let esearch_url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pmc&term={}&retmode=json&retmax={}",
        urlencoding::encode(query),
        limit.clamp(1, 30)
    );

    let res = client
        .get(&esearch_url)
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| format!("pmc: esearch failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("pmc: esearch HTTP {}", res.status()));
    }

    let val: Value = res
        .json()
        .await
        .map_err(|e| format!("pmc: esearch json parse failed: {}", e))?;

    let id_list = val
        .get("esearchresult")
        .and_then(|r| r.get("idlist"))
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if id_list.is_empty() {
        return Ok(Vec::new());
    }

    let esummary_url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pmc&id={}&retmode=json",
        id_list.join(",")
    );

    let sum_res = client
        .get(&esummary_url)
        .header("User-Agent", "ScholarGateway-Desktop/1.0")
        .send()
        .await
        .map_err(|e| format!("pmc: esummary failed: {}", e))?;

    if !sum_res.status().is_success() {
        return Err(format!("pmc: esummary HTTP {}", sum_res.status()));
    }

    let sum_val: Value = sum_res
        .json()
        .await
        .map_err(|e| format!("pmc: esummary json parse failed: {}", e))?;

    let mut papers = Vec::new();
    let result_obj = sum_val.get("result").and_then(|r| r.as_object());

    if let Some(res_map) = result_obj {
        for id in id_list {
            let Some(item) = res_map.get(id) else { continue };
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled PMC article)");

            let venue = item
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("PubMed Central");

            let authors = item
                .get("authors")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let year = item
                .get("pubdate")
                .and_then(|d| d.as_str())
                .and_then(|s| s.get(0..4))
                .and_then(|s| s.parse::<u32>().ok());

            let doi = item
                .get("articleids")
                .and_then(|a| a.as_array())
                .and_then(|arr| {
                    arr.iter().find(|x| x.get("idtype").and_then(|t| t.as_str()) == Some("doi"))
                })
                .and_then(|d| d.get("value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let source_url = format!("https://www.ncbi.nlm.nih.gov/pmc/articles/PMC{}/", id);
            let pdf_url = format!("https://www.ncbi.nlm.nih.gov/pmc/articles/PMC{}/pdf/", id);

            papers.push(Paper {
                id: format!("pmc:PMC{}", id),
                title: clean_html_text(title),
                authors,
                year,
                venue: Some(venue.to_string()),
                abstract_text: None,
                doi,
                source_url: Some(source_url),
                pdf_url: Some(pdf_url),
                citations: None,
                quartile: None,
                source: "pmc".to_string(),
                score: None,
                open_access: true,
            });
        }
    }

    Ok(papers)
}
