//! Journal quartiles (Q1–Q4) from the SCImago Journal Rank (SJR) table.
//!
//! No open API serves journal quartiles, but SCImago publishes its whole
//! ranking as a free CSV. The user imports it once (or lets the app download
//! it), and every paper whose ISSN or journal title matches a ranked journal
//! gets its best quartile. Nothing is guessed: an unmatched journal stays blank.

use crate::models::Paper;
use rusqlite::{params, Connection, OptionalExtension};

/// SCImago's own export link; it serves the latest year as a `;`-separated CSV.
pub const SJR_DOWNLOAD_URL: &str = "https://www.scimagojr.com/journalrank.php?out=xls";

pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS journal_rankings (
            issn TEXT,
            title_key TEXT NOT NULL,
            title TEXT NOT NULL,
            quartile TEXT NOT NULL,
            sjr REAL,
            categories TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_journal_rankings_issn ON journal_rankings(issn);
        CREATE INDEX IF NOT EXISTS idx_journal_rankings_title ON journal_rankings(title_key);",
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalRanking {
    pub title: String,
    pub issns: Vec<String>,
    pub quartile: String,
    pub sjr: Option<f64>,
    pub categories: Option<String>,
}

/// "1542-4863" / "15424863" / "1542486x" → "15424863" / "1542486X".
pub fn normalize_issn(value: &str) -> Option<String> {
    let cleaned: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (cleaned.len() == 8 && cleaned[..7].chars().all(|c| c.is_ascii_digit())).then_some(cleaned)
}

/// Lower-case alphanumerics only, with a leading "the" dropped, so
/// "The Lancet" and "LANCET" meet on the same key.
pub fn title_key(title: &str) -> String {
    let lower = title.trim().to_lowercase();
    let lower = lower.strip_prefix("the ").unwrap_or(&lower);
    lower.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Splits one `;`-separated CSV line, honouring double-quoted fields.
fn split_csv_line(line: &str, delimiter: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == delimiter && !quoted => fields.push(std::mem::take(&mut current)),
            c => current.push(c),
        }
    }
    fields.push(current);
    fields
}

/// Parses SCImago's export. Columns are found by header name, so a reordered
/// or extended export still reads correctly. Returns the rankings and the
/// data year when the header carries one (e.g. "Total Docs. (2024)").
pub fn parse_sjr_csv(text: &str) -> Result<(Vec<JournalRanking>, Option<u32>), String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let header_line = lines.next().ok_or("The file is empty")?;
    let delimiter = if header_line.matches(';').count() >= header_line.matches(',').count() {
        ';'
    } else {
        ','
    };
    let header: Vec<String> = split_csv_line(header_line, delimiter)
        .into_iter()
        .map(|h| h.trim().to_string())
        .collect();
    let column = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));
    let (Some(title_col), Some(quartile_col)) = (column("Title"), column("SJR Best Quartile")) else {
        return Err("This does not look like a SCImago journal ranking export (missing Title / SJR Best Quartile columns)".into());
    };
    let issn_col = column("Issn");
    let sjr_col = column("SJR");
    let categories_col = column("Categories");
    let year = header.iter().find_map(|h| {
        let start = h.find("Total Docs. (")? + "Total Docs. (".len();
        h[start..].split(')').next()?.parse::<u32>().ok()
    });

    let mut rankings = Vec::new();
    for line in lines {
        let fields = split_csv_line(line, delimiter);
        let get = |index: Option<usize>| index.and_then(|i| fields.get(i)).map(|v| v.trim().to_string());
        let quartile = get(Some(quartile_col)).unwrap_or_default().to_uppercase();
        if !matches!(quartile.as_str(), "Q1" | "Q2" | "Q3" | "Q4") {
            continue;
        }
        let title = get(Some(title_col)).unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let issns = get(issn_col)
            .unwrap_or_default()
            .split(',')
            .filter_map(normalize_issn)
            .collect();
        rankings.push(JournalRanking {
            title,
            issns,
            quartile,
            sjr: get(sjr_col).and_then(|v| v.replace(',', ".").parse().ok()),
            categories: get(categories_col).filter(|v| !v.is_empty()),
        });
    }
    if rankings.is_empty() {
        return Err("No ranked journals were found in the file".into());
    }
    Ok((rankings, year))
}

