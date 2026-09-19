use crate::models::{
    AgentLog, DownloadRecord, Paper, SearchHistoryItem, SearchResponse, TelemetryStats,
};
use rusqlite::{params, Connection, OptionalExtension, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Database {
    pub(crate) conn: Arc<Mutex<Connection>>,
    /// When true, secret values live in the OS keychain rather than this file.
    keychain: bool,
    /// Write counters for the bounded-retention sweeps, one per capped table.
    sweeps: Arc<Sweeps>,
}

/// Every insert into a capped table used to run its own retention `DELETE`, so a
/// single search paid for three full-table sorts (cache, agent log, history) on
/// top of its three inserts. The caps exist to stop unbounded growth, not to hold
/// the table at an exact size, so sweeping once per `SWEEP_EVERY` writes keeps the
/// same bound — the table simply floats up to `limit + SWEEP_EVERY` between
/// sweeps — at a fraction of the cost.
#[derive(Default)]
struct Sweeps {
    cache: std::sync::atomic::AtomicU32,
    logs: std::sync::atomic::AtomicU32,
    history: std::sync::atomic::AtomicU32,
}

const SWEEP_EVERY: u32 = 64;

impl Sweeps {
    /// True once every `SWEEP_EVERY` calls, including the very first one so a
    /// long-running install still trims promptly after a restart.
    fn due(counter: &std::sync::atomic::AtomicU32) -> bool {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % SWEEP_EVERY == 0
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;
    #[test]
    fn cache_telemetry_comes_from_academic_search_logs() {
        let db = Database::in_memory().unwrap();
        assert_eq!(db.get_telemetry_stats(9876).cache_hit_rate, 0.0);
        for (id, method, status) in [
            ("1", "POST /api/search", "Success (200 OK)"),
            ("2", "POST /api/search", "Cache Hit (200 OK)"),
            ("3", "POST /api/web/search", "Success (200 OK)"),
        ] {
            db.log_agent_query(&AgentLog {
                id: id.into(),
                timestamp: "12:00:00".into(),
                agent_name: "test".into(),
                method: method.into(),
                query: "education".into(),
                result_count: 1,
                latency_ms: 1,
                status: status.into(),
            });
        }
        let stats = db.get_telemetry_stats(9876);
        assert_eq!(stats.cache_hit_rate, 50.0);
        assert_eq!(stats.total_queries, 3);
        assert_eq!(stats.port, 9876);
    }
    fn paper(id: &str, title: &str) -> Paper {
        serde_json::from_value(serde_json::json!({"id":id,"title":title,"authors":[],"source":"crossref","open_access":false})).unwrap()
    }
    #[test]
    fn legacy_import_preserves_existing_and_is_atomic() {
        let db = Database::in_memory().unwrap();
        db.save_paper(&paper("existing", "Current title")).unwrap();
        let imported = [
            paper("existing", "Old title"),
            paper("new", "Economics paper"),
        ];
        assert_eq!(db.import_library(&imported).unwrap(), 1);
        assert_eq!(db.import_library(&imported).unwrap(), 0);
        assert_eq!(db.find_paper("existing").unwrap().title, "Current title");
        assert_eq!(db.find_paper("new").unwrap().title, "Economics paper");
        assert_eq!(db.read_library().unwrap().len(), 2);
        assert!(db
            .import_library(&[paper("valid", "Valid"), paper("", "Invalid")])
            .is_err());
        assert!(db.find_paper("valid").is_none());
        db.delete_saved_paper("new").unwrap();
        assert!(db.find_paper("new").is_none());
    }

    #[test]
    fn an_older_installs_default_workspace_is_renamed_but_a_chosen_name_is_kept() {
        let db = Database::in_memory().unwrap();
        assert_eq!(db.list_workspaces()[0].name, DEFAULT_WORKSPACE);

        // An install from before the English interface: one workspace still
        // carrying the old default name.
        let legacy = Database::in_memory().unwrap();
        {
            let conn = legacy.conn();
            conn.execute(
                "UPDATE workspaces SET name = ?1",
                params![LEGACY_DEFAULT_WORKSPACE],
            )
            .unwrap();
            bootstrap_default_workspace(&conn).unwrap();
        }
        assert_eq!(legacy.list_workspaces()[0].name, DEFAULT_WORKSPACE);

        // The same name typed by the user, alongside another workspace, is theirs.
        let chosen = Database::in_memory().unwrap();
        chosen
            .create_workspace(LEGACY_DEFAULT_WORKSPACE, None)
            .unwrap();
        {
            let conn = chosen.conn();
            bootstrap_default_workspace(&conn).unwrap();
        }
        assert!(chosen
            .list_workspaces()
            .iter()
            .any(|workspace| workspace.name == LEGACY_DEFAULT_WORKSPACE));
    }

    #[test]
    fn workspace_lifecycle_scopes_papers_and_notes() {
        let db = Database::in_memory().unwrap();
        assert_eq!(
            db.list_workspaces().len(),
            1,
            "a default workspace is created"
        );
        let ws = db.create_workspace("Ung thư", Some("tổng quan")).unwrap();
        assert_eq!(db.list_workspaces().len(), 2);
        let p = paper("p1", "Paper one");
        db.add_workspace_paper(&ws.id, &p, Some("ghi chú")).unwrap();
        db.add_workspace_paper(&ws.id, &p, Some("cập nhật"))
            .unwrap();
        let papers = db.workspace_papers(&ws.id).unwrap();
        assert_eq!(papers.len(), 1, "same paper is not duplicated");
        assert_eq!(papers[0].note.as_deref(), Some("cập nhật"));
        assert_eq!(
            db.list_workspaces()
                .into_iter()
                .find(|w| w.id == ws.id)
                .unwrap()
                .paper_count,
            1
        );
        db.update_workspace_paper(
            &ws.id,
            "p1",
            Some("lần nữa"),
            Some("reading"),
            Some(true),
            Some(&["ung thư".to_string()]),
        )
        .unwrap();
        let updated = &db.workspace_papers(&ws.id).unwrap()[0];
        assert_eq!(updated.note.as_deref(), Some("lần nữa"));
        assert_eq!(updated.status, "reading");
        assert!(updated.favorite);
        assert_eq!(updated.tags, vec!["ung thư".to_string()]);
        assert!(db.add_workspace_paper("missing", &p, None).is_err());
        db.remove_workspace_paper(&ws.id, "p1").unwrap();
        assert!(db.workspace_papers(&ws.id).unwrap().is_empty());
        db.delete_workspace(&ws.id).unwrap();
        assert!(!db.workspace_exists(&ws.id));
    }

    #[test]
    fn interest_library_migrates_existing_papers_once_and_stays_hidden() {
        let db = Database::in_memory().unwrap();
        let legacy = db.create_workspace("Legacy project", None).unwrap();
        let p = paper("interesting", "Paper of interest");
        db.add_workspace_paper(&legacy.id, &p, Some("keep this"))
            .unwrap();
        {
            let conn = db.conn();
            conn.execute(
                "DELETE FROM workspace_papers WHERE workspace_id = ?1",
                params![INTEREST_LIBRARY_ID],
            )
            .unwrap();
            conn.execute(
                "DELETE FROM workspaces WHERE id = ?1",
                params![INTEREST_LIBRARY_ID],
            )
            .unwrap();
            bootstrap_interest_library(&conn).unwrap();
        }
        let migrated = db.library_papers().unwrap();
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0].note.as_deref(), Some("keep this"));
        assert!(db
            .list_workspaces()
            .iter()
            .all(|workspace| workspace.id != INTEREST_LIBRARY_ID));

        db.remove_library_paper("interesting").unwrap();
        {
            let conn = db.conn();
            bootstrap_interest_library(&conn).unwrap();
        }
        assert!(db.library_papers().unwrap().is_empty());
    }

    #[test]
    fn saved_search_flag_round_trips() {
        let db = Database::in_memory().unwrap();
        db.add_search_history(&SearchHistoryItem {
            id: "s1".into(),
            query: "ung thư".into(),
            sources: None,
            result_count: 3,
            elapsed_ms: 10,
            created_at: 1,
            workspace_id: None,
            saved: false,
        });
        assert!(!db
            .get_search_history(10, None)
            .iter()
            .any(|item| item.saved));
        db.set_search_saved("s1", true).unwrap();
        let rows = db.get_search_history(10, None);
        assert!(rows.iter().any(|item| item.id == "s1" && item.saved));
    }
}

