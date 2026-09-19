//! Portable research-data backups. Secrets and PDF binaries are deliberately excluded.
use crate::{
    db::Database,
    models::{DownloadRecord, Paper, SearchHistoryItem, Workspace},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};

const FORMAT: &str = "scholargate-backup";
const VERSION: u32 = 1;
const MAX_BACKUP_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct SavedPaper {
    paper: Paper,
    saved_at: u64,
}

#[derive(Serialize, Deserialize)]
struct WorkspacePaperLink {
    workspace_id: String,
    paper_id: String,
    note: Option<String>,
    added_at: u64,
    status: String,
    favorite: bool,
    tags: String,
}

#[derive(Serialize, Deserialize)]
struct ResearchBackup {
    format: String,
    version: u32,
    exported_at: u64,
    notice: String,
    workspaces: Vec<Workspace>,
    papers: Vec<SavedPaper>,
    workspace_papers: Vec<WorkspacePaperLink>,
    search_history: Vec<SearchHistoryItem>,
    download_history: Vec<DownloadRecord>,
}

fn rows<T>(
    mut statement: rusqlite::Statement<'_>,
    map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, String> {
    statement
        .query_map([], map)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

impl Database {
    fn research_backup(&self) -> Result<ResearchBackup, String> {
        let conn = self.conn();
        let workspaces = rows(conn.prepare(
            "SELECT id,name,description,created_at,updated_at,0,0 FROM workspaces ORDER BY created_at,id"
        ).map_err(|e| e.to_string())?, |row| Ok(Workspace {
            id: row.get(0)?, name: row.get(1)?, description: row.get(2)?,
            created_at: row.get(3)?, updated_at: row.get(4)?, paper_count: 0, query_count: 0,
        }))?;
        let papers = rows(
            conn.prepare("SELECT paper_json,saved_at FROM saved_papers ORDER BY saved_at,id")
                .map_err(|e| e.to_string())?,
            |row| {
                let json: String = row.get(0)?;
                let paper = serde_json::from_str(&json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(SavedPaper {
                    paper,
                    saved_at: row.get(1)?,
                })
            },
        )?;
        let workspace_papers = rows(conn.prepare(
            "SELECT workspace_id,paper_id,note,added_at,status,favorite,tags FROM workspace_papers ORDER BY workspace_id,paper_id"
        ).map_err(|e| e.to_string())?, |row| Ok(WorkspacePaperLink {
            workspace_id: row.get(0)?, paper_id: row.get(1)?, note: row.get(2)?,
            added_at: row.get(3)?, status: row.get(4)?, favorite: row.get::<_, i64>(5)? != 0,
            tags: row.get(6)?,
        }))?;
        let search_history = rows(conn.prepare(
            "SELECT id,query,sources,result_count,elapsed_ms,created_at,workspace_id,saved FROM search_history ORDER BY created_at,id"
        ).map_err(|e| e.to_string())?, |row| Ok(SearchHistoryItem {
            id: row.get(0)?, query: row.get(1)?, sources: row.get(2)?,
            result_count: row.get::<_, i64>(3)?.max(0) as usize, elapsed_ms: row.get(4)?,
            created_at: row.get(5)?, workspace_id: row.get(6)?, saved: row.get::<_, i64>(7)? != 0,
        }))?;
        let download_history = rows(conn.prepare(
            "SELECT id,paper_id,title,pdf_url,local_path,file_size_bytes,source,year,downloaded_at,workspace_id FROM download_history ORDER BY downloaded_at,id"
        ).map_err(|e| e.to_string())?, |row| Ok(DownloadRecord {
            id: row.get(0)?, paper_id: row.get(1)?, title: row.get(2)?, pdf_url: row.get(3)?,
            local_path: row.get(4)?, file_size_bytes: row.get(5)?, source: row.get(6)?,
            year: row.get(7)?, downloaded_at: row.get(8)?, workspace_id: row.get(9)?,
        }))?;
        Ok(ResearchBackup {
            format: FORMAT.into(), version: VERSION,
            exported_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            notice: "Research metadata only. API keys, tokens, sessions, agent credentials, cached responses and PDF files are excluded.".into(),
            workspaces, papers, workspace_papers, search_history, download_history,
        })
    }

    fn restore_research_backup(&self, backup: ResearchBackup) -> Result<(), String> {
        if backup.format != FORMAT || backup.version != VERSION {
            return Err("Unsupported ScholarGate backup format or version".into());
        }
        if backup.workspaces.len() > 10_000
            || backup.papers.len() > 1_000_000
            || backup.workspace_papers.len() > 2_000_000
            || backup.search_history.len() > 1_000_000
            || backup.download_history.len() > 1_000_000
        {
            return Err("Backup exceeds the supported record limits".into());
        }
        let workspace_ids: HashSet<&str> = backup
            .workspaces
            .iter()
            .map(|row| row.id.as_str())
            .collect();
        let paper_ids: HashSet<&str> = backup
            .papers
            .iter()
            .map(|row| row.paper.id.as_str())
            .collect();
        if workspace_ids.len() != backup.workspaces.len()
            || paper_ids.len() != backup.papers.len()
            || !workspace_ids.contains(crate::db::INTEREST_LIBRARY_ID)
            || backup
                .workspaces
                .iter()
                .any(|row| row.id.trim().is_empty() || row.name.trim().is_empty())
            || backup
                .papers
                .iter()
                .any(|row| row.paper.id.trim().is_empty() || row.paper.title.trim().is_empty())
            || backup.workspace_papers.iter().any(|row| {
                !workspace_ids.contains(row.workspace_id.as_str())
                    || !paper_ids.contains(row.paper_id.as_str())
            })
        {
            return Err("Backup contains invalid or duplicate research records".into());
        }

        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch(
            "DELETE FROM agent_credentials; DELETE FROM search_cache; DELETE FROM agent_logs;
             DELETE FROM download_history; DELETE FROM search_history; DELETE FROM workspace_papers;
             DELETE FROM saved_papers; DELETE FROM workspaces;",
        )
        .map_err(|e| e.to_string())?;
        for row in backup.workspaces {
            tx.execute("INSERT INTO workspaces(id,name,description,created_at,updated_at) VALUES(?1,?2,?3,?4,?5)",
                params![row.id,row.name,row.description,row.created_at,row.updated_at]).map_err(|e| e.to_string())?;
        }
        for row in backup.papers {
            let json = serde_json::to_string(&row.paper).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO saved_papers(id,title,paper_json,saved_at) VALUES(?1,?2,?3,?4)",
                params![row.paper.id, row.paper.title, json, row.saved_at],
            )
            .map_err(|e| e.to_string())?;
        }
        for row in backup.workspace_papers {
            tx.execute("INSERT INTO workspace_papers(workspace_id,paper_id,note,added_at,status,favorite,tags) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![row.workspace_id,row.paper_id,row.note,row.added_at,row.status,row.favorite,row.tags]).map_err(|e| e.to_string())?;
        }
        for row in backup.search_history {
            tx.execute("INSERT INTO search_history(id,query,sources,result_count,elapsed_ms,created_at,workspace_id,saved) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![row.id,row.query,row.sources,row.result_count as i64,row.elapsed_ms,row.created_at,row.workspace_id,row.saved]).map_err(|e| e.to_string())?;
        }
        for row in backup.download_history {
            tx.execute("INSERT INTO download_history(id,paper_id,title,pdf_url,local_path,file_size_bytes,source,year,downloaded_at,workspace_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![row.id,row.paper_id,row.title,row.pdf_url,row.local_path,row.file_size_bytes,row.source,row.year,row.downloaded_at,row.workspace_id]).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn export_backup(
    state: tauri::State<'_, crate::AppSharedState>,
) -> Result<Option<String>, String> {
    let backup = state.db.research_backup()?;
    let json = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let file_name = format!(
            "ScholarGate-backup-{}.json",
            chrono::Local::now().format("%Y-%m-%d")
        );
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save ScholarGate backup")
            .set_file_name(&file_name)
            .save_file()
        else {
            return Ok(None);
        };
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, json).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn restore_backup(
    state: tauri::State<'_, crate::AppSharedState>,
) -> Result<Option<String>, String> {
    let Some(path): Option<PathBuf> = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Restore ScholarGate backup")
            .add_filter("ScholarGate backup", &["json"])
            .pick_file()
    })
    .await
    .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if metadata.len() > MAX_BACKUP_BYTES {
        return Err("Backup file is larger than 100 MiB".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let backup: ResearchBackup = serde_json::from_slice(&bytes)
        .map_err(|_| "Backup is not valid ScholarGate JSON".to_string())?;
    state.db.restore_research_backup(backup)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_round_trip_restores_research_without_secrets() {
        let source = Database::in_memory().unwrap();
        let workspace = source.list_workspaces().remove(0);
        let paper: Paper = serde_json::from_value(serde_json::json!({
            "id":"doi:10.1/test","title":"Evidence","authors":["A"],"source":"crossref","open_access":false
        })).unwrap();
        source
            .add_workspace_paper(&workspace.id, &paper, Some("page 3"))
            .unwrap();
        source.set_config("openai_api_key", "secret").unwrap();
        let backup = source.research_backup().unwrap();

        let target = Database::in_memory().unwrap();
        target.restore_research_backup(backup).unwrap();
        assert_eq!(
            target.workspace_papers(&workspace.id).unwrap()[0]
                .note
                .as_deref(),
            Some("page 3")
        );
        assert!(target
            .get_config("openai_api_key")
            .unwrap_or_default()
            .is_empty());
    }
}
