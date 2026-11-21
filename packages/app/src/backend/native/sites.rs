use chrono::Utc;
use rusqlite::params;

use super::apps::normalize_style;
use super::db::DbState;
use crate::backend::{
    DictionaryWord, MAX_WORD_CHARS, SiteDictionary, SiteStyle, domain_lookup_groups,
    normalize_storage_domain,
};

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

pub fn fetch_site_dictionary_words(conn: &rusqlite::Connection, domain: &str) -> Option<String> {
    for group in domain_lookup_groups(domain) {
        let placeholders = std::iter::repeat_n("?", group.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT word FROM site_dictionary_words WHERE domain IN ({placeholders}) \
             ORDER BY created_at DESC LIMIT 50"
        );
        let Ok(mut stmt) = conn.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(group.iter()), |row| {
            row.get::<_, String>(0)
        }) else {
            continue;
        };
        let words: Vec<String> = rows.filter_map(|r| r.ok()).collect();
        if !words.is_empty() {
            return Some(words.join(", "));
        }
    }
    None
}

fn validate_domain(input: &str) -> Result<String, String> {
    normalize_storage_domain(input).ok_or_else(|| "Invalid domain".to_string())
}

/// Keeps the exact stored key when a site already exists in `table` (such as a
/// legacy Unicode entry), and validates and normalizes new site inputs.
fn resolve_storage_domain(
    conn: &rusqlite::Connection,
    table: &str,
    input: &str,
) -> Result<String, String> {
    if input.trim().is_empty() {
        return Err("Invalid domain".to_string());
    }
    let exists: bool = conn
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE domain = ?1)"),
            params![input],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists {
        Ok(input.to_string())
    } else {
        validate_domain(input)
    }
}

fn validate_word(input: &str) -> Result<&str, String> {
    let word = input.trim();
    if word.is_empty() || word.chars().count() > MAX_WORD_CHARS {
        Err(format!(
            "Word must be between 1 and {MAX_WORD_CHARS} characters"
        ))
    } else {
        Ok(word)
    }
}

pub fn list_site_dictionaries(state: &DbState) -> Result<Vec<SiteDictionary>, String> {
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
        let entry = DictionaryWord {
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

pub fn add_site_dictionary_word(
    state: &DbState,
    domain: &str,
    word: &str,
) -> Result<DictionaryWord, String> {
    let word = validate_word(word)?;

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let domain = resolve_storage_domain(&conn, "site_dictionary_words", domain)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO site_dictionary_words (id, domain, word, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, domain, word, now, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(DictionaryWord {
        id,
        word: word.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update_site_dictionary_word(
    state: &DbState,
    id: &str,
    word: &str,
) -> Result<DictionaryWord, String> {
    let word = validate_word(word)?;
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
    Ok(DictionaryWord {
        id: id.to_string(),
        word: word.to_string(),
        created_at: String::new(),
        updated_at: now,
    })
}

pub fn delete_site_dictionary_word(state: &DbState, id: &str) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM site_dictionary_words WHERE id = ?1",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_site_dictionary(state: &DbState, domain: &str) -> Result<(), String> {
    if domain.trim().is_empty() {
        return Err("Invalid domain".to_string());
    }
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    // Delete strictly by the exact stored key so repeating a delete for a
    // legacy entry (such as a Unicode domain) never removes a separate entry.
    conn.execute(
        "DELETE FROM site_dictionary_words WHERE domain = ?1",
        params![domain],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_site_styles(state: &DbState) -> Result<Vec<SiteStyle>, String> {
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

pub fn set_site_style(state: &DbState, domain: &str, style: &str) -> Result<SiteStyle, String> {
    let style = normalize_style(style)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let domain = resolve_storage_domain(&conn, "site_styles", domain)?;
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

pub fn delete_site_style(state: &DbState, id: &str) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM site_styles WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_dictionaries_are_keyed_by_normalized_domain() {
        let state = DbState::in_memory();
        let word = add_site_dictionary_word(&state, "https://www.GitHub.com/zed", "GPUI").unwrap();
        add_site_dictionary_word(&state, "github.com", "Rust").unwrap();

        let dictionaries = list_site_dictionaries(&state).unwrap();
        assert_eq!(dictionaries.len(), 1);
        assert_eq!(dictionaries[0].domain, "github.com");
        assert_eq!(dictionaries[0].words.len(), 2);

        assert!(add_site_dictionary_word(&state, "   ", "GPUI").is_err());
        assert!(add_site_dictionary_word(&state, "github.com", "").is_err());

        update_site_dictionary_word(&state, &word.id, "gpui-kit").unwrap();
        delete_site_dictionary_word(&state, &word.id).unwrap();
        assert_eq!(list_site_dictionaries(&state).unwrap()[0].words.len(), 1);

        delete_site_dictionary(&state, "github.com").unwrap();
        assert!(list_site_dictionaries(&state).unwrap().is_empty());
    }

    #[test]
    fn a_site_has_one_style_that_later_calls_replace() {
        let state = DbState::in_memory();

        let first = set_site_style(&state, "www.github.com", "agents").unwrap();
        let second = set_site_style(&state, "github.com", "casual").unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.domain, "github.com");

        let listed = list_site_styles(&state).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].style, "casual");

        assert!(set_site_style(&state, "github.com", "shouting").is_err());

        delete_site_style(&state, &first.id).unwrap();
        assert!(list_site_styles(&state).unwrap().is_empty());
    }
}
