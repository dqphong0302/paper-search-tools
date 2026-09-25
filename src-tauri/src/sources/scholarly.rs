//! The first-party scholarly APIs the engine queries directly (OpenAlex,
//! PubMed, arXiv, Crossref, Semantic Scholar, DOAJ, Zenodo, HAL, Europe PMC,
//! SearXNG) and the record parsers they share. The dispatch table in
//! `registry.rs` decides when each one runs.

use crate::engine::{polite_get, AcademicEngine, ARXIV_GATE, S2_GATE};
use crate::models::{Paper, SourceCredentials};
use std::time::Duration;

impl AcademicEngine {
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
    }
    .non_empty()
}

pub(crate) fn reconstruct_abstract(val: Option<&serde_json::Value>) -> Option<String> {
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

pub(crate) fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    let start = xml.find(&start_tag)? + start_tag.len();
    let end = xml[start..].find(&end_tag)? + start;
    Some(xml[start..end].trim().to_string())
}

pub(crate) fn parse_arxiv_feed(xml_text: &str) -> Result<Vec<Paper>, String> {
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
