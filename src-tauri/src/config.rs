use serde_json::Value;
use std::collections::BTreeMap;

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
];

/// Placeholder the UI sends back for an unchanged secret. Writing it is a no-op,
/// so blurting the full settings form can never erase a stored credential.
pub const KEEP_SECRET: &str = "__SG_KEEP__";

pub fn is_secret(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

/// Replace every stored secret with a sentinel (configured) or empty string
/// (not configured) before the value leaves the process.
pub fn sanitize_config(value: Value) -> Value {
    let Value::Object(object) = value else { return value };
    let mut map = serde_json::Map::new();
    for (key, value) in object {
        if is_secret(&key) {
            let configured = value.as_str().is_some_and(|text| !text.is_empty());
            map.insert(
                key,
                Value::String(if configured { KEEP_SECRET.into() } else { String::new() }),
            );
        } else {
            map.insert(key, value);
        }
    }
    Value::Object(map)
}

/// Validate the complete patch before any writes, preserving existing settings on error.
pub fn validate_patch(payload: Value) -> Result<BTreeMap<String, String>, String> {
    let object = payload.as_object().ok_or("Configuration must be a JSON object")?;
    if object.len() > 64 { return Err("Too many configuration fields".into()); }
    let mut result = BTreeMap::new();
    for (key, value) in object {
        if key.is_empty() || key.len() > 80 || !key.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_') {
            return Err("Invalid configuration field name".into());
        }
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Bool(_) | Value::Number(_) => value.to_string(),
            _ => return Err(format!("{key}: expected string, number or boolean")),
        };
        if text.len() > 8192 || text.contains('\0') { return Err(format!("{key}: value too long or invalid")); }
        // An untouched secret field keeps the stored credential; only an explicit
        // new value (or empty string to clear) is written.
        if is_secret(&key) && text == KEEP_SECRET { continue; }
        match key.as_str() {
            "mcp_auth_token" => {
                if !text.is_empty() && (text.len() < 16 || text.len() > 256 || !text.bytes().all(|b| b.is_ascii_graphic())) {
                    return Err("mcp_auth_token: use 16–256 printable characters, or empty to disable".into());
                }
            }
            "gateway_port" => {
                if !text.parse::<u16>().ok().is_some_and(|n| n >= 1024 && n != 1420) {
                    return Err("gateway_port: expected port 1024–65535, excluding development port 1420".into());
                }
            }
            "rate_limit_per_minute" => {
                if !text.parse::<u32>().ok().is_some_and(|n| n <= 100_000) {
                    return Err("rate_limit_per_minute: expected integer between 0 and 100000".into());
                }
            }
            "cache_ttl_hours" | "max_results_default" | "search_timeout_seconds" => {
                let maximum = match key.as_str() { "cache_ttl_hours" => 720, "search_timeout_seconds" => 120, _ => 50 };
                if !text.parse::<u32>().ok().is_some_and(|n| (1..=maximum).contains(&n)) {
                    return Err(format!("{key}: expected integer between 1 and {maximum}"));
                }
            }
            "domain_preset" => {
                if !crate::catalog::catalog().presets.iter().any(|preset| preset.id == text) {
                    return Err("domain_preset: unknown discipline".into());
                }
            }
            "enabled_sources" => {
                let mut sources = Vec::new();
                for source in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let source = source.to_lowercase();
                    let source = match source.as_str() { "vjol" => "vietnam", source => source };
                    match crate::catalog::catalog().sources.iter().find(|s| s.id == source) {
                        None => return Err("enabled_sources: unknown source".into()),
                        Some(found) if !found.available => {
                            return Err(format!("enabled_sources: source '{}' is not supported", source))
                        }
                        Some(_) => {}
                    }
                    if !sources.contains(&source.to_string()) { sources.push(source.to_string()); }
                }
                result.insert(key.clone(), sources.join(","));
                continue;
            }
            "searxng_enabled" | "web_search_enabled" => if !["true", "false"].contains(&text.as_str()) { return Err(format!("{key}: expected true or false")); },
            "download_directory" if !text.is_empty() => { crate::skills::directory(&text)?; }
            "openai_base_url" | "ollama_base_url" | "searxng_url" | "web_search_url" if !text.is_empty() => {
                let url = reqwest::Url::parse(&text).map_err(|_| format!("{key}: invalid URL"))?;
                if key == "web_search_url" && url.query().is_some() { return Err("web_search_url: use base URL without query".into()); }
                if !["http", "https"].contains(&url.scheme()) || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
                    return Err(format!("{key}: use HTTP(S) without embedded credentials or fragment"));
                }
            }
            _ => {}
        }
        result.insert(key.clone(), text);
    }
    Ok(result)
}

