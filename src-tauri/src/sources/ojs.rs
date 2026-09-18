use crate::models::Paper;
use crate::sources::{clean_html_text, parse_author_list};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static SUMMARY_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:div|li|article)[^>]*class=["'][^"']*(?:obj_article_summary|article-summary)[^"']*["'][^>]*>"#).unwrap()
});

static HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)href=["']((?:https?://[^"']+)?/[^"']*?/article/view/(\d+)(?:/(\d+))?[^"']*)["']"#,
    )
    .unwrap()
});

static TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:h[2-4]|div)[^>]*class=["'][^"']*\btitle\b[^"']*["'][^>]*>([\s\S]*?)</(?:h[2-4]|div)>"#).unwrap()
});

static AUTHORS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<div[^>]*class=["'][^"']*\bauthors\b[^"']*["'][^>]*>([\s\S]*?)</div>"#)
        .unwrap()
});

static PUBLISHED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)<div[^>]*class=["'][^"']*\b(?:published|date)\b[^"']*["'][^>]*>([\s\S]*?)</div>"#,
    )
    .unwrap()
});

static YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\b(19\d\d|20\d\d)\b"#).unwrap());

/// Many OJS themes render the title as the bare text of the article link with no
/// `class="title"` wrapper, so the class-based match above finds nothing.
static LINK_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)<a[^>]*href=["'][^"']*?/article/view/\d+(?:/\d+)?[^"']*["'][^>]*>([\s\S]*?)</a>"#,
    )
    .unwrap()
});

static PDF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)href=["']([^"']*/article/download/\d+/\d+[^"']*)["']"#).unwrap()
});

/// Upper bound for a single OJS search page when the caller has no tighter
/// budget. These portals render results server-side on shared hosting and
/// regularly take 20-30s, so they need far longer than the JSON APIs — but the
/// engine caps this against the time left in the whole search, so one slow
/// portal can no longer set the latency of every query.
pub const OJS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);

#[derive(Clone, Copy)]
pub struct OjsSiteConfig {
    pub source_id: &'static str,
    pub base_url: &'static str,
    /// One entry per OJS search endpoint. Sites without a site-wide index
    /// (e.g. VNU-JS) list each journal separately and the results are merged.
    pub search_paths: &'static [&'static str],
    pub default_venue: &'static str,
}

pub const VJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "vjol",
    base_url: "https://vjol.info.vn",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Vietnam Journals Online (VJOL)",
};

pub const VAST_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "vast",
    base_url: "https://vjs.ac.vn",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Vietnam Academy of Science & Technology (VAST)",
};

pub const JST_HUST_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "jst_hust",
    base_url: "https://jst.vn",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Journal of Science and Technology HUST",
};

// js.vnu.edu.vn does not expose a site-wide OJS index (that path returns 404),
// so each journal is searched on its own endpoint and the hits are merged.
pub const VNU_JS_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "vnu_js",
    base_url: "https://js.vnu.edu.vn",
    search_paths: &[
        "/index.php/SSH/search/search",
        "/index.php/NST/search/search",
        "/index.php/EES/search/search",
        "/index.php/ER/search/search",
        "/index.php/EAB/search/search",
        "/index.php/FS/search/search",
        "/index.php/LS/search/search",
        "/index.php/MaP/search/search",
        "/index.php/MPS/search/search",
        "/index.php/PaM/search/search",
    ],
    default_venue: "VNU Journal of Science",
};

pub const MEDPHARMRES_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "medpharmres",
    base_url: "https://medpharmres.vn",
    search_paths: &["/index.php/medpharmres/search/search"],
    default_venue: "Medical and Pharmaceutical Research (UMP)",
};

pub const YHOC_TPHCM_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "tapchi_yhoc_tphcm",
    base_url: "https://yhoctphcm.ump.edu.vn",
    search_paths: &["/index.php/yhoctphcm/search/search"],
    default_venue: "Ho Chi Minh City Journal of Medicine",
};

pub const NGHIENCUU_YHOC_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "tapchi_nghiencuuyhoc",
    base_url: "https://tapchinghiencuuyhoc.vn",
    search_paths: &["/index.php/tcncyh/search/search"],
    default_venue: "Journal of Medical Research (Hanoi Medical University)",
};

// INASP regional journals
pub const BANGLAJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "banglajol",
    base_url: "https://www.banglajol.info",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Bangladesh Journals Online (BanglaJOL)",
};

pub const NEPJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "nepjol",
    base_url: "https://www.nepjol.info",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Nepal Journals Online (NepJOL)",
};

pub const PHILJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "philjol",
    base_url: "https://philjol.info",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Philippine Journals Online (PhilJOL)",
};

pub const MONGOLIAJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "mongoliajol",
    base_url: "https://www.mongoliajol.info",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Mongolia Journals Online (MongoliaJOL)",
};