fn normalize_note(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(10_000).collect())
    }
}

fn normalize_status(value: &str) -> Result<String, String> {
    match value.trim().to_lowercase().as_str() {
        "unread" => Ok("unread".into()),
        "reading" => Ok("reading".into()),
        "read" => Ok("read".into()),
        _ => Err("status must be unread, reading or read".into()),
    }
}

fn split_tags(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

fn join_tags(tags: &[String]) -> String {
    let mut cleaned: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        let tag: String = tag.chars().take(40).collect();
        if !cleaned
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&tag))
        {
            cleaned.push(tag);
        }
        if cleaned.len() >= 20 {
            break;
        }
    }
    cleaned.join(",")
}

/// Moves a pre-rename data directory to the current one.
///
/// This directory holds the entire library — saved papers, workspaces, notes,
/// history and settings — so renaming the app without moving it would look
/// exactly like losing everything. The move only runs when the new directory
/// does not exist yet, and a failure is deliberately not fatal: the app then
/// starts on an empty directory and the old one is still on disk, untouched,
/// rather than the app refusing to launch.
fn adopt_legacy_data_dir(legacy: &std::path::Path, current: &std::path::Path) {
    if current.exists() || !legacy.is_dir() {
        return;
    }
    if let Err(error) = std::fs::rename(legacy, current) {
        eprintln!(
            "Could not move {} to {}: {error}. Starting with an empty library; \
             the previous data is still in the old directory.",
            legacy.display(),
            current.display()
        );
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Add a column to an existing table so upgrades don't require a fresh DB.
fn ensure_column(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == column);
    drop(stmt);
    if !exists {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"),
            [],
        )?;
    }
    Ok(())
}

/// The name this app shipped as the default workspace before the interface was
/// English throughout. Installs created back then still carry it.
const LEGACY_DEFAULT_WORKSPACE: &str = "Nghiên cứu của tôi";
const DEFAULT_WORKSPACE: &str = "My Research";
pub const INTEREST_LIBRARY_ID: &str = "__interest_library__";

