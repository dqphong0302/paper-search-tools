//! Explicit local MCP client configuration edits. No server process is started here.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

static OPERATIONS: Mutex<()> = Mutex::new(());
/// Client configs are usually tiny, but Claude Code keeps per-project state in
/// the same file and it grows past a megabyte on an active machine. Refusing to
/// read it would mean refusing to install the server for exactly the people who
/// use the client most.
const MAX_CONFIG: u64 = 8 * 1024 * 1024;
/// A remote MCP server's initialize response has no reason to be large.
const MAX_PROBE: usize = 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
pub struct ManagedServer {
    pub definition: Value,
    pub enabled: bool,
}

#[derive(Default, Deserialize, Serialize)]
struct Receipt {
    entries: BTreeMap<String, ManagedServer>,
}

#[derive(Serialize)]
pub struct ConfigView {
    pub path: String,
    pub revision: String,
    pub servers: BTreeMap<String, Value>,
    pub managed: BTreeMap<String, ManagedServer>,
    pub backup: Option<String>,
}

fn config_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.extension().and_then(|s| s.to_str()) != Some("json") {
        return Err("Only .json files with an mcpServers object are supported, not TOML/JSONC".into());
    }
    let parent = crate::skills::directory(
        path.parent()
            .and_then(Path::to_str)
            .ok_or("An absolute path is required")?,
    )?;
    let target = parent.join(path.file_name().ok_or("Path has no file name")?);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("Refusing to replace a symlink. Choose the real config file".into());
        }
    }
    Ok(target)
}

fn sidecar(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        ".{}.scholargateway.json",
        path.file_name().unwrap().to_string_lossy()
    ))
}

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
        Ok(meta) => {
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_CONFIG {
                return Err("File must be a regular file of at most 8 MiB, not a symlink".into());
            }
            fs::read(path).map(Some).map_err(|error| error.to_string())
        }
    }
}

fn revision(document: &Option<Vec<u8>>, receipt: &Option<Vec<u8>>) -> String {
    format!(
        "{:x}",
        md5::compute(serde_json::to_vec(&(document, receipt)).unwrap())
    )
}

fn parse_document(bytes: &Option<Vec<u8>>) -> Result<Value, String> {
    let value: Value = match bytes {
        Some(bytes) => serde_json::from_slice(bytes).map_err(|e| e.to_string())?,
        None => json!({"mcpServers":{}}),
    };
    if !value.is_object()
        || value
            .get("mcpServers")
            .is_some_and(|servers| !servers.is_object())
    {
        return Err("Expected a JSON object whose mcpServers field is an object".into());
    }
    Ok(value)
}

fn load(path: &Path) -> Result<(Value, Receipt, String), String> {
    let bytes = read_bytes(path)?;
    let receipt_bytes = read_bytes(&sidecar(path))?;
    let document = parse_document(&bytes)?;
    let receipt: Receipt = match &receipt_bytes {
        Some(bytes) => serde_json::from_slice(bytes).map_err(|e| e.to_string())?,
        None => Receipt::default(),
    };
    for (name, entry) in &receipt.entries {
        let current = document
            .get("mcpServers")
            .and_then(|servers| servers.get(name));
        if (entry.enabled && current != Some(&entry.definition))
            || (!entry.enabled && current.is_some())
        {
            return Err(format!(
                "Entry {name} was edited outside the app. Not overwriting; check the config and receipt {}",
                sidecar(path).display()
            ));
        }
    }
    Ok((document, receipt, revision(&bytes, &receipt_bytes)))
}

fn view(path: &Path, backup: Option<String>) -> Result<ConfigView, String> {
    let (document, receipt, revision) = load(path)?;
    Ok(ConfigView {
        path: path.to_string_lossy().into(),
        revision,
        servers: serde_json::from_value(document.get("mcpServers").cloned().unwrap_or(json!({})))
            .map_err(|e| e.to_string())?,
        managed: receipt.entries,
        backup,
    })
}

