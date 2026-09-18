use serde_json::Value;
use std::collections::BTreeMap;

pub const CONFIG_KEYS: &[&str] = &[
    "domain_preset",
    "topic_setup_completed",
    "enabled_sources",
    "openai_api_key",
    "openai_base_url",
    "anthropic_api_key",
    "gemini_api_key",
    "deepseek_api_key",
    "groq_api_key",
    "ollama_base_url",
    "ollama_model",
    "searxng_enabled",
    "searxng_url",
    "searxng_categories",
    "searxng_engines",
    "ncbi_api_key",
    "ncbi_email",
    "openalex_email",
    "openalex_api_key",
    "semantic_scholar_api_key",
    "crossref_email",
    "unpaywall_email",
    "scopus_api_key",
    "ieee_api_key",
    "springer_api_key",
    "perplexity_api_key",
    "core_api_key",
    "dimensions_api_key",
    "wos_api_key",
    "consensus_session",
    "openevidence_session",
    "openai_session",
    "anthropic_session",
    "gemini_session",
    "deepseek_session",
    "perplexity_session",
    "web_search_enabled",
    "web_search_url",
    "mcp_auth_token",
    "gateway_port",
    "cache_ttl_hours",
    "max_results_default",
    "search_timeout_seconds",
    "search_delay_ms",
    "rate_limit_per_minute",
    "proxy_enabled",
    "proxy_url",
    "download_directory",
];

/// Credential/session fields that must never be returned by the config APIs.
/// `mcp_auth_token` is included because it also guards the whole gateway.
pub const SECRET_KEYS: &[&str] = &[
    "openai_api_key",
    "anthropic_api_key",
    "gemini_api_key",
    "deepseek_api_key",
    "groq_api_key",
    "ncbi_api_key",
    "openalex_api_key",
    "semantic_scholar_api_key",
    "scopus_api_key",
    "ieee_api_key",
    "springer_api_key",
    "perplexity_api_key",
    "core_api_key",
    "dimensions_api_key",
    "wos_api_key",
    "consensus_session",
    "openevidence_session",
    // Provider-console sessions captured by the in-app sign-in window.
    "openai_session",
    "anthropic_session",
    "gemini_session",
    "deepseek_session",
    "perplexity_session",
    "mcp_auth_token",
    // Proxy URLs commonly contain user:password credentials.
    "proxy_url",
];

/// Placeholder the UI sends back for an unchanged secret. Writing it is a no-op,
/// so blurting the full settings form can never erase a stored credential.
pub const KEEP_SECRET: &str = "__SG_KEEP__";