pub fn search_timeout(db: &crate::db::Database) -> u64 {
    db.get_config("search_timeout_seconds").and_then(|value| value.parse().ok()).unwrap_or(12).clamp(1, 120)
}

pub fn gateway_port(db: &crate::db::Database) -> u16 {
    db.get_config("gateway_port").and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port >= 1024 && *port != 1420).unwrap_or(8795)
}

pub fn download_directory(db: &crate::db::Database) -> Result<std::path::PathBuf, String> {
    if let Some(path) = db.get_config("download_directory").filter(|value| !value.is_empty()) {
        return crate::skills::directory(&path);
    }
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).map_err(|_| "Could not determine the user home directory")?;
    let path = std::path::PathBuf::from(home).join("Documents/ScholarGateway/Papers");
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
        db.set_config_patch(&validate_patch(json!({"gateway_port":9876})).unwrap()).unwrap();
        assert_eq!(gateway_port(&db), 9876);
    }

    #[test]
    fn timeout_and_download_directory_follow_saved_settings() {
        let db = crate::db::Database::in_memory().unwrap();
        assert_eq!(search_timeout(&db), 12);
        let root = std::env::temp_dir().join(format!("sg-download-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let patch = validate_patch(json!({"download_directory":root.to_str().unwrap(),"search_timeout_seconds":30})).unwrap();
        db.set_config_patch(&patch).unwrap();
        assert_eq!(search_timeout(&db), 30);
        assert_eq!(download_directory(&db).unwrap(), root);
        assert!(validate_patch(json!({"search_timeout_seconds":121})).is_err());
        assert!(validate_patch(json!({"download_directory":"relative/path"})).is_err());
        std::fs::remove_dir(&root).unwrap();
        assert!(download_directory(&db).is_err());
    }

    #[test]
    fn rejects_invalid_settings_and_preserves_string_credentials() {
        for value in [json!({"max_results_default":51}), json!({"cache_ttl_hours":0}), json!({"domain_preset":"missing"}), json!({"enabled_sources":"missing"}), json!({"searxng_enabled":"yes"}), json!({"openai_base_url":"file:///tmp"}), json!({"ncbi_api_key":null}), json!({"rate_limit_per_minute":100001})] {
            assert!(validate_patch(value).is_err());
        }
        assert!(validate_patch(json!({"rate_limit_per_minute":0})).is_ok());
        let patch = validate_patch(json!({"ncbi_api_key":"001234","domain_preset":"economics","enabled_sources":"OpenAlex, vjol,openalex","max_results_default":50})).unwrap();
        assert_eq!(patch["ncbi_api_key"], "001234");
        assert_eq!(patch["enabled_sources"], "openalex,vietnam");
        let db = crate::db::Database::in_memory().unwrap();
        db.set_config_patch(&patch).unwrap();
        let loaded = db.get_all_config();
        assert_eq!(loaded["ncbi_api_key"], "001234");
        assert_eq!(loaded["max_results_default"], "50");
        assert_eq!(loaded["domain_preset"], "economics");
    }

    #[test]
    fn secrets_are_write_only_and_keep_sentinel_is_ignored() {
        let db = crate::db::Database::in_memory().unwrap();
        db.set_config_patch(&validate_patch(json!({"openai_api_key":"sk-real"})).unwrap()).unwrap();
        let sanitized = sanitize_config(db.get_all_config());
        assert_eq!(sanitized["openai_api_key"], KEEP_SECRET);
        // Echoing the sanitized value back must not overwrite the credential.
        db.set_config_patch(&validate_patch(sanitized).unwrap()).unwrap();
        assert_eq!(db.get_config("openai_api_key").as_deref(), Some("sk-real"));
        // An explicit empty string clears it.
        db.set_config_patch(&validate_patch(json!({"openai_api_key":""})).unwrap()).unwrap();
        assert_eq!(db.get_config("openai_api_key").as_deref(), Some(""));
        assert_eq!(sanitize_config(db.get_all_config())["openai_api_key"], "");
    }
}
