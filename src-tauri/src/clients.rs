//! One-step setup for the AI clients installed on this machine.
//!
//! The app already knows how to edit an MCP client config (`integrations`) and
//! how to install a skill folder (`skills`). What it could not do was tell the
//! user *which* clients exist on this machine, whether ScholarGateway is already
//! wired into them, and do both edits in one click. This module adds exactly
//! that: detection plus the two install actions, reusing the safe primitives so
//! backups, receipts and the "never touch what we did not install" rule apply
//! unchanged.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// The entry name this app writes into every client config.
pub const SERVER_NAME: &str = "scholargateway";
/// Environment variable Codex reads the gateway token from. Codex only accepts a
/// variable name, never a literal token, so the secret stays out of the file.
pub const CODEX_TOKEN_ENV: &str = "SCHOLARGATEWAY_TOKEN";

const BUNDLED_SKILLS: [(&str, &str); 3] = [
    ("paper-search", "builtin:paper-search"),
    ("paper-collect", "builtin:paper-collect"),
    ("research-resume", "builtin:research-resume"),
];

#[derive(Serialize, Clone)]
pub struct SkillState {
    pub name: String,
    pub source: String,
    /// A directory with this name exists in the client's skills folder.
    pub installed: bool,
    /// Installed by this app, so it can also be removed by this app.
    pub managed: bool,
    pub enabled: bool,
}

#[derive(Serialize, Clone)]
pub struct ClientStatus {
    pub id: String,
    pub name: String,
    /// The client keeps state on this machine, so it is worth offering.
    pub detected: bool,
    pub mcp_path: String,
    /// "json" or "toml" — the UI explains that Codex is edited as TOML.
    pub mcp_format: String,
    /// A server entry pointing at this gateway is present in the config.
    pub mcp_installed: bool,
    /// That entry was written by this app and can be removed again from here.
    pub mcp_managed: bool,
    /// Name of the entry found, which may differ from ours if the user added it.
    pub mcp_entry: Option<String>,
    pub mcp_error: Option<String>,
    pub skills_path: Option<String>,
    pub skills: Vec<SkillState>,
    pub skills_error: Option<String>,
    /// Client-specific instruction the UI shows verbatim.
    pub note: String,
    /// Set right after an install that had to deal with the gateway token, since
    /// what the user must do next differs per client.
    pub token_note: Option<String>,
}

struct Profile {
    id: &'static str,
    name: &'static str,
    /// Directory that exists only when the client itself is installed.
    marker: PathBuf,
    mcp: PathBuf,
    toml: bool,
    skills: Option<PathBuf>,
    note: &'static str,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Where Claude Desktop keeps its config, which is the one path that differs per
/// platform rather than living under the home directory in the same place.
fn claude_desktop_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/Claude")
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"))
            .join("Claude")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home.join(".config/Claude")
    }
}

fn profiles() -> Vec<Profile> {
    match home() {
        Some(home) => profiles_in(&home),
        None => Vec::new(),
    }
}

/// Split out from `profiles` so the tests can point a full set of clients at a
/// throwaway home directory instead of touching the real one.
fn profiles_in(home: &Path) -> Vec<Profile> {
    vec![
        Profile {
            id: "claude_code",
            name: "Claude Code",
            marker: home.join(".claude"),
            mcp: home.join(".claude.json"),
            toml: false,
            skills: Some(home.join(".claude/skills")),
            note: "Run /mcp in Claude Code after installing, or restart it, to pick up the new server.",
        },
        Profile {
            id: "claude_desktop",
            name: "Claude Desktop",
            marker: claude_desktop_dir(home),
            mcp: claude_desktop_dir(home).join("claude_desktop_config.json"),
            toml: false,
            skills: None,
            note: "Quit and reopen Claude Desktop to load the server. Claude Desktop does not read skill folders.",
        },
        Profile {
            id: "codex",
            name: "Codex",
            marker: home.join(".codex"),
            mcp: home.join(".codex/config.toml"),
            toml: true,
            skills: Some(home.join(".codex/skills")),
            note: "Codex is configured in TOML. Start a new Codex session after installing.",
        },
        Profile {
            id: "antigravity",
            name: "Antigravity",
            marker: home.join(".gemini/antigravity"),
            mcp: home.join(".gemini/antigravity/mcp_config.json"),
            toml: false,
            skills: Some(home.join(".gemini/antigravity/skills")),
            note: "Reload the MCP servers from Antigravity's MCP panel after installing.",
        },
    ]
}