/// Create a default workspace on first run and adopt any papers saved before
/// workspaces existed, so the library is never orphaned.
fn bootstrap_default_workspace(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM workspaces WHERE id <> ?1",
        params![INTEREST_LIBRARY_ID],
        |row| row.get(0),
    )?;
    if count == 1 {
        // Bring an older install's default workspace in line with the English
        // default. The name must still be the one this app wrote, and it must be
        // the only workspace, so a name the user chose is never overwritten —
        // and renaming it again afterwards is one click away.
        conn.execute(
            "UPDATE workspaces SET name = ?1 WHERE name = ?2 AND id <> ?3",
            params![
                DEFAULT_WORKSPACE,
                LEGACY_DEFAULT_WORKSPACE,
                INTEREST_LIBRARY_ID
            ],
        )?;
    }
    if count == 0 {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_secs();
        conn.execute(
            "INSERT INTO workspaces (id, name, description, created_at, updated_at) VALUES (?1, ?2, NULL, ?3, ?3)",
            params![id, DEFAULT_WORKSPACE, now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO workspace_papers (workspace_id, paper_id, note, added_at) SELECT ?1, id, NULL, ?2 FROM saved_papers",
            params![id, now],
        )?;
    }
    Ok(())
}

/// Create the single library used by the desktop UI. Existing workspace data
/// is copied only when this hidden library is first introduced, so removing an
/// item later never makes it reappear on the next launch.
fn bootstrap_interest_library(conn: &Connection) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
        params![INTEREST_LIBRARY_ID],
        |row| row.get(0),
    )?;
    if exists {
        return Ok(());
    }

    let now = now_secs();
    conn.execute(
        "INSERT INTO workspaces (id, name, description, created_at, updated_at) VALUES (?1, 'Library', NULL, ?2, ?2)",
        params![INTEREST_LIBRARY_ID, now],
    )?;
    conn.execute(
        "INSERT INTO workspace_papers (workspace_id, paper_id, note, added_at, status, favorite, tags)
         SELECT ?1, p.id,
                (SELECT wp.note FROM workspace_papers wp WHERE wp.paper_id = p.id AND wp.note IS NOT NULL ORDER BY wp.added_at DESC LIMIT 1),
                p.saved_at,
                COALESCE((SELECT CASE MAX(CASE wp.status WHEN 'read' THEN 2 WHEN 'reading' THEN 1 ELSE 0 END)
                                  WHEN 2 THEN 'read' WHEN 1 THEN 'reading' ELSE 'unread' END
                          FROM workspace_papers wp WHERE wp.paper_id = p.id), 'unread'),
                COALESCE((SELECT MAX(wp.favorite) FROM workspace_papers wp WHERE wp.paper_id = p.id), 0),
                COALESCE((SELECT wp.tags FROM workspace_papers wp WHERE wp.paper_id = p.id AND wp.tags <> '' ORDER BY wp.added_at DESC LIMIT 1), '')
         FROM saved_papers p",
        params![INTEREST_LIBRARY_ID],
    )?;
    Ok(())
}

impl Database {
    /// Credentials the user entered in Settings, read fresh so a key saved mid-session
    /// takes effect on the next search without a restart.
    pub fn source_credentials(&self) -> crate::models::SourceCredentials {
        crate::models::SourceCredentials {
            openalex_email: self.get_config("openalex_email"),
            openalex_api_key: self.get_config("openalex_api_key"),
            semantic_scholar_api_key: self.get_config("semantic_scholar_api_key"),
            ncbi_api_key: self.get_config("ncbi_api_key"),
            ncbi_email: self.get_config("ncbi_email"),
            crossref_email: self.get_config("crossref_email"),
            unpaywall_email: self.get_config("unpaywall_email"),
            scopus_api_key: self.get_config("scopus_api_key"),
            ieee_api_key: self.get_config("ieee_api_key"),
            springer_api_key: self.get_config("springer_api_key"),
            perplexity_api_key: self.get_config("perplexity_api_key"),
            core_api_key: self.get_config("core_api_key"),
            dimensions_api_key: self.get_config("dimensions_api_key"),
            wos_api_key: self.get_config("wos_api_key"),
            consensus_session: self.get_config("consensus_session"),
            openevidence_session: self.get_config("openevidence_session"),
        }
    }