pub fn is_secret(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

fn canonical_domain_preset(value: &str) -> &str {
    match value {
        "vietnam_medical"
        | "vietnam_academic"
        | "vietnam_science"
        | "vietnam_engineering"
        | "vietnam_agriculture"
        | "vietnam_economics"
        | "vietnam_education"
        | "vietnam_social" => "vietnam",
        "biomedical_full"
        | "clinical_trials"
        | "pharma_clinical"
        | "pharma_deep"
        | "bioinformatics"
        | "global_health"
        | "fulltext_biomedical"
        | "medical" => "biomedical",
        "cs_ai" | "ml_ai_deep" | "nlp_llm" | "nlp_acl" | "cv_vision" | "code_knowledge"
        | "cyber_security" | "datasets" => "ai_cs",
        "physics" | "chemistry" | "energy_materials" | "aerospace" | "agriculture"
        | "engineering" | "natural_sciences" => "stem_nature",
        "economics" | "education" | "humanities" | "books" | "theses" | "social_sciences"
        | "law" | "environment" => "social_humanities",
        "systematic_review" | "evidence_based" => "evidence_review",
        "patents" | "us_gov" | "funding" => "patents_gov",
        "asia_pacific" | "global_south" | "africa" | "latin_america" => "global_regional",
        other => other,
    }
}

/// Replace every stored secret with a sentinel (configured) or empty string
/// (not configured) before the value leaves the process.
pub fn sanitize_config(value: Value) -> Value {
    let Value::Object(object) = value else {
        return value;
    };
    let mut map = serde_json::Map::new();
    for (key, value) in object {
        if key == "domain_preset" {
            if let Some(preset) = value.as_str() {
                map.insert(
                    key,
                    Value::String(canonical_domain_preset(preset).to_string()),
                );
                continue;
            }
        }
        if is_secret(&key) {
            let configured = value.as_str().is_some_and(|text| !text.is_empty());
            map.insert(
                key,
                Value::String(if configured {
                    KEEP_SECRET.into()
                } else {
                    String::new()
                }),
            );
        } else {
            map.insert(key, value);
        }
    }
    Value::Object(map)
}

/// Validate the complete patch before any writes, preserving existing settings on error.
pub fn validate_patch(payload: Value) -> Result<BTreeMap<String, String>, String> {
    let object = payload
        .as_object()
        .ok_or("Configuration must be a JSON object")?;
    if object.len() > 64 {
        return Err("Too many configuration fields".into());
    }
    let mut result = BTreeMap::new();
    for (key, value) in object {
        if key.is_empty()
            || key.len() > 80
            || !key
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        {
            return Err("Invalid configuration field name".into());
        }
        if !CONFIG_KEYS.contains(&key.as_str()) {
            return Err(format!("Unknown configuration field: {key}"));
        }
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Bool(_) | Value::Number(_) => value.to_string(),
            _ => return Err(format!("{key}: expected string, number or boolean")),
        };
        if text.len() > 8192 || text.contains('\0') {
            return Err(format!("{key}: value too long or invalid"));
        }
        // An untouched secret field keeps the stored credential; only an explicit
        // new value (or empty string to clear) is written.
        if is_secret(&key) && text == KEEP_SECRET {
            continue;
        }
        match key.as_str() {
            "mcp_auth_token" => {
                if !text.is_empty()
                    && (text.len() < 16
                        || text.len() > 256
                        || !text.bytes().all(|b| b.is_ascii_graphic()))
                {
                    return Err(
                        "mcp_auth_token: use 16–256 printable characters, or empty to disable"
                            .into(),
                    );
                }
            }
            "gateway_port" => {
                if !text
                    .parse::<u16>()
                    .ok()
                    .is_some_and(|n| n >= 1024 && n != 1420)
                {
                    return Err(
                        "gateway_port: expected port 1024–65535, excluding development port 1420"
                            .into(),
                    );
                }
            }
            "rate_limit_per_minute" => {
                if !text.parse::<u32>().ok().is_some_and(|n| n <= 100_000) {
                    return Err(
                        "rate_limit_per_minute: expected integer between 0 and 100000".into(),
                    );
                }
            }
            "search_delay_ms" => {
                if !text.parse::<u64>().ok().is_some_and(|n| n <= 60_000) {
                    return Err("search_delay_ms: expected integer between 0 and 60000".into());
                }
            }
            "cache_ttl_hours" | "max_results_default" | "search_timeout_seconds" => {
                let maximum = match key.as_str() {
                    "cache_ttl_hours" => 720,
                    "search_timeout_seconds" => 120,
                    _ => 50,
                };
                if !text
                    .parse::<u32>()
                    .ok()
                    .is_some_and(|n| (1..=maximum).contains(&n))
                {
                    return Err(format!("{key}: expected integer between 1 and {maximum}"));
                }
            }
            "domain_preset" => {
                let canonical = canonical_domain_preset(&text);
                if !crate::catalog::catalog()
                    .presets
                    .iter()
                    .any(|preset| preset.id == canonical)
                {
                    return Err("domain_preset: unknown discipline".into());
                }
                result.insert(key.clone(), canonical.to_string());
                continue;
            }
            "enabled_sources" => {
                let mut sources = Vec::new();
                for source in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let source = source.to_lowercase();
                    let source = source.as_str();
                    match crate::catalog::catalog()
                        .sources
                        .iter()
                        .find(|s| s.id == source)
                    {
                        None => return Err("enabled_sources: unknown source".into()),
                        Some(found) if !found.available => {
                            return Err(format!(
                                "enabled_sources: source '{}' is not supported",
                                source
                            ))
                        }
                        Some(_) => {}
                    }
                    if !sources.contains(&source.to_string()) {
                        sources.push(source.to_string());
                    }
                }
                result.insert(key.clone(), sources.join(","));
                continue;
            }
            "searxng_enabled"
            | "web_search_enabled"
            | "proxy_enabled"
            | "topic_setup_completed" => {
                if !["true", "false"].contains(&text.as_str()) {
                    return Err(format!("{key}: expected true or false"));
                }
            }
            "searxng_url"
                if result
                    .get("searxng_enabled")
                    .is_some_and(|value| value == "true")
                    && text.trim().is_empty() =>
            {
                return Err("searxng_url: required when external SearXNG is enabled".into());
            }
            "web_search_url"
                if result
                    .get("web_search_enabled")
                    .is_some_and(|value| value == "true")
                    && text.trim().is_empty() =>
            {
                return Err("web_search_url: required when web search is enabled".into());
            }
            "proxy_url"
                if result
                    .get("proxy_enabled")
                    .is_some_and(|value| value == "true")
                    && text.trim().is_empty() =>
            {
                return Err("proxy_url: required when the outbound proxy is enabled".into());
            }
            "searxng_categories" if !["science", "general"].contains(&text.as_str()) => {
                return Err("searxng_categories: expected science or general".into());
            }
            "download_directory" if !text.is_empty() => {
                crate::skills::directory(&text)?;
            }
            "openalex_email" | "crossref_email" | "ncbi_email" | "unpaywall_email"
                if !text.is_empty() =>
            {
                let trimmed = text.trim();
                let mut parts = trimmed.split('@');
                if trimmed.chars().any(char::is_whitespace)
                    || parts.next().is_none_or(str::is_empty)
                    || parts.next().is_none_or(|domain| {
                        domain.is_empty()
                            || !domain.contains('.')
                            || domain.starts_with('.')
                            || domain.ends_with('.')
                    })
                    || parts.next().is_some()
                {
                    return Err(format!("{key}: expected a valid email address"));
                }
            }
            "openai_base_url" | "ollama_base_url" | "searxng_url" | "web_search_url"
            | "proxy_url"
                if !text.is_empty() =>
            {
                let url = reqwest::Url::parse(&text).map_err(|_| format!("{key}: invalid URL"))?;
                if key == "web_search_url" && url.query().is_some() {
                    return Err("web_search_url: use base URL without query".into());
                }
                if !["http", "https"].contains(&url.scheme())
                    || url.host_str().is_none()
                    || (key != "proxy_url"
                        && (!url.username().is_empty() || url.password().is_some()))
                    || url.fragment().is_some()
                {
                    return Err(format!(
                        "{key}: use HTTP(S) without embedded credentials or fragment"
                    ));
                }
            }
            _ => {}
        }
        result.insert(key.clone(), text);
    }
    Ok(result)
}

