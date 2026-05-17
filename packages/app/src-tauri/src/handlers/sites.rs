use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::handlers::apps::normalize_style;
use crate::handlers::browser::normalize_domain;
use crate::handlers::db::DbState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDictionaryWord {
    pub id: String,
    pub word: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDictionary {
    pub domain: String,
    pub words: Vec<SiteDictionaryWord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteStyle {
    pub id: String,
    pub domain: String,
    pub style: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn run_migrations(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS site_dictionary_words (
            id TEXT PRIMARY KEY,
            domain TEXT NOT NULL,
            word TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_site_dictionary_words_domain ON site_dictionary_words(domain);
        CREATE TABLE IF NOT EXISTS site_styles (
            id TEXT PRIMARY KEY,
            domain TEXT NOT NULL UNIQUE,
            style TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_site_styles_domain ON site_styles(domain);",
    )
    .map_err(|e| format!("Failed to create site tables: {}", e))?;
    Ok(())
}

pub fn fetch_site_dictionary_words(
    conn: &rusqlite::Connection,
    domain: &str,
) -> Option<String> {
    let mut stmt = conn
        .prepare(
            "SELECT word FROM site_dictionary_words WHERE domain = ?1 \
             ORDER BY created_at DESC LIMIT 50",
        )
        .ok()?;
    let words: Vec<String> = stmt
        .query_map(params![domain], |row| row.get::<_, String>(0))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(", "))
    }
}

fn validate_domain(input: &str) -> Result<String, String> {
    normalize_domain(input).ok_or_else(|| "Invalid domain".to_string())
}

fn validate_word(input: &str) -> Result<&str, String> {
    let word = input.trim();
    if word.is_empty() || word.len() > 100 {
        Err("Word must be between 1 and 100 characters".to_string())
    } else {
        Ok(word)
    }
}

#[tauri::command]
pub fn list_site_dictionaries(
    state: tauri::State<'_, DbState>,
) -> Result<Vec<SiteDictionary>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, domain, word, created_at, updated_at \
             FROM site_dictionary_words \
             ORDER BY domain COLLATE NOCASE ASC, created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut grouped: Vec<SiteDictionary> = Vec::new();
    for (id, domain, word, created_at, updated_at) in rows {
        let entry = SiteDictionaryWord {
            id,
            word,
            created_at,
            updated_at,
        };
        match grouped.iter_mut().find(|g| g.domain == domain) {
            Some(group) => group.words.push(entry),
            None => grouped.push(SiteDictionary {
                domain,
                words: vec![entry],
            }),
        }
    }
    Ok(grouped)
}

#[tauri::command]
pub fn add_site_dictionary_word(
    state: tauri::State<'_, DbState>,
    domain: String,
    word: String,
) -> Result<SiteDictionaryWord, String> {
    let domain = validate_domain(&domain)?;
    let word = validate_word(&word)?;

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO site_dictionary_words (id, domain, word, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, domain, word, now, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(SiteDictionaryWord {
        id,
        word: word.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub fn update_site_dictionary_word(
    state: tauri::State<'_, DbState>,
    id: String,
    word: String,
) -> Result<SiteDictionaryWord, String> {
    let word = validate_word(&word)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let rows_affected = conn
        .execute(
            "UPDATE site_dictionary_words SET word = ?1, updated_at = ?2 WHERE id = ?3",
            params![word, now, id],
        )
        .map_err(|e| e.to_string())?;
    if rows_affected == 0 {
        return Err("Word not found".to_string());
    }
    Ok(SiteDictionaryWord {
        id,
        word: word.to_string(),
        created_at: String::new(),
        updated_at: now,
    })
}

#[tauri::command]
pub fn delete_site_dictionary_word(
    state: tauri::State<'_, DbState>,
    id: String,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM site_dictionary_words WHERE id = ?1",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_site_dictionary(
    state: tauri::State<'_, DbState>,
    domain: String,
) -> Result<(), String> {
    let domain = validate_domain(&domain)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM site_dictionary_words WHERE domain = ?1",
        params![domain],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_site_styles(state: tauri::State<'_, DbState>) -> Result<Vec<SiteStyle>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, domain, style, created_at, updated_at \
             FROM site_styles ORDER BY domain COLLATE NOCASE ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SiteStyle {
                id: row.get(0)?,
                domain: row.get(1)?,
                style: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub fn set_site_style(
    state: tauri::State<'_, DbState>,
    domain: String,
    style: String,
) -> Result<SiteStyle, String> {
    let domain = validate_domain(&domain)?;
    let style = normalize_style(style.as_str())?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();

    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT id, created_at FROM site_styles WHERE domain = ?1",
            params![domain],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let (id, created_at) = match existing {
        Some((id, created_at)) => {
            conn.execute(
                "UPDATE site_styles SET style = ?1, updated_at = ?2 WHERE id = ?3",
                params![style, now, id],
            )
            .map_err(|e| e.to_string())?;
            (id, created_at)
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO site_styles (id, domain, style, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, domain, style, now, now],
            )
            .map_err(|e| e.to_string())?;
            (id, now.clone())
        }
    };

    Ok(SiteStyle {
        id,
        domain,
        style: style.to_string(),
        created_at,
        updated_at: now,
    })
}

#[tauri::command]
pub fn delete_site_style(state: tauri::State<'_, DbState>, id: String) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM site_styles WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