    /// The one way this module takes the connection lock.
    ///
    /// A panic while the lock was held used to poison the mutex permanently:
    /// every later `lock().unwrap()` panicked in turn, so a single failed
    /// request took the whole database down until the app was restarted. The
    /// connection itself is not left in a torn state by a panicking Rust
    /// closure — rusqlite statements are transactional — so recovering the
    /// guard is safe and strictly better than refusing to serve.
    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| {
            self.conn.clear_poison();
            poisoned.into_inner()
        })
    }

    pub fn init() -> Result<Self, String> {
        // The old variable name still works for anyone who scripted against it.
        let configured = std::env::var("SCHOLARGATE_DATA_DIR")
            .or_else(|_| std::env::var("SCHOLARGATEWAY_DATA_DIR"));
        let db_dir = match configured {
            Ok(path) => crate::skills::directory(&path)?,
            Err(std::env::VarError::NotPresent) => {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .map_err(|_| "Cannot locate user data directory")?;
                let home = PathBuf::from(home);
                let path = home.join(".scholargate");
                adopt_legacy_data_dir(&home.join(".scholargateway"), &path);
                std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                path
            }
            Err(_) => return Err("SCHOLARGATE_DATA_DIR must be UTF-8".into()),
        };
        let db_path = db_dir.join("library.db");
        // Credentials live in this SQLite file, so keep it owner-only instead of
        // relying on the process umask.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&db_dir, std::fs::Permissions::from_mode(0o700));
        }
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600));
        }

        Self::from_connection(conn, crate::secrets::enabled()).map_err(|e| e.to_string())
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        // Tests must never touch the real keychain.
        Self::from_connection(Connection::open_in_memory()?, false)
    }

    fn from_connection(conn: Connection, keychain: bool) -> Result<Self> {
        // WAL lets the read paths run while a write is in flight instead of
        // serialising behind it; `busy_timeout` makes a contended write wait
        // rather than fail instantly with SQLITE_BUSY. `synchronous=NORMAL` is
        // the documented safe pairing with WAL for an app-local database.
        // In-memory test databases do not support WAL, so a failure here is not
        // fatal — the pragma is an optimisation, not a correctness requirement.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
        let _ = conn.pragma_update(None, "foreign_keys", "ON");

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_credentials (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE,
            workspace_ids TEXT NOT NULL, writable INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0
        );",
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS search_cache (
                query_hash TEXT PRIMARY KEY,
                query TEXT NOT NULL,
                response_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS saved_papers (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                paper_json TEXT NOT NULL,
                saved_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_logs (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                agent_name TEXT NOT NULL,
                method TEXT NOT NULL,
                query TEXT NOT NULL,
                result_count INTEGER NOT NULL,
                latency_ms INTEGER NOT NULL,
                status TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS search_history (
                id TEXT PRIMARY KEY,
                query TEXT NOT NULL,
                sources TEXT,
                result_count INTEGER NOT NULL,
                elapsed_ms INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS download_history (
                id TEXT PRIMARY KEY,
                paper_id TEXT NOT NULL,
                title TEXT NOT NULL,
                pdf_url TEXT NOT NULL,
                local_path TEXT NOT NULL,
                file_size_bytes INTEGER NOT NULL,
                source TEXT,
                year INTEGER,
                downloaded_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS app_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;

        // A paper can belong to several research projects, each with its own note.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS workspace_papers (
                workspace_id TEXT NOT NULL,
                paper_id TEXT NOT NULL,
                note TEXT,
                added_at INTEGER NOT NULL,
                PRIMARY KEY (workspace_id, paper_id)
            )",
            [],
        )?;

        ensure_column(&conn, "search_history", "workspace_id", "TEXT")?;
        ensure_column(
            &conn,
            "search_history",
            "saved",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(&conn, "download_history", "workspace_id", "TEXT")?;
        ensure_column(
            &conn,
            "workspace_papers",
            "status",
            "TEXT NOT NULL DEFAULT 'unread'",
        )?;
        ensure_column(
            &conn,
            "workspace_papers",
            "favorite",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &conn,
            "workspace_papers",
            "tags",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        // Indexes are created after `ensure_column` so they can cover columns
        // that older installs gained by migration.
        //
        // `workspace_papers` is keyed (workspace_id, paper_id), which answers
        // "papers in this workspace" but not the reverse. `read_library` runs
        // four correlated subqueries per row that all look up by `paper_id`
        // alone, so without this index each saved paper costs a full scan of
        // the join table four times over.
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_workspace_papers_paper
                ON workspace_papers(paper_id);
             CREATE INDEX IF NOT EXISTS idx_search_history_workspace
                ON search_history(workspace_id, created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_download_history_workspace
                ON download_history(workspace_id, downloaded_at DESC);
             CREATE INDEX IF NOT EXISTS idx_download_history_paper
                ON download_history(paper_id);
             CREATE INDEX IF NOT EXISTS idx_search_cache_created
                ON search_cache(created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_saved_papers_saved_at
                ON saved_papers(saved_at DESC);",
        )?;

        bootstrap_default_workspace(&conn)?;
        bootstrap_interest_library(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            keychain,
            sweeps: Arc::new(Sweeps::default()),
        })
    }

    pub fn get_cache(&self, hash: &str, ttl_seconds: u64) -> Option<SearchResponse> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT response_json, created_at FROM search_cache WHERE query_hash = ?1")
            .ok()?;

        let mut rows = stmt.query(params![hash]).ok()?;
        if let Some(row) = rows.next().ok()? {
            let json_str: String = row.get(0).ok()?;
            let created_at: u64 = row.get(1).ok()?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            if now.saturating_sub(created_at) < ttl_seconds {
                if let Ok(mut resp) = serde_json::from_str::<SearchResponse>(&json_str) {
                    resp.cache_hit = true;
                    return Some(resp);
                }
            }
        }
        None
    }

    pub fn set_cache(&self, hash: &str, query: &str, resp: &SearchResponse) {
        {
            let conn = self.conn();
            if let Ok(json_str) = serde_json::to_string(resp) {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO search_cache (query_hash, query, response_json, created_at) VALUES (?1, ?2, ?3, ?4)",
                    params![hash, query, json_str, now],
                );
                // Bounded cache retention: prune entries older than 30 days and
                // keep the latest 3,000. Swept periodically rather than on every
                // write; see `Sweeps`.
                if Sweeps::due(&self.sweeps.cache) {
                    let thirty_days_ago = now.saturating_sub(30 * 86400);
                    let _ = conn.execute(
                        "DELETE FROM search_cache WHERE created_at < ?1 OR query_hash NOT IN (SELECT query_hash FROM search_cache ORDER BY created_at DESC LIMIT 3000)",
                        params![thirty_days_ago],
                    );
                }
            }
        }
    }

    pub fn save_paper(&self, paper: &Paper) -> Result<()> {
        let conn = self.conn();
        let json_str = serde_json::to_string(paper).unwrap_or_default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        conn.execute(
            "INSERT OR REPLACE INTO saved_papers (id, title, paper_json, saved_at) VALUES (?1, ?2, ?3, ?4)",
            params![paper.id, paper.title, json_str, now],
        )?;
        Ok(())
    }

    pub fn get_saved_papers(&self) -> Vec<Paper> {
        let mut papers = Vec::new();
        {
            let conn = self.conn();
            // Bound to a local first: in edition 2021 an `if let` scrutinee
            // temporary lives until the end of the enclosing block, which would
            // outlive the guard declared in that same block.
            let prepared = conn.prepare("SELECT paper_json FROM saved_papers ORDER BY saved_at DESC");
            if let Ok(mut stmt) = prepared {
                if let Ok(rows) = stmt.query_map([], |row| {
                    let json_str: String = row.get(0)?;
                    Ok(serde_json::from_str::<Paper>(&json_str).ok())
                }) {
                    for p in rows.flatten().flatten() {
                        papers.push(p);
                    }
                }
            }
        }
        papers
    }

    /// Retained for legacy library migration/import and tests; the UI now reads
    /// papers through workspace endpoints.
    #[allow(dead_code)]
    pub fn read_library(&self) -> Result<Vec<Paper>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT paper_json FROM saved_papers ORDER BY saved_at DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }

    #[allow(dead_code)]
    pub fn import_library(&self, papers: &[Paper]) -> Result<usize, String> {
        if papers.len() > 5000 {
            return Err("At most 5000 papers can be imported at once".into());
        }
        let mut encoded = Vec::new();
        let mut bytes = 0;
        for paper in papers {
            if paper.id.trim().is_empty() || paper.title.trim().is_empty() {
                return Err("Imported paper is missing an id or a title".into());
            }
            let json = serde_json::to_string(paper).map_err(|e| e.to_string())?;
            bytes += json.len();
            if bytes > 20 * 1024 * 1024 {
                return Err("Import is limited to 20 MiB".into());
            }
            encoded.push(json);
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut inserted = 0;
        for (paper, json) in papers.iter().zip(encoded) {
            inserted += tx.execute("INSERT OR IGNORE INTO saved_papers (id, title, paper_json, saved_at) VALUES (?1, ?2, ?3, ?4)", params![paper.id, paper.title, json, now]).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(inserted)
    }

    pub fn find_paper(&self, id: &str) -> Option<Paper> {
        if let Some(paper) = self
            .get_saved_papers()
            .into_iter()
            .find(|paper| crate::details::matches(paper, id))
        {
            return Some(paper);
        }
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT response_json FROM search_cache ORDER BY created_at DESC LIMIT 100")
            .ok()?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .ok()?;
        for row in rows.flatten() {
            if let Ok(response) = serde_json::from_str::<SearchResponse>(&row) {
                if let Some(paper) = response
                    .papers
                    .into_iter()
                    .find(|paper| crate::details::matches(paper, id))
                {
                    return Some(paper);
                }
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn remove_paper(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM saved_papers WHERE id = ?1", params![id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn delete_saved_paper(&self, id: &str) -> Result<()> {
        self.remove_paper(id)
    }

    // ---- Workspaces -------------------------------------------------------

    pub fn list_workspaces(&self) -> Vec<crate::models::Workspace> {
        let conn = self.conn();
        let mut statement = match conn.prepare(
            "SELECT w.id, w.name, w.description, w.created_at, w.updated_at,
                    (SELECT COUNT(*) FROM workspace_papers wp WHERE wp.workspace_id = w.id),
                    (SELECT COUNT(*) FROM search_history sh WHERE sh.workspace_id = w.id)
             FROM workspaces w WHERE w.id <> ?1 ORDER BY w.updated_at DESC",
        ) {
            Ok(statement) => statement,
            Err(_) => return Vec::new(),
        };
        let rows = statement.query_map(params![INTEREST_LIBRARY_ID], |row| {
            Ok(crate::models::Workspace {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                paper_count: row.get::<_, i64>(5)?.max(0) as usize,
                query_count: row.get::<_, i64>(6)?.max(0) as usize,
            })
        });
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn workspace_exists(&self, id: &str) -> bool {
        self.conn()
            .query_row("SELECT 1 FROM workspaces WHERE id = ?1", params![id], |_| {
                Ok(())
            })
            .is_ok()
    }

    pub fn create_workspace(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<crate::models::Workspace, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("Workspace name must be 1-120 characters".into());
        }
        let description = description
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(2000).collect::<String>());
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_secs();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO workspaces (id, name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id, name, description, now],
        )
        .map_err(|error| error.to_string())?;
        Ok(crate::models::Workspace {
            id,
            name: name.to_string(),
            description,
            created_at: now,
            updated_at: now,
            paper_count: 0,
            query_count: 0,
        })
    }

    pub fn rename_workspace(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<(), String> {
        if id == INTEREST_LIBRARY_ID {
            return Err("The interest library cannot be renamed".into());
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("Workspace name must be 1-120 characters".into());
        }
        let description = description
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(2000).collect::<String>());
        let conn = self.conn();
        let changed = conn
            .execute(
                "UPDATE workspaces SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
                params![name, description, now_secs(), id],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Workspace not found".into());
        }
        Ok(())
    }

    pub fn delete_workspace(&self, id: &str) -> Result<(), String> {
        if id == INTEREST_LIBRARY_ID {
            return Err("The interest library cannot be deleted".into());
        }
        let conn = self.conn();
        conn.execute(
            "DELETE FROM workspace_papers WHERE workspace_id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
        let changed = conn
            .execute("DELETE FROM workspaces WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;
        let _ = conn.execute(
            "UPDATE search_history SET workspace_id = NULL WHERE workspace_id = ?1",
            params![id],
        );
        if changed == 0 {
            return Err("Workspace not found".into());
        }
        Ok(())
    }

    pub fn workspace_papers(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<crate::models::WorkspacePaper>, String> {
        self.workspace_papers_page(workspace_id, i64::MAX, 0)
    }

    pub fn workspace_papers_page(
        &self,
        workspace_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::models::WorkspacePaper>, String> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT p.paper_json, wp.note, wp.added_at, wp.status, wp.favorite, wp.tags
                 FROM workspace_papers wp JOIN saved_papers p ON p.id = wp.paper_id
                 WHERE wp.workspace_id = ?1 ORDER BY wp.added_at DESC, wp.paper_id ASC LIMIT ?2 OFFSET ?3",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![workspace_id, limit, offset], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut papers = Vec::new();
        for row in rows.flatten() {
            if let Ok(paper) = serde_json::from_str::<Paper>(&row.0) {
                papers.push(crate::models::WorkspacePaper {
                    paper,
                    note: row.1,
                    added_at: row.2,
                    status: row.3,
                    favorite: row.4 != 0,
                    tags: split_tags(&row.5),
                });
            }
        }
        Ok(papers)
    }

    pub fn add_workspace_paper(
        &self,
        workspace_id: &str,
        paper: &Paper,
        note: Option<&str>,
    ) -> Result<(), String> {
        if !self.workspace_exists(workspace_id) {
            return Err("Workspace not found".into());
        }
        if paper.id.trim().is_empty() || paper.title.trim().is_empty() {
            return Err("Paper is missing an id or a title".into());
        }
        self.save_paper(paper).map_err(|error| error.to_string())?;
        let note = note
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(10_000).collect::<String>());
        let conn = self.conn();
        conn.execute(
            "INSERT INTO workspace_papers (workspace_id, paper_id, note, added_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workspace_id, paper_id) DO UPDATE SET note = COALESCE(excluded.note, workspace_papers.note)",
            params![workspace_id, paper.id, note, now_secs()],
        )
        .map_err(|error| error.to_string())?;
        let _ = conn.execute(
            "UPDATE workspaces SET updated_at = ?1 WHERE id = ?2",
            params![now_secs(), workspace_id],
        );
        Ok(())
    }

    pub fn update_workspace_paper(
        &self,
        workspace_id: &str,
        paper_id: &str,
        note: Option<&str>,
        status: Option<&str>,
        favorite: Option<bool>,
        tags: Option<&[String]>,
    ) -> Result<(), String> {
        let conn = self.conn();
        let existing = conn
            .query_row(
                "SELECT note, status, favorite, tags FROM workspace_papers WHERE workspace_id = ?1 AND paper_id = ?2",
                params![workspace_id, paper_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((current_note, current_status, current_favorite, current_tags)) = existing else {
            return Err("Paper is not in this workspace".into());
        };

        let note = match note {
            Some(value) => normalize_note(value),
            None => current_note,
        };
        let status = match status {
            Some(value) => normalize_status(value)?,
            None => current_status,
        };
        let favorite = favorite.unwrap_or(current_favorite != 0);
        let tags = match tags {
            Some(list) => join_tags(list),
            None => current_tags,
        };

        conn.execute(
            "UPDATE workspace_papers SET note = ?1, status = ?2, favorite = ?3, tags = ?4 WHERE workspace_id = ?5 AND paper_id = ?6",
            params![note, status, favorite as i64, tags, workspace_id, paper_id],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn remove_workspace_paper(&self, workspace_id: &str, paper_id: &str) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM workspace_papers WHERE workspace_id = ?1 AND paper_id = ?2",
            params![workspace_id, paper_id],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn library_papers(&self) -> Result<Vec<crate::models::WorkspacePaper>, String> {
        self.workspace_papers(INTEREST_LIBRARY_ID)
    }

    pub fn library_papers_page(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::models::WorkspacePaper>, String> {
        self.workspace_papers_page(INTEREST_LIBRARY_ID, limit, offset)
    }

    pub fn library_paper_count(&self) -> Result<usize, String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT COUNT(*) FROM workspace_papers WHERE workspace_id = ?1",
            [INTEREST_LIBRARY_ID],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
    }

    pub fn add_library_paper(&self, paper: &Paper) -> Result<(), String> {
        self.add_workspace_paper(INTEREST_LIBRARY_ID, paper, None)
    }

    pub fn update_library_paper(
        &self,
        paper_id: &str,
        note: Option<&str>,
        status: Option<&str>,
        favorite: Option<bool>,
        tags: Option<&[String]>,
    ) -> Result<(), String> {
        self.update_workspace_paper(INTEREST_LIBRARY_ID, paper_id, note, status, favorite, tags)
    }

    pub fn remove_library_paper(&self, paper_id: &str) -> Result<(), String> {
        self.remove_workspace_paper(INTEREST_LIBRARY_ID, paper_id)
    }

    pub fn log_agent_query(&self, log: &AgentLog) {
        {
            let conn = self.conn();
            let _ = conn.execute(
                "INSERT INTO agent_logs (id, timestamp, agent_name, method, query, result_count, latency_ms, status) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    log.id,
                    log.timestamp,
                    log.agent_name,
                    log.method,
                    log.query,
                    log.result_count,
                    log.latency_ms,
                    log.status
                ],
            );
            // Bounded log retention: keep the latest 2,000. Swept periodically
            // rather than on every write; see `Sweeps`.
            if Sweeps::due(&self.sweeps.logs) {
                let _ = conn.execute(
                    "DELETE FROM agent_logs WHERE rowid NOT IN (SELECT rowid FROM agent_logs ORDER BY rowid DESC LIMIT 2000)",
                    [],
                );
            }
        }
    }

    // Search history
    pub fn add_search_history(&self, item: &SearchHistoryItem) {
        {
            let conn = self.conn();
            let _ = conn.execute(
                "INSERT INTO search_history (id, query, sources, result_count, elapsed_ms, created_at, workspace_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![item.id, item.query, item.sources, item.result_count, item.elapsed_ms, item.created_at, item.workspace_id],
            );
            // Bounded unsaved history retention: keep the latest 2,000 unsaved
            // queries; saved searches are always retained. Swept periodically
            // rather than on every write; see `Sweeps`.
            if Sweeps::due(&self.sweeps.history) {
                let _ = conn.execute(
                    "DELETE FROM search_history WHERE saved = 0 AND rowid NOT IN (SELECT rowid FROM search_history WHERE saved = 0 ORDER BY rowid DESC LIMIT 2000)",
                    [],
                );
            }
        }
    }

    pub fn get_search_history(
        &self,
        limit: usize,
        workspace_id: Option<&str>,
    ) -> Vec<SearchHistoryItem> {
        let mut list = Vec::new();
        {
            let conn = self.conn();
            let (sql, filter) = match workspace_id {
                Some(_) => (
                    "SELECT id, query, sources, result_count, elapsed_ms, created_at, workspace_id, saved FROM search_history WHERE workspace_id = ?1 ORDER BY saved DESC, created_at DESC LIMIT ?2",
                    true,
                ),
                None => (
                    "SELECT id, query, sources, result_count, elapsed_ms, created_at, workspace_id, saved FROM search_history ORDER BY saved DESC, created_at DESC LIMIT ?1",
                    false,
                ),
            };
            let prepared = conn.prepare(sql);
            if let Ok(mut stmt) = prepared {
                let map_row = |row: &rusqlite::Row<'_>| {
                    Ok(SearchHistoryItem {
                        id: row.get(0)?,
                        query: row.get(1)?,
                        sources: row.get(2)?,
                        result_count: row.get(3)?,
                        elapsed_ms: row.get(4)?,
                        created_at: row.get(5)?,
                        workspace_id: row.get(6)?,
                        saved: row.get::<_, i64>(7)? != 0,
                    })
                };
                let rows = if filter {
                    stmt.query_map(params![workspace_id, limit], map_row)
                } else {
                    stmt.query_map(params![limit], map_row)
                };
                if let Ok(rows) = rows {
                    for item in rows.flatten() {
                        list.push(item);
                    }
                }
            }
        }
        list
    }

    pub fn clear_search_history(&self, workspace_id: Option<&str>) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM search_history WHERE (?1 IS NULL OR workspace_id = ?1)",
            params![workspace_id],
        )?;
        Ok(())
    }

    pub fn delete_search_history_item(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM search_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_search_saved(&self, id: &str, saved: bool) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE search_history SET saved = ?1 WHERE id = ?2",
            params![saved as i64, id],
        )?;
        Ok(())
    }

    // Download history
    pub fn add_download_record(&self, record: &DownloadRecord) {
        {
            let conn = self.conn();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO download_history (id, paper_id, title, pdf_url, local_path, file_size_bytes, source, year, downloaded_at, workspace_id) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    record.id,
                    record.paper_id,
                    record.title,
                    record.pdf_url,
                    record.local_path,
                    record.file_size_bytes,
                    record.source,
                    record.year,
                    record.downloaded_at,
                    record.workspace_id
                ],
            );
        }
    }

    pub fn get_download_history(&self, workspace_id: Option<&str>) -> Vec<DownloadRecord> {
        let mut list = Vec::new();
        {
            let conn = self.conn();
            let (sql, filter) = match workspace_id {
                Some(_) => (
                    "SELECT id, paper_id, title, pdf_url, local_path, file_size_bytes, source, year, downloaded_at, workspace_id FROM download_history WHERE workspace_id = ?1 ORDER BY downloaded_at DESC",
                    true,
                ),
                None => (
                    "SELECT id, paper_id, title, pdf_url, local_path, file_size_bytes, source, year, downloaded_at, workspace_id FROM download_history ORDER BY downloaded_at DESC",
                    false,
                ),
            };
            let prepared = conn.prepare(sql);
            if let Ok(mut stmt) = prepared {
                let map_row = |row: &rusqlite::Row<'_>| {
                    Ok(DownloadRecord {
                        id: row.get(0)?,
                        paper_id: row.get(1)?,
                        title: row.get(2)?,
                        pdf_url: row.get(3)?,
                        local_path: row.get(4)?,
                        file_size_bytes: row.get(5)?,
                        source: row.get(6)?,
                        year: row.get(7)?,
                        downloaded_at: row.get(8)?,
                        workspace_id: row.get(9)?,
                    })
                };
                let rows = if filter {
                    stmt.query_map(params![workspace_id], map_row)
                } else {
                    stmt.query_map([], map_row)
                };
                if let Ok(rows) = rows {
                    for item in rows.flatten() {
                        list.push(item);
                    }
                }
            }
        }
        list
    }

    pub fn get_download_record(&self, id: &str) -> Option<DownloadRecord> {
        self.conn()
            .query_row(
                "SELECT id,paper_id,title,pdf_url,local_path,file_size_bytes,source,year,downloaded_at,workspace_id FROM download_history WHERE id=?1",
                params![id],
                |row| {
                    Ok(DownloadRecord {
                        id: row.get(0)?,
                        paper_id: row.get(1)?,
                        title: row.get(2)?,
                        pdf_url: row.get(3)?,
                        local_path: row.get(4)?,
                        file_size_bytes: row.get(5)?,
                        source: row.get(6)?,
                        year: row.get(7)?,
                        downloaded_at: row.get(8)?,
                        workspace_id: row.get(9)?,
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn delete_download_record(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM download_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Forgets download records, optionally only those of one workspace.
    ///
    /// Records only — the PDFs on disk are the user's files and are left alone,
    /// which is what the single-record delete already promises.
    pub fn clear_download_history(&self, workspace_id: Option<&str>) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM download_history WHERE (?1 IS NULL OR workspace_id = ?1)",
            params![workspace_id],
        )?;
        Ok(())
    }

    // App config
    pub fn get_config(&self, key: &str) -> Option<String> {
        if self.keychain && crate::config::is_secret(key) {
            if let Some(value) = crate::secrets::read(key) {
                return Some(value);
            }
        }
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT value FROM app_config WHERE key = ?1")
            .ok()?;
        stmt.query_row(params![key], |row| row.get(0)).ok()
    }

    #[cfg(test)]
    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO app_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_config_patch(
        &self,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<()> {
        let mut conn = self.conn();
        if values
            .get("mcp_auth_token")
            .is_some_and(|value| value.is_empty())
        {
            let active: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_credentials WHERE revoked=0)",
                [],
                |r| r.get(0),
            )?;
            if active {
                return Err(rusqlite::Error::InvalidParameterName(
                    "Revoke active agent connections before disabling the administrator token"
                        .into(),
                ));
            }
        }
        let transaction = conn.transaction()?;
        for (key, value) in values {
            // Route secrets to the OS keychain; keep them out of the SQLite file.
            if self.keychain && crate::config::is_secret(key) {
                if value.is_empty() {
                    crate::secrets::delete(key);
                    transaction.execute("DELETE FROM app_config WHERE key = ?1", params![key])?;
                    continue;
                }
                if crate::secrets::write(key, value) {
                    transaction.execute("DELETE FROM app_config WHERE key = ?1", params![key])?;
                    continue;
                }
                // Keychain unavailable: fall through and keep the value in SQLite.
            }
            transaction.execute(
                "INSERT OR REPLACE INTO app_config (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        transaction.commit()
    }

    pub fn get_all_config(&self) -> serde_json::Value {
        let conn = self.conn();
        let mut stmt = match conn.prepare("SELECT key, value FROM app_config") {
            Ok(s) => s,
            Err(_) => return serde_json::json!({}),
        };
        let mut map = serde_json::Map::new();
        let rows = stmt.query_map([], |row| {
            let k: String = row.get(0)?;
            let v: String = row.get(1)?;
            Ok((k, v))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                // Config values are strings. Parsing numeric-looking API keys corrupts them.
                map.insert(row.0, serde_json::Value::String(row.1));
            }
        }
        // Secrets moved to the keychain take precedence over any legacy copy.
        if self.keychain {
            for key in crate::config::SECRET_KEYS {
                if let Some(value) = crate::secrets::read(key) {
                    map.insert((*key).to_string(), serde_json::Value::String(value));
                }
            }
        }
        serde_json::Value::Object(map)
    }

    pub fn clear_cache(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM search_cache", [])?;
        Ok(())
    }

    pub fn get_telemetry_stats(&self, port: u16) -> TelemetryStats {
        let mut logs = Vec::new();
        let mut total_queries = 0;
        let mut sum_latency = 0;
        let cache_hit_rate;

        {
            let conn = self.conn();
            if let Ok(mut stmt) = conn.prepare("SELECT id, timestamp, agent_name, method, query, result_count, latency_ms, status FROM agent_logs ORDER BY rowid DESC LIMIT 15") {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok(AgentLog {
                        id: row.get(0)?,
                        timestamp: row.get(1)?,
                        agent_name: row.get(2)?,
                        method: row.get(3)?,
                        query: row.get(4)?,
                        result_count: row.get(5)?,
                        latency_ms: row.get(6)?,
                        status: row.get(7)?,
                    })
                }) {
                    for log in rows.flatten() {
                        sum_latency += log.latency_ms;
                        logs.push(log);
                    }
                }
            }

            if let Ok(count) = conn.query_row("SELECT COUNT(*) FROM agent_logs", [], |r| {
                r.get::<_, usize>(0)
            }) {
                total_queries = count;
            }
            cache_hit_rate = conn.query_row(
                "SELECT COALESCE(100.0 * SUM(CASE WHEN status = 'Cache Hit (200 OK)' THEN 1 ELSE 0 END) / NULLIF(COUNT(*), 0), 0.0) FROM agent_logs WHERE method = 'POST /api/search'",
                [], |row| row.get::<_, f64>(0),
            ).unwrap_or(0.0);
        }

        let avg_latency = if !logs.is_empty() {
            sum_latency / logs.len() as u64
        } else {
            0
        };

        TelemetryStats {
            gateway_status: "ONLINE".to_string(),
            port,
            total_queries,
            cache_hit_rate,
            avg_latency_ms: avg_latency,
            recent_logs: logs,
        }
    }
}