pub const SLJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "sljol",
    base_url: "https://sljol.info",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Sri Lanka Journals Online (SLJOL)",
};

pub const LAMJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "lamjol",
    base_url: "https://lamjol.info",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Latin America Journals Online (LAMJOL)",
};

pub const AJOL_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "ajol",
    base_url: "https://www.ajol.info",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "African Journals Online (AJOL)",
};

pub const THAIJO_CONFIG: OjsSiteConfig = OjsSiteConfig {
    source_id: "thaijo",
    base_url: "https://so01.tci-thaijo.org",
    search_paths: &["/index.php/index/search/search"],
    default_venue: "Thai Journals Online (ThaiJO)",
};

/// Searches every endpoint the site exposes and merges the hits. The call only
/// fails when *no* endpoint answered, so one dead journal cannot blank a site
/// that still has working ones.
#[allow(clippy::too_many_arguments)]
pub async fn search_ojs(
    client: &reqwest::Client,
    config: OjsSiteConfig,
    query: &str,
    limit: usize,
    year_min: Option<u32>,
    year_max: Option<u32>,
    timeout: std::time::Duration,
) -> Result<Vec<Paper>, String> {
    let timeout = timeout.min(OJS_TIMEOUT);
    let requests = config.search_paths.iter().map(|path| {
        search_ojs_endpoint(
            client, config, path, query, limit, year_min, year_max, timeout,
        )
    });
    let outcomes = futures::future::join_all(requests).await;

    let mut papers = Vec::new();
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for outcome in outcomes {
        match outcome {
            Ok(found) => {
                for paper in found {
                    if papers.len() >= limit {
                        break;
                    }
                    if seen.insert(paper.id.clone()) {
                        papers.push(paper);
                    }
                }
            }
            Err(error) => errors.push(error),
        }
    }

    if papers.is_empty() && !errors.is_empty() {
        return Err(errors.remove(0));
    }
    Ok(papers)
}

#[allow(clippy::too_many_arguments)]
async fn search_ojs_endpoint(
    client: &reqwest::Client,
    config: OjsSiteConfig,
    search_path: &str,
    query: &str,
    limit: usize,
    year_min: Option<u32>,
    year_max: Option<u32>,
    timeout: std::time::Duration,
) -> Result<Vec<Paper>, String> {
    let url = format!(
        "{}{}?query={}",
        config.base_url,
        search_path,
        urlencoding::encode(query)
    );

    let res = client
        .get(&url)
        // Overrides the client's JSON-API timeout, which used to cut these
        // portals off before they ever replied and made every one of them look
        // permanently dead.
        .timeout(timeout)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) ScholarGateway/1.0",
        )
        .send()
        .await
        .map_err(|e| format!("{}: request failed — {}", config.source_id, crate::sources::transport_reason(&e)))?;

    if !res.status().is_success() {
        return Err(format!("{}: HTTP {}", config.source_id, res.status()));
    }

    let html = res
        .text()
        .await
        .map_err(|e| format!("{}: failed to read text: {}", config.source_id, e))?;

    Ok(parse_ojs_results(&html, config, limit, year_min, year_max))
}