fn validate_definition(value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("Server entry must be a JSON object")?;
    if object
        .keys()
        .any(|key| !["command", "args", "env", "url", "headers", "type"].contains(&key.as_str()))
    {
        return Err("Supported fields: command, args, env, url, headers, type".into());
    }
    if let Some(command) = value.get("command") {
        if value.get("url").is_some()
            || value.get("headers").is_some()
            || !command.as_str().is_some_and(|s| Path::new(s).is_absolute())
        {
            return Err("stdio requires an absolute command path and no url/headers".into());
        }
        if value.get("type").is_some_and(|kind| kind != "stdio") {
            return Err("type must be stdio".into());
        }
        if value.get("args").is_some_and(|args| {
            !args
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string))
        }) {
            return Err("args must be an array of strings".into());
        }
    } else {
        let url = reqwest::Url::parse(value["url"].as_str().ok_or("Either command or url is required")?)
            .map_err(|e| e.to_string())?;
        if !["http", "https"].contains(&url.scheme())
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err("URL must be HTTP(S) and contain no password or fragment".into());
        }
        if value.get("args").is_some() || value.get("env").is_some() {
            return Err("HTTP transports do not take args/env".into());
        }
        if value
            .get("type")
            .is_some_and(|kind| kind != "http" && kind != "sse")
        {
            return Err("type must be http or sse".into());
        }
    }
    for field in ["env", "headers"] {
        if value.get(field).is_some_and(|items| {
            !items
                .as_object()
                .is_some_and(|items| items.values().all(Value::is_string))
        }) {
            return Err(format!("{field} must map names to strings"));
        }
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_file_name(format!(".sg-config-{}.tmp", uuid::Uuid::new_v4()));
    write_private(&temp, bytes)?;
    fs::rename(&temp, path).map_err(|e| format!("{e}; the temporary file was kept at {}", temp.display()))
}

#[tauri::command]
pub fn read_mcp_config(path: String) -> Result<ConfigView, String> {
    let _guard = OPERATIONS.lock().map_err(|_| "The MCP manager is in a failed state")?;
    view(&config_path(&path)?, None)
}

#[tauri::command]
pub fn edit_mcp_config(
    path: String,
    expected_revision: String,
    name: String,
    action: String,
    definition: Option<Value>,
) -> Result<ConfigView, String> {
    let _guard = OPERATIONS.lock().map_err(|_| "The MCP manager is in a failed state")?;
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    {
        return Err("Server name may only contain letters, digits, - or _ (1-64 characters)".into());
    }
    let path = config_path(&path)?;
    let (mut document, mut receipt, current_revision) = load(&path)?;
    if expected_revision != current_revision {
        return Err("The config changed; reload it before saving".into());
    }
    let servers = document
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert(json!({}))
        .as_object_mut()
        .unwrap();
    match action.as_str() {
        "add" | "update" => {
            if action == "add"
                && (servers.contains_key(&name) || receipt.entries.contains_key(&name))
            {
                return Err("A server with this name already exists; not overwriting".into());
            }
            if action == "update" && !receipt.entries.contains_key(&name) {
                return Err("Only entries managed by this app can be edited".into());
            }
            let definition = definition.ok_or("Server definition is missing")?;
            validate_definition(&definition)?;
            let enabled = receipt
                .entries
                .get(&name)
                .map(|entry| entry.enabled)
                .unwrap_or(true);
            if enabled {
                servers.insert(name.clone(), definition.clone());
            }
            receipt.entries.insert(
                name,
                ManagedServer {
                    definition,
                    enabled,
                },
            );
        }
        "enable" | "disable" | "remove" => {
            let entry = receipt
                .entries
                .get_mut(&name)
                .ok_or("Only entries installed by this app are managed; pre-existing servers are left alone")?;
            entry.enabled = action == "enable";
            if entry.enabled {
                servers.insert(name.clone(), entry.definition.clone());
            } else {
                servers.remove(&name);
            }
            if action == "remove" {
                receipt.entries.remove(&name);
            }
        }
        _ => return Err("Unsupported operation".into()),
    }
    let old_document = read_bytes(&path)?;
    let old_receipt = read_bytes(&sidecar(&path))?;
    if revision(&old_document, &old_receipt) != expected_revision {
        return Err("The file changed while it was being processed; reload it".into());
    }
    let suffix = uuid::Uuid::new_v4();
    let backup = path.with_file_name(format!(
        "{}.{}.bak",
        path.file_name().unwrap().to_string_lossy(),
        suffix
    ));
    // Keep config and management receipt together so recovery preserves ownership.
    write_private(
        &backup,
        old_document.as_deref().unwrap_or(b"{\"mcpServers\":{}}"),
    )?;
    let receipt_backup = backup.with_file_name(format!(
        "{}.receipt",
        backup.file_name().unwrap().to_string_lossy()
    ));
    write_private(
        &receipt_backup,
        old_receipt.as_deref().unwrap_or(b"{\"entries\":{}}"),
    )?;
    replace(
        &path,
        &serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?,
    )?;
    if let Err(error) = replace(
        &sidecar(&path),
        &serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    ) {
        // On a partial write, restore the previous client document; leave backup for inspection.
        if let Some(bytes) = old_document {
            let _ = replace(&path, &bytes);
        }
        return Err(format!("{error}; backup: {}", backup.display()));
    }
    view(&path, Some(backup.to_string_lossy().into()))
}

