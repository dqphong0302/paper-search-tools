//! Secret storage in the OS keychain, with an in-process cache so the auth token
//! is not re-read on every request. Callers fall back to SQLite when the keychain
//! is disabled (`SCHOLARGATE_KEYCHAIN=0`) or unavailable.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "scholargate";

/// The service name used before the app was renamed. Secrets saved by an older
/// install still live under it, so a read that misses the current service falls
/// back to this one and copies the value forward. Without that, renaming the
/// app would silently empty every saved API key and session.
const LEGACY_SERVICE: &str = "scholargateway";

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn entry(key: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, key).ok()
}

/// Reads a secret left behind by the pre-rename service and re-saves it under
/// the current one, so the migration happens once per key, on first use.
fn adopt_legacy(key: &str) -> Option<String> {
    let value = keyring::Entry::new(LEGACY_SERVICE, key)
        .ok()?
        .get_password()
        .ok()?;
    if let Some(entry) = entry(key) {
        let _ = entry.set_password(&value);
    }
    Some(value)
}

/// Whether the keychain should be used at all.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        // The old variable name keeps working for anyone who set it.
        let disabled = |name: &str| matches!(std::env::var(name).as_deref(), Ok("0"));
        !(disabled("SCHOLARGATE_KEYCHAIN") || disabled("SCHOLARGATEWAY_KEYCHAIN"))
    })
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
    let value = entry(key)
        .and_then(|entry| entry.get_password().ok())
        .or_else(|| adopt_legacy(key));
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
    // Clear the pre-rename copy too, or the next read would adopt it back.
    if let Ok(entry) = keyring::Entry::new(LEGACY_SERVICE, key) {
        let _ = entry.delete_credential();
    }
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key.to_string(), None);
    }
}