pub fn search_timeout(db: &crate::db::Database) -> u64 {
    db.get_config("search_timeout_seconds")
        .and_then(|value| value.parse().ok())
        .unwrap_or(12)
        .clamp(1, 120)
}

pub fn search_delay(db: &crate::db::Database) -> std::time::Duration {
    let default = if cfg!(test) { 0 } else { 1_500 };
    std::time::Duration::from_millis(
        db.get_config("search_delay_ms")
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
            .min(60_000),
    )
}

pub fn outbound_proxy(db: &crate::db::Database) -> Option<String> {
    (db.get_config("proxy_enabled").as_deref() == Some("true"))
        .then(|| db.get_config("proxy_url"))
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn gateway_port(db: &crate::db::Database) -> u16 {
    db.get_config("gateway_port")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port >= 1024 && *port != 1420)
        .unwrap_or(8795)
}

pub fn download_directory(db: &crate::db::Database) -> Result<std::path::PathBuf, String> {
    if let Some(path) = db
        .get_config("download_directory")
        .filter(|value| !value.is_empty())
    {
        return crate::skills::directory(&path);
    }
    let home = std::path::PathBuf::from(
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| "Could not determine the user home directory")?,
    );
    // The app was called ScholarGateway before, and this default is not stored
    // in config — so renaming it would silently start writing to a second
    // folder while the user's existing PDFs sat in the old one. Keep using the
    // old folder when it is already there.
    let legacy = home.join("Documents/ScholarGateway/Papers");
    let path = if legacy.is_dir() {
        legacy
    } else {
        home.join("Documents/ScholarGate/Papers")
    };
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    crate::skills::directory(path.to_str().ok_or("Path must be valid UTF-8")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gateway_port_defaults_validates_and_persists() {
        let db = crate::db::Database::in_memory().unwrap();
        assert_eq!(gateway_port(&db), 8795);
        for port in [0, 80, 1420, 65536] {
            assert!(validate_patch(json!({"gateway_port":port})).is_err());
        }
        db.set_config_patch(&validate_patch(json!({"gateway_port":9876})).unwrap())
            .unwrap();
        assert_eq!(gateway_port(&db), 9876);
    }

    #[test]
    fn timeout_and_download_directory_follow_saved_settings() {
        let db = crate::db::Database::in_memory().unwrap();
        assert_eq!(search_timeout(&db), 12);
        assert_eq!(search_delay(&db), std::time::Duration::ZERO);
        assert_eq!(outbound_proxy(&db), None);
        let root = std::env::temp_dir().join(format!("sg-download-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let patch = validate_patch(
            json!({"download_directory":root.to_str().unwrap(),"search_timeout_seconds":30,"search_delay_ms":250,"proxy_enabled":true,"proxy_url":"http://user:pass@127.0.0.1:8080"}),
        )
        .unwrap();
        db.set_config_patch(&patch).unwrap();
        assert_eq!(search_timeout(&db), 30);
        assert_eq!(search_delay(&db), std::time::Duration::from_millis(250));
        assert_eq!(
            outbound_proxy(&db).as_deref(),
            Some("http://user:pass@127.0.0.1:8080")
        );
        assert_eq!(download_directory(&db).unwrap(), root);
        assert!(validate_patch(json!({"search_timeout_seconds":121})).is_err());
        assert!(validate_patch(json!({"download_directory":"relative/path"})).is_err());
        std::fs::remove_dir(&root).unwrap();
        assert!(download_directory(&db).is_err());
    }

    #[test]
    fn rejects_invalid_settings_and_preserves_string_credentials() {
        for value in [
            json!({"max_results_default":51}),
            json!({"cache_ttl_hours":0}),
            json!({"domain_preset":"missing"}),
            json!({"enabled_sources":"missing"}),
            json!({"searxng_enabled":"yes"}),
            json!({"topic_setup_completed":"yes"}),
            json!({"openai_base_url":"file:///tmp"}),
            json!({"ncbi_api_key":null}),
            json!({"rate_limit_per_minute":100001}),
            json!({"search_delay_ms":60001}),
            json!({"proxy_enabled":true,"proxy_url":""}),
            json!({"proxy_url":"socks5://127.0.0.1:1080"}),
            json!({"openalex_email":"not-an-email"}),
        ] {
            assert!(validate_patch(value).is_err());
        }
        assert!(validate_patch(json!({"rate_limit_per_minute":0})).is_ok());
        assert!(validate_patch(json!({"search_delay_ms":60000})).is_ok());
        assert!(validate_patch(
            json!({"proxy_enabled":true,"proxy_url":"http://user:pass@127.0.0.1:8080"})
        )
        .is_ok());
        assert!(validate_patch(json!({"topic_setup_completed":true})).is_ok());
        assert!(validate_patch(json!({"web_search_enabled":true,"web_search_url":""})).is_err());
        assert!(validate_patch(json!({"searxng_enabled":true,"searxng_url":""})).is_err());
        let patch = validate_patch(json!({"ncbi_api_key":"001234","domain_preset":"economics","enabled_sources":"OpenAlex, vjol,openalex","max_results_default":50})).unwrap();
        assert_eq!(patch["ncbi_api_key"], "001234");
        assert_eq!(patch["enabled_sources"], "openalex,vjol");
        let db = crate::db::Database::in_memory().unwrap();
        db.set_config_patch(&patch).unwrap();
        let loaded = db.get_all_config();
        assert_eq!(loaded["ncbi_api_key"], "001234");
        assert_eq!(loaded["max_results_default"], "50");
        assert_eq!(loaded["domain_preset"], "social_humanities");
        assert!(validate_patch(json!({"made_up_setting":"value"})).is_err());
    }

    #[test]
    fn secrets_are_write_only_and_keep_sentinel_is_ignored() {
        let db = crate::db::Database::in_memory().unwrap();
        db.set_config_patch(
            &validate_patch(json!({
                "openai_api_key":"sk-real",
                "proxy_url":"http://user:password@proxy.example:8080"
            }))
            .unwrap(),
        )
        .unwrap();
        let sanitized = sanitize_config(db.get_all_config());
        assert_eq!(sanitized["openai_api_key"], KEEP_SECRET);
        assert_eq!(sanitized["proxy_url"], KEEP_SECRET);
        // Echoing the sanitized value back must not overwrite the credential.
        db.set_config_patch(&validate_patch(sanitized).unwrap())
            .unwrap();
        assert_eq!(db.get_config("openai_api_key").as_deref(), Some("sk-real"));
        // An explicit empty string clears it.
        db.set_config_patch(&validate_patch(json!({"openai_api_key":""})).unwrap())
            .unwrap();
        assert_eq!(db.get_config("openai_api_key").as_deref(), Some(""));
        assert_eq!(sanitize_config(db.get_all_config())["openai_api_key"], "");
    }

    #[test]
    fn legacy_settings_are_canonical_on_read_and_write() {
        let visible = sanitize_config(json!({"domain_preset":"economics"}));
        assert_eq!(visible["domain_preset"], "social_humanities");

        let patch = validate_patch(json!({
            "domain_preset":"economics",
            "enabled_sources":"OpenAlex,vjol,searxng"
        }))
        .unwrap();
        assert_eq!(patch["domain_preset"], "social_humanities");
        assert_eq!(patch["enabled_sources"], "openalex,vjol,searxng");
    }
}