#[tauri::command]
pub async fn test_mcp_http(definition: Value) -> Result<Value, String> {
    validate_definition(&definition)?;
    if definition.get("command").is_some() || definition.get("type") == Some(&json!("sse")) {
        return Err("Live testing currently supports Streamable HTTP JSON only. This app will not run a stdio command for you, nor fake a successful connection.".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client
        .post(definition["url"].as_str().unwrap())
        .header("Accept", "application/json, text/event-stream");
    if let Some(headers) = definition["headers"].as_object() {
        for (name, value) in headers {
            request = request.header(name, value.as_str().unwrap());
        }
    }
    let mut response = request.json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ScholarGateway-check","version":"1"}}}))
        .send().await.map_err(|error| error.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| error.without_url().to_string())?
    {
        if bytes.len() + chunk.len() > MAX_PROBE {
            return Err("The MCP response exceeds 1 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let data: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "The server did not return a valid MCP initialize JSON response")?;
    if data["id"] != 1
        || data["jsonrpc"] != "2.0"
        || !data["result"]["serverInfo"]["name"].is_string()
        || !data["result"]["protocolVersion"].is_string()
    {
        return Err("Initialize failed, or serverInfo/protocolVersion is missing".into());
    }
    Ok(
        json!({"serverInfo":data["result"]["serverInfo"],"protocolVersion":data["result"]["protocolVersion"],"message":"Initialize succeeded; no tool was called"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_existing_config_and_manages_only_owned_entries() {
        let root = std::env::temp_dir().join(format!("sg-mcp-config-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let path = root.join("client.json");
        fs::write(
            &path,
            br#"{"preferences":{"theme":"dark"},"mcpServers":{"existing":{"command":"keep-me"}}}"#,
        )
        .unwrap();
        let path = path.to_str().unwrap().to_string();
        let before = read_mcp_config(path.clone()).unwrap();
        let definition = json!({"url":"http://127.0.0.1:8795/mcp"});
        let added = edit_mcp_config(
            path.clone(),
            before.revision.clone(),
            "scholargateway".into(),
            "add".into(),
            Some(definition.clone()),
        )
        .unwrap();
        assert!(Path::new(added.backup.as_ref().unwrap()).exists());
        let backup_doc: Value =
            serde_json::from_slice(&fs::read(added.backup.as_ref().unwrap()).unwrap()).unwrap();
        assert_eq!(backup_doc["preferences"]["theme"], "dark");
        assert_eq!(added.servers["existing"]["command"], "keep-me");
        assert!(edit_mcp_config(
            path.clone(),
            before.revision,
            "x".into(),
            "add".into(),
            Some(definition.clone())
        )
        .is_err());
        assert!(edit_mcp_config(
            path.clone(),
            added.revision.clone(),
            "existing".into(),
            "remove".into(),
            None
        )
        .is_err());
        let disabled = edit_mcp_config(
            path.clone(),
            added.revision,
            "scholargateway".into(),
            "disable".into(),
            None,
        )
        .unwrap();
        assert!(!disabled.servers.contains_key("scholargateway"));
        let enabled = edit_mcp_config(
            path.clone(),
            disabled.revision,
            "scholargateway".into(),
            "enable".into(),
            None,
        )
        .unwrap();
        let removed = edit_mcp_config(
            path.clone(),
            enabled.revision,
            "scholargateway".into(),
            "remove".into(),
            None,
        )
        .unwrap();
        assert_eq!(removed.servers.len(), 1);
        let final_doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(final_doc["preferences"]["theme"], "dark");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&path, root.join("link.json")).unwrap();
            assert!(read_mcp_config(root.join("link.json").to_str().unwrap().into()).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_ambiguous_or_unsafe_server_definitions() {
        for definition in [
            json!({"command":"node"}),
            json!({"url":"file:///tmp/server"}),
            json!({"url":"https://user:password@example.com"}),
            json!({"command":"/usr/bin/node","url":"http://localhost"}),
            json!({"url":"https://example.com","headers":{"key":1}}),
        ] {
            assert!(validate_definition(&definition).is_err());
        }
        assert!(validate_definition(&json!({"url":"http://localhost:8795/mcp"})).is_ok());
        assert!(validate_definition(
            &json!({"command":std::env::current_exe().unwrap(),"args":["server.js"],"env":{"KEY":"${KEY}"}})
        )
        .is_ok());
    }

    #[tokio::test]
    async fn http_probe_checks_real_initialize_response() {
        use axum::{routing::post, Router};
        let state = crate::server::AppState {
            db: crate::db::Database::in_memory().unwrap(),
            engine: std::sync::Arc::new(crate::engine::AcademicEngine::new()),
            port: 0,
            mcp_sessions: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/mcp", post(crate::mcp::http))
            .with_state(state);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let result = test_mcp_http(json!({"url":url})).await.unwrap();
        assert_eq!(result["serverInfo"]["name"], "scholargateway");
        assert!(test_mcp_http(json!({"command":"/usr/bin/node"}))
            .await
            .is_err());
        server.abort();
    }
}
