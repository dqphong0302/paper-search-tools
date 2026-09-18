use regex::{Captures, Regex};
use serde::Serialize;
use std::sync::OnceLock;

pub const ADAPTER_VERSION: &str = "query-adapter-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMode {
    PubmedMesh,
    EuropePmc,
    Arxiv,
    NativeBoolean,
    PlainKeywords,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryPreview {
    pub id: String,
    pub mode: QueryMode,
    pub query: String,
    pub notes: &'static str,
}

fn pubmed_tag() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)(\"[^\"]+\"|[^\s()]+)\s*\[(mesh terms|mh|majr|tiab|ti|tw|all fields|au|ad|pt)(:noexp)?\]"#)
            .expect("valid PubMed field-tag regex")
    })
}

fn loose_pubmed_tag() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s*\[(?:mesh terms|mh|majr|tiab|ti|tw|all fields|au|ad|pt)(?::noexp)?\]")
            .expect("valid PubMed tag regex")
    })
}

fn normalize_space(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_pubmed_tags(query: &str) -> String {
    normalize_space(&loose_pubmed_tag().replace_all(query, ""))
}

fn europe_pmc(query: &str) -> String {
    let translated = pubmed_tag().replace_all(query, |caps: &Captures<'_>| {
        let term = caps.get(1).map_or("", |m| m.as_str());
        match caps
            .get(2)
            .map_or("", |m| m.as_str())
            .to_ascii_lowercase()
            .as_str()
        {
            "mesh terms" | "mh" | "majr" => format!("MESH:{term}"),
            "tiab" | "tw" => format!("(TITLE:{term} OR ABSTRACT:{term})"),
            "ti" => format!("TITLE:{term}"),
            "au" => format!("AUTH:{term}"),
            _ => term.to_string(),
        }
    });
    normalize_space(&loose_pubmed_tag().replace_all(&translated, ""))
}

fn arxiv(query: &str) -> String {
    let translated = pubmed_tag().replace_all(query, |caps: &Captures<'_>| {
        let term = caps.get(1).map_or("", |m| m.as_str());
        match caps
            .get(2)
            .map_or("", |m| m.as_str())
            .to_ascii_lowercase()
            .as_str()
        {
            "ti" => format!("ti:{term}"),
            "tiab" | "tw" => format!("(ti:{term} OR abs:{term})"),
            "au" => format!("au:{term}"),
            _ => format!("all:{term}"),
        }
    });
    let stripped = loose_pubmed_tag().replace_all(&translated, "");
    let not_re = Regex::new(r"(?i)\bNOT\b").expect("valid NOT regex");
    normalize_space(&not_re.replace_all(&stripped, "ANDNOT"))
}

