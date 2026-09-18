use crate::models::Paper;
use crate::sources::clean_html_text;
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static ITEM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<a[^>]+href=["']((?:https://sti\.vista\.gov\.vn)?/publication/view/([^"']+?)-(\d+)\.html)["'][^>]*>([\s\S]*?)</a>"#).unwrap()
});

static YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"-(19\d\d|20\d\d)(?:-|$)"#).unwrap());

pub async fn search_nasati(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    year_min: Option<u32>,
    year_max: Option<u32>,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "https://sti.vista.gov.vn/publication.html?q={}",
        urlencoding::encode(query)
    );

    let res = client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) ScholarGateway/1.0",
        )
        .send()
        .await
        .map_err(|e| format!("vista_nasati: request failed — {}", crate::sources::transport_reason(&e)))?;

    if !res.status().is_success() {
        return Err(format!("vista_nasati: HTTP {}", res.status()));
    }

    let html = res
        .text()
        .await
        .map_err(|e| format!("vista_nasati: failed to read text: {}", e))?;

    let mut papers = Vec::new();
    let mut seen = HashSet::new();

    for cap in ITEM_RE.captures_iter(&html) {
        if papers.len() >= limit {
            break;
        }

        let href = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        let slug = cap.get(2).map(|m| m.as_str()).unwrap_or_default();
        let pub_id = cap.get(3).map(|m| m.as_str()).unwrap_or_default();
        let title_raw = cap.get(4).map(|m| m.as_str()).unwrap_or_default();
        let title = clean_html_text(title_raw);

        if title.len() < 5 || !seen.insert(pub_id.to_string()) {
            continue;
        }

        let year = YEAR_RE
            .captures(slug)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok());

        if let Some(y) = year {
            if let Some(ymin) = year_min {
                if y < ymin {
                    continue;
                }
            }
            if let Some(ymax) = year_max {
                if y > ymax {
                    continue;
                }
            }
        }

        let full_url = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("https://sti.vista.gov.vn{}", href)
        };

        papers.push(Paper {
            id: format!("vista_nasati:{}", pub_id),
            title,
            authors: Vec::new(),
            year,
            venue: Some(
                "National Agency for Science and Technology Information (NASATI)".to_string(),
            ),
            abstract_text: None,
            doi: None,
            source_url: Some(full_url),
            pdf_url: None,
            citations: None,
            quartile: None,
            source: "vista_nasati".to_string(),
            score: None,
            open_access: true,
        });
    }

    Ok(papers)
}
