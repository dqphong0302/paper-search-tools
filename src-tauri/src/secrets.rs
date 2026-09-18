//! Secret storage in the OS keychain, with an in-process cache so the auth token
//! is not re-read on every request. Callers fall back to SQLite when the keychain
//! is disabled (`SCHOLARGATEWAY_KEYCHAIN=0`) or unavailable.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "scholargateway";

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn entry(key: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, key).ok()
}

/// Whether the keychain should be used at all.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| !matches!(std::env::var("SCHOLARGATEWAY_KEYCHAIN").as_deref(), Ok("0")))
}

pub fn read(key: &str) -> Option<String> {
    if !enabled() {
        return None;
    }
    if let Ok(cache) = cache().lock() {
        if let Some(cached) = cache.get(key) {
            return cached.clone();
        }
    }
    let value = entry(key).and_then(|entry| entry.get_password().ok());
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key.to_string(), value.clone());
    }
    value
}

/// Returns true when the value was stored in the keychain. On failure the caller
/// should keep the value in SQLite so the setting is never lost.
pub fn write(key: &str, value: &str) -> bool {
    if !enabled() {
        return false;
    }
    let stored = entry(key)
        .map(|entry| entry.set_password(value).is_ok())
        .unwrap_or(false);
    if stored {
        if let Ok(mut cache) = cache().lock() {
            cache.insert(key.to_string(), Some(value.to_string()));
        }
    }
    stored
}

pub fn delete(key: &str) {
    if !enabled() {
        return;
    }
    if let Some(entry) = entry(key) {
        let _ = entry.delete_credential();
    }
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key.to_string(), None);
    }
}