fn lex(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut bracketed = false;
    for ch in query.chars() {
        match ch {
            '"' if !bracketed => {
                quoted = !quoted;
                current.push(ch);
            }
            '[' if !quoted => {
                bracketed = true;
                current.push(ch);
            }
            ']' if !quoted => {
                bracketed = false;
                current.push(ch);
            }
            '(' | ')' if !quoted && !bracketed => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
                tokens.push(ch.to_string());
            }
            c if c.is_whitespace() && !quoted && !bracketed => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

/// Relevance-style endpoints such as Semantic Scholar `/paper/search` accept
/// plain text only. Keep positive concepts and phrases, and omit NOT branches.
fn plain_keywords(query: &str) -> String {
    let tokens = lex(query);
    let mut out = Vec::new();
    let mut skip_next = false;
    let mut skip_depth = 0usize;
    for token in tokens {
        let upper = token.to_ascii_uppercase();
        if skip_depth > 0 {
            if token == "(" {
                skip_depth += 1;
            }
            if token == ")" {
                skip_depth -= 1;
            }
            continue;
        }
        if skip_next {
            if token == "(" {
                skip_depth = 1;
            }
            skip_next = false;
            continue;
        }
        if upper == "NOT" || upper == "ANDNOT" {
            skip_next = true;
            continue;
        }
        if token.starts_with('-') {
            continue;
        }
        if matches!(upper.as_str(), "AND" | "OR") || token == "(" || token == ")" {
            continue;
        }
        let clean = strip_pubmed_tags(&token);
        if !clean.is_empty() {
            out.push(clean);
        }
    }
    normalize_space(&out.join(" "))
}

pub fn mode(source: &str) -> QueryMode {
    match source {
        "pubmed" | "pmc" => QueryMode::PubmedMesh,
        "europe_pmc" | "metasearch" => QueryMode::EuropePmc,
        "arxiv" => QueryMode::Arxiv,
        // These endpoints document Boolean/query-string syntax. PubMed field
        // tags are still removed because they are not portable.
        "openalex" | "zenodo" | "clinicaltrials" | "scopus" | "hal" | "plos" | "inspire_hep"
        | "openaire" => QueryMode::NativeBoolean,
        _ => QueryMode::PlainKeywords,
    }
}

pub fn adapt(source: &str, query: &str) -> String {
    let trimmed = query.trim();
    let adapted = match mode(source) {
        QueryMode::PubmedMesh => trimmed.to_string(),
        QueryMode::EuropePmc => europe_pmc(trimmed),
        QueryMode::Arxiv => arxiv(trimmed),
        QueryMode::NativeBoolean => strip_pubmed_tags(trimmed),
        QueryMode::PlainKeywords => plain_keywords(trimmed),
    };
    if adapted.trim().is_empty() {
        strip_pubmed_tags(trimmed)
    } else {
        adapted
    }
}

pub fn preview(source: &str, query: &str) -> QueryPreview {
    let mode = mode(source);
    let notes = match mode {
        QueryMode::PubmedMesh => "MeSH, field tags and Boolean are sent natively",
        QueryMode::EuropePmc => "MeSH and PubMed fields are translated to Europe PMC syntax",
        QueryMode::Arxiv => "Fields and NOT are translated to arXiv syntax",
        QueryMode::NativeBoolean => "Boolean is preserved; PubMed-only field tags are removed",
        QueryMode::PlainKeywords => {
            "Positive concepts are sent as plain keywords; exclusions are omitted"
        }
    };
    QueryPreview {
        id: source.to_string(),
        mode,
        query: adapt(source, query),
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPLEX: &str =
        r#"("Heart Failure"[MeSH Terms] OR cardiomyopathy[tiab]) AND therapy[ti] NOT animals[mh]"#;

    #[test]
    fn pubmed_preserves_mesh_boolean_exactly() {
        assert_eq!(adapt("pubmed", COMPLEX), COMPLEX);
        assert_eq!(adapt("pmc", COMPLEX), COMPLEX);
    }

    #[test]
    fn europe_pmc_translates_mesh_and_fields() {
        assert_eq!(
            adapt("europe_pmc", COMPLEX),
            r#"(MESH:"Heart Failure" OR (TITLE:cardiomyopathy OR ABSTRACT:cardiomyopathy)) AND TITLE:therapy NOT MESH:animals"#
        );
    }

    #[test]
    fn arxiv_translates_fields_and_not() {
        let value = adapt("arxiv", COMPLEX);
        assert!(value.contains(r#"all:"Heart Failure""#));
        assert!(value.contains("(ti:cardiomyopathy OR abs:cardiomyopathy)"));
        assert!(value.contains("ANDNOT all:animals"));
    }

    #[test]
    fn native_boolean_strips_pubmed_tags() {
        assert_eq!(
            adapt("openalex", COMPLEX),
            r#"("Heart Failure" OR cardiomyopathy) AND therapy NOT animals"#
        );
    }

    #[test]
    fn plain_query_keeps_positive_terms_and_drops_exclusion() {
        assert_eq!(
            adapt("semantic_scholar", COMPLEX),
            r#""Heart Failure" cardiomyopathy therapy"#
        );
    }

    #[test]
    fn malformed_or_exclusion_only_queries_have_a_safe_fallback() {
        assert_eq!(adapt("semantic_scholar", "NOT"), "NOT");
        assert_eq!(adapt("crossref", r#""heart failure"#), r#""heart failure"#);
    }
}
