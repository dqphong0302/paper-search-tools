//! Additional public, key-free research repositories. Each parser is defensive:
//! it skips records without a usable title instead of inventing metadata.
use crate::models::Paper;
use crate::sources::clean_html_text;
use serde_json::Value;

fn year_from(value: Option<&str>) -> Option<u32> {
    let text = value?;
    let start = text.find(|c: char| c.is_ascii_digit())?;
    text.get(start..start + 4)
        .and_then(|year| year.parse::<u32>().ok())
}

fn json_field<'a>(item: &'a Value, key: &str) -> Option<&'a Value> {
    item.get(key)
}

/// CiNii Research (Japan) OpenSearch JSON.
pub async fn search_cinii(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://cir.nii.ac.jp/opensearch/all?q={}&format=json&count={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "cinii").await?;
    let mut papers = Vec::new();
    if let Some(items) = json.get("items").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(title) = item
                .get("title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let authors = item
                .get("dc:creator")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|creator| match creator {
                            Value::String(name) => Some(name.clone()),
                            Value::Object(_) => creator
                                .get("foaf:name")
                                .or_else(|| creator.get("name"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            _ => None,
                        })
                        .take(5)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let link = item
                .get("link")
                .and_then(|l| l.get("@id"))
                .and_then(Value::as_str);
            let doi = item
                .get("rdfs:seeAlso")
                .and_then(|l| l.get("@id"))
                .and_then(Value::as_str)
                .filter(|url| url.contains("doi.org"))
                .map(str::to_string);
            let id = item
                .get("@id")
                .and_then(Value::as_str)
                .or(link)
                .unwrap_or(title)
                .to_string();
            papers.push(Paper {
                id: format!("cinii:{id}"),
                title: clean_html_text(title),
                authors,
                year: year_from(item.get("prism:publicationDate").and_then(Value::as_str)),
                venue: item
                    .get("prism:publicationName")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                abstract_text: None,
                doi,
                source_url: link
                    .or(item.get("@id").and_then(Value::as_str))
                    .map(str::to_string),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "CiNii".to_string(),
                score: None,
                open_access: false,
            });
        }
    }
    Ok(papers)
}