/// Pulls article rows out of an OJS search-results page.
pub fn parse_ojs_results(
    html: &str,
    config: OjsSiteConfig,
    limit: usize,
    year_min: Option<u32>,
    year_max: Option<u32>,
) -> Vec<Paper> {
    let blocks: Vec<&str> = SUMMARY_SPLIT_RE.split(html).skip(1).collect();
    let mut papers = Vec::new();
    let mut seen = HashSet::new();

    for block in blocks {
        if papers.len() >= limit {
            break;
        }

        let mut landing = None;
        let mut art_id = None;
        let mut galley = None;

        for cap in HREF_RE.captures_iter(block) {
            let href = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let id = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let galley_id = cap.get(3).map(|m| m.as_str());

            if galley_id.is_some() {
                if galley.is_none() {
                    galley = Some(href);
                }
                if art_id.is_none() {
                    art_id = Some(id);
                }
            } else if landing.is_none() {
                landing = Some(href);
                art_id = Some(id);
            }
        }

        let Some(article_id) = art_id else { continue };
        if !seen.insert(article_id.clone()) {
            continue;
        }

        let make_abs = |path: &str| -> String {
            if path.starts_with("http") {
                path.to_string()
            } else if path.starts_with('/') {
                format!("{}{}", config.base_url, path)
            } else {
                format!("{}/{}", config.base_url, path)
            }
        };

        let landing_url = landing
            .map(|u| make_abs(&u))
            .or_else(|| galley.as_ref().map(|u| make_abs(u)));
        let Some(source_url) = landing_url else {
            continue;
        };

        // Title: prefer an element carrying a `title` class, then fall back to the
        // text of the article link itself.
        let title = TITLE_RE
            .captures(block)
            .and_then(|c| c.get(1))
            .map(|m| clean_html_text(m.as_str()))
            .filter(|text| text.chars().count() >= 4)
            .or_else(|| {
                LINK_TITLE_RE
                    .captures_iter(block)
                    .filter_map(|c| c.get(1))
                    .map(|m| clean_html_text(m.as_str()))
                    .find(|text| text.chars().count() >= 4)
            });
        let Some(title) = title else { continue };

        // Authors
        let authors = AUTHORS_RE
            .captures(block)
            .and_then(|c| c.get(1))
            .map(|m| parse_author_list(m.as_str()))
            .unwrap_or_default();

        // Year
        let year = PUBLISHED_RE
            .captures(block)
            .and_then(|c| c.get(1))
            .and_then(|m| YEAR_RE.captures(m.as_str()))
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

        // PDF URL
        let pdf_url = PDF_RE
            .captures(block)
            .and_then(|c| c.get(1))
            .map(|m| make_abs(m.as_str()))
            .or_else(|| {
                galley.map(|g| make_abs(&g.replace("/article/view/", "/article/download/")))
            });

        papers.push(Paper {
            id: format!("{}:{}", config.source_id, article_id),
            title,
            authors,
            year,
            venue: Some(config.default_venue.to_string()),
            abstract_text: None,
            doi: None,
            source_url: Some(source_url),
            pdf_url,
            citations: None,
            quartile: None,
            source: config.source_id.to_string(),
            score: None,
            open_access: true,
        });
    }

    papers
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK_TITLE_THEME: &str = r#"
        <div class="article-summary media">
          <div class="media-body">
            <div class="col-md-12 pl-0">
              <a href="https://jst.vn/index.php/old/article/view/491">
                Comparison of Deep Learning Models for Recognizing Spikes
              </a>
              <div class="contextName">
                <span>Old Journal,
                  <a class="title" href="https://jst.vn/index.php/old/issue/view/67">
                    Journal of Science and Technology 145 (2020)
                  </a>
                </span>
              </div>
              <div class="meta">
                <div class="authors ">Nguyen Van A, Tran Thi B</div>
                <div class="published">2020-05-01</div>
              </div>
            </div>
          </div>
        </div>
    "#;

    const CLASS_TITLE_THEME: &str = r#"
        <div class="obj_article_summary">
          <h3 class="title">
            <a href="/index.php/demo/article/view/77">Formative assessment in practice</a>
          </h3>
          <div class="authors">Le Van C</div>
          <div class="published">2023-02-11</div>
        </div>
    "#;

    fn config() -> OjsSiteConfig {
        OjsSiteConfig {
            source_id: "test_ojs",
            base_url: "https://example.org",
            search_paths: &["/index.php/index/search/search"],
            default_venue: "Test Journals Online",
        }
    }

    // Several OJS themes (jst.vn, tapchinghiencuuyhoc.vn) put the title in the
    // article link itself. Requiring a `class="title"` element dropped every row
    // and the source reported zero results on pages that clearly had hits.
    #[test]
    fn titles_are_read_from_the_article_link_when_no_title_class_exists() {
        let papers = parse_ojs_results(LINK_TITLE_THEME, config(), 10, None, None);
        assert_eq!(papers.len(), 1);
        assert_eq!(
            papers[0].title,
            "Comparison of Deep Learning Models for Recognizing Spikes"
        );
        assert_eq!(papers[0].year, Some(2020));
        assert_eq!(
            papers[0].source_url.as_deref(),
            Some("https://jst.vn/index.php/old/article/view/491")
        );
    }

    #[test]
    fn a_title_element_still_wins_over_the_link_text() {
        let papers = parse_ojs_results(CLASS_TITLE_THEME, config(), 10, None, None);
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].title, "Formative assessment in practice");
        assert_eq!(
            papers[0].source_url.as_deref(),
            Some("https://example.org/index.php/demo/article/view/77")
        );
    }

    #[test]
    fn year_filters_still_apply_to_link_titled_rows() {
        assert!(parse_ojs_results(LINK_TITLE_THEME, config(), 10, Some(2022), None).is_empty());
        assert_eq!(
            parse_ojs_results(LINK_TITLE_THEME, config(), 10, Some(2019), Some(2021)).len(),
            1
        );
    }

    // js.vnu.edu.vn has no site-wide index, so the source must fan out across the
    // per-journal endpoints instead of hitting a path that always returns 404.
    #[test]
    fn vnu_js_searches_per_journal_endpoints() {
        assert!(VNU_JS_CONFIG.search_paths.len() > 1);
        assert!(!VNU_JS_CONFIG
            .search_paths
            .contains(&"/index.php/index/search/search"));
        assert!(VNU_JS_CONFIG
            .search_paths
            .iter()
            .all(|path| path.ends_with("/search/search")));
    }
}