fn profile(id: &str) -> Result<Profile, String> {
    find(&profiles(), id)
}

fn find(profiles: &[Profile], id: &str) -> Result<Profile, String> {
    profiles
        .iter()
        .find(|profile| profile.id == id)
        .map(|profile| Profile {
            id: profile.id,
            name: profile.name,
            marker: profile.marker.clone(),
            mcp: profile.mcp.clone(),
            toml: profile.toml,
            skills: profile.skills.clone(),
            note: profile.note,
        })
        .ok_or_else(|| "Unknown AI client".to_string())
}

/// Resolve symlinks so the caller edits the real file. The user's Antigravity
/// config is a symlink into another directory, and the config editor refuses to
/// replace a symlink — pointing it at the target is the fix it asks for, and the
/// resolved path is what the UI shows.
fn real_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        // The file itself may not exist yet; resolving the parent is enough to
        // avoid writing through a symlinked directory.
        match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => fs::canonicalize(parent)
                .map(|parent| parent.join(name))
                .unwrap_or_else(|_| path.to_path_buf()),
            _ => path.to_path_buf(),
        }
    })
}

/// The MCP endpoint of this gateway, as clients must address it.
pub fn gateway_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

fn points_at_gateway(definition: &Value, port: u16) -> bool {
    definition
        .get("url")
        .and_then(Value::as_str)
        .is_some_and(|url| {
            let url = url.trim_end_matches('/');
            url == gateway_url(port) || url == format!("http://localhost:{port}/mcp")
        })
}

// ---------------------------------------------------------------------------
// TOML clients (Codex)
// ---------------------------------------------------------------------------

fn read_toml(path: &Path) -> Result<toml_edit::DocumentMut, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("{} is not valid TOML: {error}", path.display()))
}