/// Replaces the stored ranking table with `rankings`.
pub fn replace_all(conn: &mut Connection, rankings: &[JournalRanking]) -> Result<usize, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM journal_rankings", []).map_err(|e| e.to_string())?;
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO journal_rankings (issn, title_key, title, quartile, sjr, categories)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| e.to_string())?;
        for ranking in rankings {
            let key = title_key(&ranking.title);
            let issns: Vec<Option<&str>> = if ranking.issns.is_empty() {
                vec![None]
            } else {
                ranking.issns.iter().map(|i| Some(i.as_str())).collect()
            };
            for issn in issns {
                insert
                    .execute(params![issn, key, ranking.title, ranking.quartile, ranking.sjr, ranking.categories])
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(rankings.len())
}

pub fn count(conn: &Connection) -> usize {
    conn.query_row("SELECT COUNT(DISTINCT title_key) FROM journal_rankings", [], |row| row.get(0))
        .unwrap_or(0)
}

/// Best quartile for a journal, by ISSN first (exact) and journal title second.
pub fn lookup(conn: &Connection, issn: Option<&str>, venue: Option<&str>) -> Option<String> {
    if let Some(issn) = issn.and_then(normalize_issn) {
        let found = conn
            .query_row(
                "SELECT quartile FROM journal_rankings WHERE issn = ?1 LIMIT 1",
                [&issn],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten();
        if found.is_some() {
            return found;
        }
    }
    let key = title_key(venue?);
    if key.len() < 4 {
        return None;
    }
    conn.query_row(
        "SELECT quartile FROM journal_rankings WHERE title_key = ?1 ORDER BY quartile LIMIT 1",
        [&key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// Fills `quartile` on every paper that has none yet and matches a ranked journal.
pub fn annotate(conn: &Connection, papers: &mut [Paper]) {
    // An empty table is the common case until the user imports rankings.
    let has_rankings: bool = conn
        .query_row("SELECT EXISTS(SELECT 1 FROM journal_rankings)", [], |row| row.get(0))
        .unwrap_or(false);
    if !has_rankings {
        return;
    }
    for paper in papers.iter_mut().filter(|p| p.quartile.is_none()) {
        let issn = paper.biblio.as_ref().and_then(|b| b.issn.as_deref());
        paper.quartile = lookup(conn, issn, paper.venue.as_deref());
    }
}


// ---- HTTP -----------------------------------------------------------------

use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

fn status_json(state: &crate::server::AppState) -> Value {
    let journals = count(&state.db.conn());
    json!({
        "journals": journals,
        "year": state.db.get_config("sjr_year").and_then(|y| y.parse::<u32>().ok()),
        "imported_at": state.db.get_config("sjr_imported_at").and_then(|t| t.parse::<u64>().ok()),
        "source": "SCImago Journal Rank (SJR)",
        "download_url": SJR_DOWNLOAD_URL,
    })
}

fn store(state: &crate::server::AppState, text: &str) -> (StatusCode, Json<Value>) {
    let (rankings, year) = match parse_sjr_csv(text) {
        Ok(parsed) => parsed,
        Err(error) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))),
    };
    if let Err(error) = replace_all(&mut state.db.conn(), &rankings) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": error })));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = state.db.set_config("sjr_imported_at", &now.to_string());
    let _ = state.db.set_config("sjr_year", &year.map(|y| y.to_string()).unwrap_or_default());
    (StatusCode::OK, Json(status_json(state)))
}

pub async fn status_handler(State(state): State<crate::server::AppState>) -> Json<Value> {
    Json(status_json(&state))
}

#[derive(serde::Deserialize)]
pub struct ImportRequest {
    csv: String,
}

/// Imports a SCImago CSV the user downloaded themselves.
pub async fn import_handler(
    State(state): State<crate::server::AppState>,
    Json(payload): Json<ImportRequest>,
) -> (StatusCode, Json<Value>) {
    store(&state, &payload.csv)
}

/// Downloads the current SCImago ranking and imports it.
pub async fn update_handler(State(state): State<crate::server::AppState>) -> (StatusCode, Json<Value>) {
    let client = crate::engine::pooled_client(120, crate::config::outbound_proxy(&state.db).as_deref());
    let response = match client.get(SJR_DOWNLOAD_URL).send().await {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("SCImago returned HTTP {}. Download the CSV from scimagojr.com and import it instead.", response.status().as_u16()) })),
            )
        }
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Could not reach SCImago ({error}). Download the CSV from scimagojr.com and import it instead.") })),
            )
        }
    };
    match response.text().await {
        Ok(text) => store(&state, &text),
        Err(error) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": error.to_string() }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Rank;Sourceid;Title;Type;Issn;SJR;SJR Best Quartile;H index;Total Docs. (2024);Categories\n\
1;28773;\"Ca-A Cancer Journal for Clinicians\";journal;\"15424863, 00079235\";145,004;Q1;223;43;\"Hematology (Q1); Oncology (Q1)\"\n\
2;1;\"The Lancet\";journal;\"01406736, 1474547X\";12,1;Q1;800;900;Medicine (Q1)\n\
3;2;\"Tạp chí Y học Việt Nam\";journal;\"18591868\";0,1;Q4;3;100;Medicine (Q4)\n\
4;3;\"Unranked Journal\";journal;\"12345678\";0;-;1;1;Medicine\n";

    #[test]
    fn parses_scimago_export_and_matches_by_issn_or_title() {
        let (rankings, year) = parse_sjr_csv(SAMPLE).unwrap();
        assert_eq!(year, Some(2024));
        assert_eq!(rankings.len(), 3, "rows without a quartile are skipped");
        assert_eq!(rankings[0].issns, vec!["15424863", "00079235"]);
        assert_eq!(rankings[0].sjr, Some(145.004));

        let mut conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        assert_eq!(replace_all(&mut conn, &rankings).unwrap(), 3);
        assert_eq!(count(&conn), 3);
        assert_eq!(lookup(&conn, Some("1474-547x"), None).as_deref(), Some("Q1"));
        assert_eq!(lookup(&conn, None, Some("LANCET")).as_deref(), Some("Q1"));
        assert_eq!(lookup(&conn, None, Some("Tạp chí Y học Việt Nam")).as_deref(), Some("Q4"));
        assert_eq!(lookup(&conn, Some("12345678"), Some("Unranked Journal")), None);
    }

    #[test]
    fn annotate_leaves_existing_quartiles_and_unknown_journals_alone() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        replace_all(&mut conn, &parse_sjr_csv(SAMPLE).unwrap().0).unwrap();
        let paper = |venue: &str, quartile: Option<&str>| Paper {
            id: venue.into(),
            title: "t".into(),
            authors: vec![],
            year: None,
            venue: Some(venue.into()),
            abstract_text: None,
            doi: None,
            source_url: None,
            pdf_url: None,
            citations: None,
            quartile: quartile.map(str::to_string),
            source: "test".into(),
            score: None,
            open_access: false,
            biblio: None,
        };
        let mut papers = vec![paper("The Lancet", None), paper("Nowhere", None), paper("The Lancet", Some("Q2"))];
        annotate(&conn, &mut papers);
        assert_eq!(papers[0].quartile.as_deref(), Some("Q1"));
        assert_eq!(papers[1].quartile, None);
        assert_eq!(papers[2].quartile.as_deref(), Some("Q2"));
    }

    #[test]
    fn rejects_files_that_are_not_rankings() {
        assert!(parse_sjr_csv("a,b,c\n1,2,3").is_err());
        assert!(parse_sjr_csv("").is_err());
    }
}