/// Dryad research-data repository.
pub async fn search_dryad(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://datadryad.org/api/v2/search?q={}&per_page={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "dryad").await?;
    let mut papers = Vec::new();
    if let Some(items) = json
        .get("_embedded")
        .and_then(|value| value.get("stash:datasets"))
        .and_then(Value::as_array)
    {
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
                        .filter_map(|author| {
                            let given = author
                                .get("givenName")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            let family = author
                                .get("familyName")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            let name = format!("{} {}", given, family).trim().to_string();
                            (!name.is_empty()).then_some(name)
                        })
                        .take(5)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let doi = item
                .get("identifier")
                .and_then(Value::as_str)
                .map(|id| id.trim_start_matches("doi:").to_string());
            let id = item
                .get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .unwrap_or_else(|| title.to_string());
            papers.push(Paper {
                id: format!("dryad:{id}"),
                title: title.to_string(),
                authors,
                year: year_from(item.get("publicationDate").and_then(Value::as_str)).or_else(
                    || year_from(item.get("lastModificationDate").and_then(Value::as_str)),
                ),
                venue: item
                    .get("fieldOfScience")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                abstract_text: item
                    .get("abstract")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                source_url: doi.as_ref().map(|doi| format!("https://doi.org/{doi}")),
                doi,
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "Dryad".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// Dataverse dataset search (Harvard Dataverse).
pub async fn search_dataverse(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://dataverse.harvard.edu/api/search?q={}&type=dataset&per_page={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "dataverse").await?;
    let mut papers = Vec::new();
    if let Some(items) = json
        .get("data")
        .and_then(|value| value.get("items"))
        .and_then(Value::as_array)
    {
        for item in items.iter().take(limit) {
            let Some(title) = item
                .get("name")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let url = item.get("url").and_then(Value::as_str).map(str::to_string);
            let global_id = item
                .get("global_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let doi = global_id
                .as_ref()
                .map(|id| id.trim_start_matches("doi:").to_string());
            papers.push(Paper {
                id: format!(
                    "dataverse:{}",
                    global_id.clone().unwrap_or_else(|| title.to_string())
                ),
                title: title.to_string(),
                authors: Vec::new(),
                year: year_from(item.get("published_at").and_then(Value::as_str)),
                venue: item
                    .get("publisher")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                abstract_text: item
                    .get("description")
                    .and_then(Value::as_str)
                    .map(clean_html_text)
                    .filter(|text| !text.is_empty()),
                source_url: url
                    .clone()
                    .or_else(|| doi.as_ref().map(|doi| format!("https://doi.org/{doi}"))),
                doi,
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "Dataverse".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// NASA Technical Reports Server.
pub async fn search_ntrs(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://ntrs.nasa.gov/api/citations/search?q={}&page.size={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "ntrs").await?;
    let mut papers = Vec::new();
    if let Some(items) = json.get("results").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(title) = item
                .get("title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            // NTRS returns the citation id as a JSON number.
            let id = item
                .get("id")
                .and_then(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| value.as_u64().map(|number| number.to_string()))
                })
                .unwrap_or_else(|| title.to_string());
            papers.push(Paper {
                id: format!("ntrs:{id}"),
                title: title.to_string(),
                authors: Vec::new(),
                year: year_from(item.get("distributionDate").and_then(Value::as_str)),
                venue: item
                    .get("stiType")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| Some("NASA NTRS".into())),
                abstract_text: item
                    .get("abstract")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                doi: None,
                source_url: Some(format!("https://ntrs.nasa.gov/citations/{id}")),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "NTRS".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// World Bank Documents & Reports.
pub async fn search_worldbank(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://search.worldbank.org/api/v2/wds?format=json&qterm={}&rows={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "worldbank").await?;
    let mut papers = Vec::new();
    if let Some(documents) = json.get("documents").and_then(Value::as_object) {
        for (key, doc) in documents.iter().filter(|(key, _)| key.as_str() != "facets") {
            if papers.len() >= limit {
                break;
            }
            let Some(title) = doc
                .get("display_title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            papers.push(Paper {
                id: format!("worldbank:{key}"),
                title: title.trim().to_string(),
                authors: Vec::new(),
                year: year_from(doc.get("docdt").and_then(Value::as_str)),
                venue: Some("World Bank".to_string()),
                abstract_text: None,
                doi: None,
                source_url: doc
                    .get("url")
                    .and_then(Value::as_str)
                    .or_else(|| doc.get("pdfurl").and_then(Value::as_str))
                    .map(str::to_string),
                pdf_url: doc
                    .get("pdfurl")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                citations: None,
                quartile: None,
                source: "World Bank".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// DOAB — Directory of Open Access Books (DSpace REST).
pub async fn search_doab(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://directory.doabooks.org/rest/search?query={}&expand=metadata",
        urlencoding::encode(query)
    );
    let json = get_json(client, &url, "doab").await?;
    let empty = Vec::new();
    let items = json.as_array().unwrap_or(&empty);
    let mut papers = Vec::new();
    for item in items.iter().take(limit * 2) {
        if papers.len() >= limit {
            break;
        }
        let metadata = json_field(item, "metadata").and_then(Value::as_array);
        let meta_value = |key: &str| -> Option<String> {
            metadata?
                .iter()
                .find(|entry| entry.get("key").and_then(Value::as_str) == Some(key))
                .and_then(|entry| entry.get("value"))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let Some(title) = meta_value("dc.title") else {
            continue;
        };
        let authors = metadata
            .map(|list| {
                list.iter()
                    .filter(|entry| {
                        entry.get("key").and_then(Value::as_str) == Some("dc.contributor.author")
                    })
                    .filter_map(|entry| entry.get("value").and_then(Value::as_str))
                    .take(5)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let handle = item.get("handle").and_then(Value::as_str);
        let uri = meta_value("dc.identifier.uri");
        let doi = meta_value("dc.identifier.doi");
        papers.push(Paper {
            id: format!(
                "doab:{}",
                item.get("uuid").and_then(Value::as_str).unwrap_or(&title)
            ),
            title: clean_html_text(&title),
            authors,
            year: year_from(meta_value("dc.date.issued").as_deref()),
            venue: meta_value("dc.publisher"),
            abstract_text: meta_value("dc.description.abstract").map(|text| clean_html_text(&text)),
            doi,
            source_url: uri.or_else(|| {
                handle.map(|handle| format!("https://directory.doabooks.org/handle/{handle}"))
            }),
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "DOAB".to_string(),
            score: None,
            open_access: true,
        });
    }
    Ok(papers)
}

fn first_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(list) => list.iter().find_map(first_text),
        Value::Object(object) => object.get("$").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

fn text_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(list) => list.iter().filter_map(first_text).collect(),
        _ => first_text(value).into_iter().collect(),
    }
}

/// OpenAIRE — open-access aggregator. Response is deeply nested SOAP-in-JSON.
pub async fn search_openaire(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.openaire.eu/search/publications?keywords={}&size={}&format=json",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "openaire").await?;
    let mut papers = Vec::new();
    let results = json
        .get("response")
        .and_then(|value| value.get("results"))
        .and_then(|value| value.get("result"))
        .and_then(Value::as_array);
    if let Some(results) = results {
        for item in results.iter().take(limit) {
            let Some(entity) = item
                .get("metadata")
                .and_then(|value| value.get("oaf:entity"))
                .and_then(|value| value.get("oaf:result"))
            else {
                continue;
            };
            let Some(title) = entity
                .get("title")
                .and_then(first_text)
                .filter(|text| !text.trim().is_empty())
            else {
                continue;
            };
            let creators: Vec<String> = entity
                .get("creator")
                .map(text_list)
                .unwrap_or_default()
                .into_iter()
                .take(5)
                .collect();
            let doi = entity
                .get("pid")
                .and_then(|value| value.as_array())
                .and_then(|list| {
                    list.iter().find_map(|pid| {
                        let class = pid.get("@classid").and_then(Value::as_str);
                        (class == Some("doi"))
                            .then(|| pid.get("$").and_then(Value::as_str).map(str::to_string))
                            .flatten()
                    })
                });
            let id = doi
                .clone()
                .or_else(|| entity.get("originalId").and_then(first_text))
                .unwrap_or_else(|| title.clone());
            papers.push(Paper {
                id: format!("openaire:{id}"),
                title: clean_html_text(&title),
                authors: creators,
                year: year_from(
                    entity
                        .get("dateofacceptance")
                        .and_then(first_text)
                        .as_deref(),
                ),
                venue: entity.get("publisher").and_then(first_text),
                abstract_text: entity
                    .get("description")
                    .and_then(first_text)
                    .map(|text| clean_html_text(&text)),
                source_url: doi.as_ref().map(|doi| format!("https://doi.org/{doi}")),
                doi,
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "OpenAIRE".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// NVD CVE — security advisories (non-article records).
pub async fn search_cve_nvd(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch={}&resultsPerPage={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "cve_nvd").await?;
    let mut papers = Vec::new();
    if let Some(items) = json.get("vulnerabilities").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(cve) = item.get("cve") else { continue };
            let Some(id) = cve.get("id").and_then(Value::as_str) else {
                continue;
            };
            let description = cve
                .get("descriptions")
                .and_then(Value::as_array)
                .and_then(|list| {
                    list.iter()
                        .find_map(|entry| entry.get("value").and_then(Value::as_str))
                });
            papers.push(Paper {
                id: format!("cve:{id}"),
                title: format!(
                    "{id}: {}",
                    description
                        .unwrap_or("Security advisory")
                        .chars()
                        .take(90)
                        .collect::<String>()
                ),
                authors: Vec::new(),
                year: year_from(cve.get("published").and_then(Value::as_str)),
                venue: Some("NVD (NIST)".to_string()),
                abstract_text: description.map(str::to_string),
                doi: None,
                source_url: Some(format!("https://nvd.nist.gov/vuln/detail/{id}")),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "NVD CVE".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// Hugging Face datasets (non-article records).
pub async fn search_huggingface_datasets(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://huggingface.co/api/datasets?search={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "hf_datasets").await?;
    let mut papers = Vec::new();
    let empty = Vec::new();
    for item in json.as_array().unwrap_or(&empty).iter().take(limit) {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        papers.push(Paper {
            id: format!("hfds:{id}"),
            title: id.to_string(),
            authors: item
                .get("author")
                .and_then(Value::as_str)
                .map(|author| vec![author.to_string()])
                .unwrap_or_default(),
            year: year_from(item.get("lastModified").and_then(Value::as_str)),
            venue: Some("Hugging Face Datasets".to_string()),
            abstract_text: item
                .get("description")
                .and_then(Value::as_str)
                .map(|text| clean_html_text(text))
                .filter(|text| !text.is_empty()),
            doi: None,
            source_url: Some(format!("https://huggingface.co/datasets/{id}")),
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "HF Datasets".to_string(),
            score: None,
            open_access: true,
        });
    }
    Ok(papers)
}

/// Stack Exchange Q&A (non-article records).
pub async fn search_stackexchange(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.stackexchange.com/2.3/search/advanced?order=desc&sort=relevance&q={}&site=stackoverflow&pagesize={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "stackexchange").await?;
    let mut papers = Vec::new();
    if let Some(items) = json.get("items").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(title) = item
                .get("title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let id = item
                .get("question_id")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            papers.push(Paper {
                id: format!("stackexchange:{id}"),
                title: clean_html_text(title),
                authors: item
                    .get("owner")
                    .and_then(|owner| owner.get("display_name"))
                    .and_then(Value::as_str)
                    .map(|name| vec![name.to_string()])
                    .unwrap_or_default(),
                year: item
                    .get("creation_date")
                    .and_then(Value::as_i64)
                    .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
                    .map(|date| chrono::Datelike::year(&date) as u32),
                venue: Some("Stack Overflow".to_string()),
                abstract_text: None,
                doi: None,
                source_url: item.get("link").and_then(Value::as_str).map(str::to_string),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "StackExchange".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// NCBI eutils is rate-limited (3 req/s anonymous); retry briefly on 429 so two
/// sequential lookups (esearch + esummary) do not fail under a parallel fan-out.
async fn eutils_json(client: &reqwest::Client, url: &str, label: &str) -> Result<Value, String> {
    for attempt in 0..3 {
        let response = client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| {
                format!(
                    "{}: request failed — {}",
                    label,
                    crate::sources::transport_reason(&error)
                )
            })?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tokio::time::sleep(std::time::Duration::from_millis(800 * (attempt + 1))).await;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("{}: HTTP {}", label, response.status()));
        }
        return response
            .json::<Value>()
            .await
            .map_err(|error| format!("{}: json parse failed: {}", label, error));
    }
    Err(format!("{}: HTTP 429 Too Many Requests", label))
}

async fn esearch_ids(
    client: &reqwest::Client,
    db: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db={}&term={}&retmode=json&retmax={}&tool=ScholarGateway&email=dqphong0302@gmail.com",
        db,
        urlencoding::encode(query),
        limit
    );
    let json = eutils_json(client, &url, db).await?;
    Ok(json
        .get("esearchresult")
        .and_then(|value| value.get("idlist"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

async fn esummary(
    client: &reqwest::Client,
    db: &str,
    ids: &[String],
    label: &str,
) -> Result<Value, String> {
    let url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db={}&id={}&retmode=json&tool=ScholarGateway&email=dqphong0302@gmail.com",
        db,
        ids.join(",")
    );
    eutils_json(client, &url, label).await
}

/// openFDA drug labels (regulatory records).
pub async fn search_openfda(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    // Drug-label records are ~180 KB each, so a large limit pushes the response
    // past the engine's HTTP timeout and the body read fails mid-stream.
    let fetch = limit.clamp(1, 10);
    let url = format!(
        "https://api.fda.gov/drug/label.json?search={}&limit={}",
        urlencoding::encode(query),
        fetch
    );
    let json = get_json(client, &url, "openfda").await?;
    let mut papers = Vec::new();
    if let Some(items) = json.get("results").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let brand = item
                .get("openfda")
                .and_then(|f| f.get("brand_name"))
                .and_then(Value::as_array)
                .and_then(|list| list.first())
                .and_then(Value::as_str);
            let generic = item
                .get("openfda")
                .and_then(|f| f.get("generic_name"))
                .and_then(Value::as_array)
                .and_then(|list| list.first())
                .and_then(Value::as_str);
            let title = brand
                .or(generic)
                .or_else(|| item.get("id").and_then(Value::as_str));
            let Some(title) = title else { continue };
            let id = item.get("id").and_then(Value::as_str).unwrap_or(title);
            papers.push(Paper {
                id: format!("openfda:{id}"),
                title: format!("Drug label: {}", clean_html_text(title)),
                authors: item
                    .get("openfda")
                    .and_then(|f| f.get("manufacturer_name"))
                    .and_then(Value::as_array)
                    .and_then(|list| list.first())
                    .and_then(Value::as_str)
                    .map(|name| vec![name.to_string()])
                    .unwrap_or_default(),
                year: year_from(item.get("effective_time").and_then(Value::as_str)),
                venue: Some("openFDA".to_string()),
                abstract_text: item
                    .get("indications_and_usage")
                    .and_then(Value::as_array)
                    .and_then(|list| list.first())
                    .and_then(Value::as_str)
                    .map(|text| clean_html_text(text))
                    .filter(|text| !text.is_empty()),
                doi: None,
                source_url: Some(format!("https://open.fda.gov/drug/label/{id}")),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "OpenFDA".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// UniProtKB protein records.
pub async fn search_uniprot(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://rest.uniprot.org/uniprotkb/search?query={}&format=json&size={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "uniprot").await?;
    let mut papers = Vec::new();
    if let Some(items) = json.get("results").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            let Some(accession) = item.get("primaryAccession").and_then(Value::as_str) else {
                continue;
            };
            let name = item
                .get("proteinDescription")
                .and_then(|d| d.get("recommendedName"))
                .and_then(|n| n.get("fullName"))
                .and_then(|n| n.get("value"))
                .and_then(Value::as_str)
                .unwrap_or(accession);
            papers.push(Paper {
                id: format!("uniprot:{accession}"),
                title: format!("{} ({})", name, accession),
                authors: Vec::new(),
                year: None,
                venue: item
                    .get("organism")
                    .and_then(|o| o.get("scientificName"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                abstract_text: None,
                doi: None,
                source_url: Some(format!("https://www.uniprot.org/uniprotkb/{accession}")),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "UniProt".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// NCBI GEO datasets (two-step esearch + esummary).
pub async fn search_geo(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let ids = esearch_ids(client, "gds", query, limit).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let summary = esummary(client, "gds", &ids, "geo").await?;
    let mut papers = Vec::new();
    for id in &ids {
        let Some(item) = summary.get("result").and_then(|r| r.get(id)) else {
            continue;
        };
        let Some(title) = item
            .get("title")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
        else {
            continue;
        };
        papers.push(Paper {
            id: format!("geo:{id}"),
            title: title.to_string(),
            authors: Vec::new(),
            year: year_from(item.get("pdat").and_then(Value::as_str)),
            venue: item
                .get("gdstype")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| Some("NCBI GEO".into())),
            abstract_text: item
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string),
            doi: None,
            source_url: Some(format!(
                "https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc={}",
                item.get("accession").and_then(Value::as_str).unwrap_or(id)
            )),
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "NCBI GEO".to_string(),
            score: None,
            open_access: true,
        });
    }
    Ok(papers)
}

/// ClinVar variant records (two-step esearch + esummary).
pub async fn search_clinvar(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let ids = esearch_ids(client, "clinvar", query, limit).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let summary = esummary(client, "clinvar", &ids, "clinvar").await?;
    let mut papers = Vec::new();
    for id in &ids {
        let Some(item) = summary.get("result").and_then(|r| r.get(id)) else {
            continue;
        };
        let Some(title) = item
            .get("title")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
        else {
            continue;
        };
        papers.push(Paper {
            id: format!("clinvar:{id}"),
            title: title.to_string(),
            authors: Vec::new(),
            year: None,
            venue: Some("ClinVar".to_string()),
            abstract_text: None,
            doi: None,
            source_url: Some(format!(
                "https://www.ncbi.nlm.nih.gov/clinvar/variation/{id}/"
            )),
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "ClinVar".to_string(),
            score: None,
            open_access: true,
        });
    }
    Ok(papers)
}

/// Figshare research outputs (datasets, figures, theses…).
pub async fn search_figshare(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://api.figshare.com/v2/articles?search_for={}&page_size={}",
        urlencoding::encode(query),
        limit
    );
    let json = get_json(client, &url, "figshare").await?;
    let mut papers = Vec::new();
    let empty = Vec::new();
    for item in json.as_array().unwrap_or(&empty).iter().take(limit) {
        let Some(title) = item
            .get("title")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
        else {
            continue;
        };
        let id = item.get("id").and_then(Value::as_u64).unwrap_or_default();
        papers.push(Paper {
            id: format!("figshare:{id}"),
            title: clean_html_text(title),
            authors: item
                .get("authors")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|a| a.get("full_name").and_then(Value::as_str))
                        .take(5)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            year: year_from(item.get("published_date").and_then(Value::as_str)),
            venue: Some("Figshare".to_string()),
            abstract_text: item
                .get("description")
                .and_then(Value::as_str)
                .map(clean_html_text)
                .filter(|t| !t.is_empty()),
            doi: item.get("doi").and_then(Value::as_str).map(str::to_string),
            source_url: item
                .get("url_public_html")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| Some(format!("https://figshare.com/articles/{id}"))),
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "Figshare".to_string(),
            score: None,
            open_access: true,
        });
    }
    Ok(papers)
}

/// SEC EDGAR full-text search (filings).
pub async fn search_sec_edgar(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://efts.sec.gov/LATEST/search-index?q={}&forms=10-K,10-Q,8-K",
        urlencoding::encode(query)
    );
    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "ScholarGateway-Desktop/1.0 (mailto:dqphong0302@gmail.com)",
        )
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| {
            format!(
                "sec_edgar: request failed — {}",
                crate::sources::transport_reason(&error)
            )
        })?;
    if !response.status().is_success() {
        return Err(format!("sec_edgar: HTTP {}", response.status()));
    }
    let json: Value = response
        .json()
        .await
        .map_err(|error| format!("sec_edgar: json parse failed: {}", error))?;
    let mut papers = Vec::new();
    if let Some(hits) = json
        .get("hits")
        .and_then(|value| value.get("hits"))
        .and_then(Value::as_array)
    {
        for hit in hits.iter().take(limit) {
            let source = hit.get("_source");
            let entity = source
                .and_then(|s| s.get("display_names"))
                .and_then(Value::as_array)
                .and_then(|list| list.first())
                .and_then(Value::as_str)
                .unwrap_or("SEC filing");
            let form = source
                .and_then(|s| s.get("form_type"))
                .and_then(Value::as_str)
                .unwrap_or("filing");
            let date = source
                .and_then(|s| s.get("file_date"))
                .and_then(Value::as_str);
            let id = hit.get("_id").and_then(Value::as_str).unwrap_or(entity);
            papers.push(Paper {
                id: format!("sec:{id}"),
                title: format!("{} — {}", entity, form),
                authors: Vec::new(),
                year: year_from(date),
                venue: Some("SEC EDGAR".to_string()),
                abstract_text: None,
                doi: None,
                source_url: Some(format!(
                    "https://efts.sec.gov/LATEST/search-index?q={}",
                    urlencoding::encode(query)
                )),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "SEC EDGAR".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

/// CISA Known Exploited Vulnerabilities feed (filtered client-side).
pub async fn search_cisa_kev(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<Paper>, String> {
    let url = "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";
    let json = get_json(client, url, "cisa_kev").await?;
    let needle = query.to_lowercase();
    let mut papers = Vec::new();
    if let Some(items) = json.get("vulnerabilities").and_then(Value::as_array) {
        for item in items {
            if papers.len() >= limit {
                break;
            }
            let haystack = format!(
                "{} {} {} {}",
                item.get("cveID").and_then(Value::as_str).unwrap_or(""),
                item.get("vendorProject")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                item.get("product").and_then(Value::as_str).unwrap_or(""),
                item.get("vulnerabilityName")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
            .to_lowercase();
            if !needle
                .split_whitespace()
                .any(|token| haystack.contains(token))
            {
                continue;
            }
            let Some(cve) = item.get("cveID").and_then(Value::as_str) else {
                continue;
            };
            let name = item
                .get("vulnerabilityName")
                .and_then(Value::as_str)
                .unwrap_or("Known exploited vulnerability");
            papers.push(Paper {
                id: format!("kev:{cve}"),
                title: format!("{cve}: {}", name),
                authors: item
                    .get("vendorProject")
                    .and_then(Value::as_str)
                    .map(|v| vec![v.to_string()])
                    .unwrap_or_default(),
                year: year_from(item.get("dateAdded").and_then(Value::as_str)),
                venue: Some("CISA KEV".to_string()),
                abstract_text: item
                    .get("shortDescription")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                doi: None,
                source_url: Some(format!("https://nvd.nist.gov/vuln/detail/{cve}")),
                pdf_url: None,
                citations: None,
                quartile: None,
                source: "CISA KEV".to_string(),
                score: None,
                open_access: true,
            });
        }
    }
    Ok(papers)
}

async fn get_json(client: &reqwest::Client, url: &str, label: &str) -> Result<Value, String> {
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| {
            format!(
                "{}: request failed — {}",
                label,
                crate::sources::transport_reason(&error)
            )
        })?;
    if !response.status().is_success() {
        return Err(format!("{}: HTTP {}", label, response.status()));
    }
    response
        .json::<Value>()
        .await
        .map_err(|error| format!("{}: json parse failed: {}", label, error))
}