/// Replace the file in place, keeping a timestamped backup of the previous
/// contents. `toml_edit` preserves comments and formatting, so everything the
/// user wrote outside our own table survives untouched.
fn write_toml(path: &Path, document: &toml_edit::DocumentMut) -> Result<Option<String>, String> {
    let backup = if path.exists() {
        let backup = path.with_file_name(format!(
            "{}.{}.bak",
            path.file_name().unwrap().to_string_lossy(),
            uuid::Uuid::new_v4()
        ));
        fs::copy(path, &backup).map_err(|error| error.to_string())?;
        Some(backup.to_string_lossy().into_owned())
    } else {
        None
    };
    let temp = path.with_file_name(format!(".sg-config-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    file.write_all(document.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    fs::rename(&temp, path)
        .map_err(|error| format!("{error}; the temporary file was kept at {}", temp.display()))?;
    Ok(backup)
}

fn toml_status(document: &toml_edit::DocumentMut, port: u16) -> (Option<String>, bool) {
    let Some(servers) = document.get("mcp_servers").and_then(|item| item.as_table_like()) else {
        return (None, false);
    };
    for (name, item) in servers.iter() {
        let url = item
            .as_table_like()
            .and_then(|table| table.get("url"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        if url == gateway_url(port) || url == format!("http://localhost:{port}/mcp") {
            return (Some(name.to_string()), name == SERVER_NAME);
        }
    }
    (None, false)
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

fn skill_states(root: &Path) -> (Vec<SkillState>, Option<String>) {
    let managed = crate::skills::list_skills(root.to_string_lossy().into_owned()).unwrap_or_default();
    let mut states = Vec::new();
    for (name, source) in BUNDLED_SKILLS {
        let installed = root.join(name).is_dir();
        let entry = managed.iter().find(|skill| skill.name == name);
        states.push(SkillState {
            name: name.to_string(),
            source: source.to_string(),
            installed,
            managed: entry.is_some(),
            enabled: entry.map(|skill| skill.enabled).unwrap_or(installed),
        });
    }
    let error = if root.is_dir() {
        None
    } else {
        Some(format!(
            "{} does not exist yet; installing a skill creates it.",
            root.display()
        ))
    };
    (states, error)
}

fn status(profile: &Profile, port: u16) -> ClientStatus {
    let detected = profile.marker.is_dir();
    let mcp_path = real_path(&profile.mcp);
    let mut mcp_installed = false;
    let mut mcp_managed = false;
    let mut mcp_entry = None;
    let mut mcp_error = None;

    if detected {
        if profile.toml {
            match read_toml(&mcp_path) {
                Ok(document) => {
                    let (entry, ours) = toml_status(&document, port);
                    mcp_installed = entry.is_some();
                    mcp_managed = ours;
                    mcp_entry = entry;
                }
                Err(error) => mcp_error = Some(error),
            }
        } else {
            match crate::integrations::read_mcp_config(mcp_path.to_string_lossy().into_owned()) {
                Ok(view) => {
                    mcp_entry = view
                        .servers
                        .iter()
                        .find(|(_, definition)| points_at_gateway(definition, port))
                        .map(|(name, _)| name.clone());
                    mcp_installed = mcp_entry.is_some();
                    mcp_managed = view.managed.contains_key(SERVER_NAME);
                }
                Err(error) => mcp_error = Some(error),
            }
        }
    }

    let (skills_path, skills, skills_error) = match (&profile.skills, detected) {
        (Some(root), true) => {
            let root = real_path(root);
            let (skills, error) = skill_states(&root);
            (Some(root.to_string_lossy().into_owned()), skills, error)
        }
        (Some(root), false) => (Some(root.to_string_lossy().into_owned()), Vec::new(), None),
        (None, _) => (None, Vec::new(), None),
    };

    ClientStatus {
        id: profile.id.to_string(),
        name: profile.name.to_string(),
        detected,
        mcp_path: mcp_path.to_string_lossy().into_owned(),
        mcp_format: if profile.toml { "toml".into() } else { "json".into() },
        mcp_installed,
        mcp_managed,
        mcp_entry,
        mcp_error,
        skills_path,
        skills,
        skills_error,
        note: profile.note.to_string(),
        token_note: None,
    }
}

pub fn list(port: u16) -> Vec<ClientStatus> {
    profiles()
        .iter()
        .map(|profile| status(profile, port))
        .collect()
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// The server definition written into JSON clients. The token is only included
/// when the gateway is actually protected by one; the UI warns that it lands in
/// a plaintext file.
fn json_definition(port: u16, token: Option<&str>) -> Value {
    match token.map(str::trim).filter(|token| !token.is_empty()) {
        Some(token) => json!({
            "url": gateway_url(port),
            "headers": { "Authorization": format!("Bearer {token}") },
        }),
        None => json!({ "url": gateway_url(port) }),
    }
}

pub fn install_mcp(id: &str, port: u16, token: Option<String>) -> Result<ClientStatus, String> {
    install_mcp_for(&profile(id)?, port, token)
}

fn install_mcp_for(profile: &Profile, port: u16, token: Option<String>) -> Result<ClientStatus, String> {
    if !profile.marker.is_dir() {
        return Err(format!(
            "{} was not found on this machine ({} is missing)",
            profile.name,
            profile.marker.display()
        ));
    }
    let path = real_path(&profile.mcp);
    let has_token = token.as_deref().map(str::trim).is_some_and(|token| !token.is_empty());
    let mut note = None;
    if profile.toml {
        let mut document = read_toml(&path)?;
        if let (Some(existing), false) = toml_status(&document, port) {
            if existing != SERVER_NAME {
                return Err(format!(
                    "{} already points at this gateway under the name '{existing}'. Remove that entry first if you want it managed here.",
                    profile.name
                ));
            }
        }
        let servers = document["mcp_servers"].or_insert(toml_edit::table());
        let servers = servers
            .as_table_mut()
            .ok_or("mcp_servers is not a table in config.toml")?;
        servers.set_implicit(true);
        let mut entry = toml_edit::Table::new();
        entry.insert("url", toml_edit::value(gateway_url(port)));
        if has_token {
            // Codex reads the token from the environment, never from the file.
            entry.insert(
                "bearer_token_env_var",
                toml_edit::value(CODEX_TOKEN_ENV),
            );
        }
        servers.insert(SERVER_NAME, toml_edit::Item::Table(entry));
        write_toml(&path, &document)?;
        if has_token {
            note = Some(format!(
                "Codex reads the token from the environment: export {CODEX_TOKEN_ENV}=<your gateway token> before starting Codex. The token itself is not written to config.toml."
            ));
        }
    } else {
        let view = crate::integrations::read_mcp_config(path.to_string_lossy().into_owned())?;
        let action = if view.managed.contains_key(SERVER_NAME) {
            "update"
        } else if view.servers.contains_key(SERVER_NAME) {
            return Err(format!(
                "{} already has an entry named '{SERVER_NAME}' that this app did not create; it will not be overwritten.",
                profile.name
            ));
        } else {
            "add"
        };
        crate::integrations::edit_mcp_config(
            path.to_string_lossy().into_owned(),
            view.revision,
            SERVER_NAME.to_string(),
            action.to_string(),
            Some(json_definition(port, token.as_deref())),
        )?;
        if has_token {
            note = Some(format!(
                "The gateway token was written into {} as an Authorization header. That file is plain text — keep it out of any shared folder or repository.",
                path.display()
            ));
        }
    }
    Ok(ClientStatus { token_note: note, ..status(profile, port) })
}

pub fn remove_mcp(id: &str, port: u16) -> Result<ClientStatus, String> {
    remove_mcp_for(&profile(id)?, port)
}

fn remove_mcp_for(profile: &Profile, port: u16) -> Result<ClientStatus, String> {
    let path = real_path(&profile.mcp);
    if profile.toml {
        let mut document = read_toml(&path)?;
        let removed = document
            .get_mut("mcp_servers")
            .and_then(|item| item.as_table_like_mut())
            .map(|servers| servers.remove(SERVER_NAME).is_some())
            .unwrap_or(false);
        if !removed {
            return Err(format!(
                "No '{SERVER_NAME}' entry to remove in {}",
                path.display()
            ));
        }
        write_toml(&path, &document)?;
    } else {
        let view = crate::integrations::read_mcp_config(path.to_string_lossy().into_owned())?;
        if !view.managed.contains_key(SERVER_NAME) {
            return Err(
                "Only the entry this app installed can be removed from here; edit the client config yourself for anything else."
                    .to_string(),
            );
        }
        crate::integrations::edit_mcp_config(
            path.to_string_lossy().into_owned(),
            view.revision,
            SERVER_NAME.to_string(),
            "remove".to_string(),
            None,
        )?;
    }
    Ok(status(profile, port))
}

pub fn install_skills(id: &str, port: u16) -> Result<ClientStatus, String> {
    install_skills_for(&profile(id)?, port)
}

fn install_skills_for(profile: &Profile, port: u16) -> Result<ClientStatus, String> {
    let Some(root) = profile.skills.clone() else {
        return Err(format!("{} does not load skills from a folder", profile.name));
    };
    if !profile.marker.is_dir() {
        return Err(format!("{} was not found on this machine", profile.name));
    }
    if !root.exists() {
        fs::create_dir_all(&root).map_err(|error| {
            format!("Could not create {}: {error}", root.display())
        })?;
    }
    let root = real_path(&root);
    let target = root.to_string_lossy().into_owned();
    let mut failures = Vec::new();
    for (name, source) in BUNDLED_SKILLS {
        if root.join(name).is_dir() {
            continue;
        }
        if let Err(error) = crate::skills::install_skill(source.to_string(), target.clone()) {
            failures.push(format!("{name}: {error}"));
        }
    }
    if !failures.is_empty() {
        return Err(failures.join(" · "));
    }
    Ok(status(profile, port))
}

pub fn remove_skills(id: &str, port: u16) -> Result<ClientStatus, String> {
    remove_skills_for(&profile(id)?, port)
}

fn remove_skills_for(profile: &Profile, port: u16) -> Result<ClientStatus, String> {
    let Some(root) = profile.skills.clone() else {
        return Err(format!("{} does not load skills from a folder", profile.name));
    };
    let root = real_path(&root);
    let target = root.to_string_lossy().into_owned();
    let mut failures = Vec::new();
    let mut removed = 0;
    for (name, _) in BUNDLED_SKILLS {
        if !root.join(name).is_dir() {
            continue;
        }
        match crate::skills::remove_skill(target.clone(), name.to_string()) {
            Ok(_) => removed += 1,
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    if removed == 0 && !failures.is_empty() {
        return Err(failures.join(" · "));
    }
    Ok(status(profile, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let root = std::env::temp_dir().join(format!("sg-clients-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::canonicalize(root).unwrap()
    }

    /// A home directory containing every client this module knows about, so the
    /// real one is never touched by a test.
    fn fake_home() -> PathBuf {
        let home = scratch();
        fs::create_dir_all(home.join(".claude/skills")).unwrap();
        fs::write(home.join(".claude.json"), br#"{"theme":"dark","mcpServers":{}}"#).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/config.toml"), "model = \"gpt-5\"\n").unwrap();
        fs::create_dir_all(home.join(".gemini/antigravity")).unwrap();
        fs::write(
            home.join(".gemini/antigravity/mcp_config.json"),
            br#"{"mcpServers":{"other":{"command":"/usr/bin/node"}}}"#,
        )
        .unwrap();
        home
    }

    #[test]
    fn a_json_client_is_detected_wired_up_and_can_be_unwired() {
        let home = fake_home();
        let profiles = profiles_in(&home);
        let claude = find(&profiles, "claude_code").unwrap();

        let before = status(&claude, 8795);
        assert!(before.detected);
        assert!(!before.mcp_installed);
        assert_eq!(before.skills.len(), 3);
        assert!(before.skills.iter().all(|skill| !skill.installed));

        let installed = install_mcp_for(&claude, 8795, None).unwrap();
        assert!(installed.mcp_installed && installed.mcp_managed);
        assert_eq!(installed.mcp_entry.as_deref(), Some(SERVER_NAME));
        // Everything the client stored outside mcpServers survives the edit.
        let document: Value =
            serde_json::from_slice(&fs::read(home.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(document["theme"], "dark");
        assert_eq!(document["mcpServers"][SERVER_NAME]["url"], gateway_url(8795));

        let with_skills = install_skills_for(&claude, 8795).unwrap();
        assert!(with_skills.skills.iter().all(|skill| skill.installed && skill.managed));
        assert!(home.join(".claude/skills/paper-search/SKILL.md").is_file());

        // Installing twice must not fail or duplicate anything.
        assert!(install_skills_for(&claude, 8795).is_ok());

        let removed = remove_mcp_for(&claude, 8795).unwrap();
        assert!(!removed.mcp_installed);
        let cleared = remove_skills_for(&claude, 8795).unwrap();
        assert!(cleared.skills.iter().all(|skill| !skill.installed));
        let document: Value =
            serde_json::from_slice(&fs::read(home.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(document["theme"], "dark");
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_toml_client_is_wired_up_without_disturbing_the_rest_of_the_file() {
        let home = fake_home();
        let profiles = profiles_in(&home);
        let codex = find(&profiles, "codex").unwrap();
        assert_eq!(status(&codex, 8795).mcp_format, "toml");

        let installed = install_mcp_for(&codex, 8795, Some("a-gateway-token".into())).unwrap();
        assert!(installed.mcp_installed && installed.mcp_managed);
        let written = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(written.contains("model = \"gpt-5\""));
        assert!(written.contains("[mcp_servers.scholargateway]"));
        // Codex only accepts an environment variable name, so the token itself
        // never reaches the file.
        assert!(written.contains(CODEX_TOKEN_ENV));
        assert!(!written.contains("a-gateway-token"));

        let removed = remove_mcp_for(&codex, 8795).unwrap();
        assert!(!removed.mcp_installed);
        assert!(remove_mcp_for(&codex, 8795).is_err());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn an_entry_this_app_did_not_write_is_reported_but_never_replaced() {
        let home = fake_home();
        fs::write(
            home.join(".gemini/antigravity/mcp_config.json"),
            format!(
                r#"{{"mcpServers":{{"my-own":{{"url":"{}"}}}}}}"#,
                gateway_url(8795)
            ),
        )
        .unwrap();
        let profiles = profiles_in(&home);
        let antigravity = find(&profiles, "antigravity").unwrap();

        let state = status(&antigravity, 8795);
        assert!(state.mcp_installed, "an entry pointing here counts as connected");
        assert!(!state.mcp_managed);
        assert_eq!(state.mcp_entry.as_deref(), Some("my-own"));

        // A client that is not installed is reported, not set up behind the
        // user's back.
        let absent = find(&profiles_in(&home.join("empty")), "codex").unwrap();
        assert!(!status(&absent, 8795).detected);
        assert!(install_mcp_for(&absent, 8795, None).is_err());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn toml_install_keeps_surrounding_configuration_and_round_trips() {
        let root = scratch();
        let path = root.join("config.toml");
        let original = "# user comment\nmodel = \"gpt-5\"\n\n[mcp_servers.existing]\ncommand = \"/usr/bin/node\"\n";
        fs::write(&path, original).unwrap();

        let mut document = read_toml(&path).unwrap();
        assert_eq!(toml_status(&document, 8795), (None, false));

        let servers = document["mcp_servers"].or_insert(toml_edit::table());
        let servers = servers.as_table_mut().unwrap();
        let mut entry = toml_edit::Table::new();
        entry.insert("url", toml_edit::value(gateway_url(8795)));
        servers.insert(SERVER_NAME, toml_edit::Item::Table(entry));
        let backup = write_toml(&path, &document).unwrap().unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("# user comment\nmodel = \"gpt-5\""));
        assert!(written.contains("[mcp_servers.existing]"));
        assert!(written.contains("[mcp_servers.scholargateway]"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);

        let mut document = read_toml(&path).unwrap();
        assert_eq!(
            toml_status(&document, 8795),
            (Some(SERVER_NAME.to_string()), true)
        );
        // A different port is a different gateway and must not be claimed as ours.
        assert_eq!(toml_status(&document, 9000), (None, false));

        document
            .get_mut("mcp_servers")
            .unwrap()
            .as_table_like_mut()
            .unwrap()
            .remove(SERVER_NAME);
        write_toml(&path, &document).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(!written.contains("scholargateway"));
        assert!(written.contains("[mcp_servers.existing]"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_gateway_entry_is_recognised_whatever_the_user_named_it() {
        assert!(points_at_gateway(
            &json!({"url": "http://127.0.0.1:8795/mcp"}),
            8795
        ));
        assert!(points_at_gateway(
            &json!({"url": "http://localhost:8795/mcp/"}),
            8795
        ));
        assert!(!points_at_gateway(
            &json!({"url": "http://127.0.0.1:8795/mcp"}),
            8796
        ));
        assert!(!points_at_gateway(&json!({"command": "/usr/bin/node"}), 8795));
    }

    #[test]
    fn a_definition_carries_the_token_only_when_the_gateway_has_one() {
        assert_eq!(
            json_definition(8795, None),
            json!({"url": "http://127.0.0.1:8795/mcp"})
        );
        assert_eq!(json_definition(8795, Some("   ")), json_definition(8795, None));
        assert_eq!(
            json_definition(8795, Some("s3cret-token-value"))["headers"]["Authorization"],
            "Bearer s3cret-token-value"
        );
    }

    #[test]
    fn unknown_clients_are_rejected_rather_than_guessed() {
        assert!(profile("not-a-client").is_err());
        assert!(install_mcp("not-a-client", 8795, None).is_err());
    }

    #[test]
    fn a_symlinked_config_resolves_to_the_file_it_points_at() {
        let root = scratch();
        let real = root.join("real.json");
        fs::write(&real, "{}").unwrap();
        #[cfg(unix)]
        {
            let link = root.join("link.json");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert_eq!(real_path(&link), real);
        }
        // A file that does not exist yet still resolves through its parent.
        assert_eq!(real_path(&root.join("absent.json")), root.join("absent.json"));
        fs::remove_dir_all(root).unwrap();
    }
}
