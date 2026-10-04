use chrono::Utc;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::backend::{DictionaryWord, Transcription, TranscriptionPage, TranscriptionResult};

pub struct DbState {
    pub conn: Mutex<Connection>,
}

fn get_db_path() -> Result<PathBuf, String> {
    let data_dir = crate::paths::data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("voxfusion.db"))
}

pub fn init_db() -> Result<DbState, String> {
    let db_path = get_db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;",
    )
    .map_err(|e| e.to_string())?;

    run_migrations(&conn)?;

    Ok(DbState {
        conn: Mutex::new(conn),
    })
}

fn run_migrations(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS transcriptions (
            id TEXT PRIMARY KEY,
            text TEXT NOT NULL,
            word_count INTEGER NOT NULL DEFAULT 0,
            processing_time_ms INTEGER NOT NULL DEFAULT 0,
            audio_duration_ms INTEGER,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS dictionary_words (
            id TEXT PRIMARY KEY,
            word TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_transcriptions_created_at ON transcriptions(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_dictionary_words_word ON dictionary_words(word);",
    )
    .map_err(|e| format!("Failed to create tables: {}", e))?;

    super::apps::run_migrations(conn)?;
    super::sites::run_migrations(conn)?;

    Ok(())
}

pub fn save_transcription(
    state: &DbState,
    result: &TranscriptionResult,
) -> Result<Transcription, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO transcriptions (id, text, word_count, processing_time_ms, audio_duration_ms, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, result.text, result.word_count, result.processing_time_ms, result.audio_duration_ms, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(Transcription {
        id,
        text: result.text.clone(),
        word_count: result.word_count,
        processing_time_ms: result.processing_time_ms,
        audio_duration_ms: result.audio_duration_ms,
        created_at: now,
    })
}

/// A page of transcriptions, newest first, older than `cursor` (the
/// `created_at` of the last one already listed).
pub fn list_transcriptions(
    state: &DbState,
    limit: i64,
    cursor: Option<String>,
) -> Result<TranscriptionPage, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    // SQLite reads a negative limit as no limit at all.
    let limit = limit.max(0);
    let fetch_limit = limit + 1;

    let rows = if let Some(cursor) = cursor {
        let mut stmt = conn
            .prepare(
                "SELECT id, text, word_count, processing_time_ms, audio_duration_ms, created_at \
                 FROM transcriptions WHERE created_at < ?1 ORDER BY created_at DESC LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_map(params![cursor, fetch_limit], |row| {
            Ok(Transcription {
                id: row.get(0)?,
                text: row.get(1)?,
                word_count: row.get(2)?,
                processing_time_ms: row.get(3)?,
                audio_duration_ms: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, text, word_count, processing_time_ms, audio_duration_ms, created_at \
                 FROM transcriptions ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_map(params![fetch_limit], |row| {
            Ok(Transcription {
                id: row.get(0)?,
                text: row.get(1)?,
                word_count: row.get(2)?,
                processing_time_ms: row.get(3)?,
                audio_duration_ms: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    };

    let limit = limit as usize;
    let has_more = rows.len() > limit;
    let transcriptions: Vec<Transcription> = rows.into_iter().take(limit).collect();

    Ok(TranscriptionPage {
        transcriptions,
        has_more,
    })
}

pub fn list_dictionary_words(state: &DbState) -> Result<Vec<DictionaryWord>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, word, created_at, updated_at FROM dictionary_words ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let words = stmt
        .query_map([], |row| {
            Ok(DictionaryWord {
                id: row.get(0)?,
                word: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(words)
}

pub fn add_dictionary_word(state: &DbState, word: &str) -> Result<DictionaryWord, String> {
    let word = word.trim();
    if word.is_empty() || word.len() > 100 {
        return Err("Word must be between 1 and 100 characters".to_string());
    }

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO dictionary_words (id, word, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, word, now, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(DictionaryWord {
        id,
        word: word.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update_dictionary_word(
    state: &DbState,
    id: &str,
    word: &str,
) -> Result<DictionaryWord, String> {
    let word = word.trim();
    if word.is_empty() || word.len() > 100 {
        return Err("Word must be between 1 and 100 characters".to_string());
    }

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();

    let rows_affected = conn
        .execute(
            "UPDATE dictionary_words SET word = ?1, updated_at = ?2 WHERE id = ?3",
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

pub fn delete_dictionary_word(state: &DbState, id: &str) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM dictionary_words WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
impl DbState {
    /// A fresh database that lives as long as the value.
    pub fn in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("an in-memory database opens");
        run_migrations(&conn).expect("migrations apply");

        Self {
            conn: Mutex::new(conn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_transcription(state: &DbState, id: &str, created_at: &str) {
        state
            .conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO transcriptions (id, text, created_at) VALUES (?1, ?1, ?2)",
                params![id, created_at],
            )
            .unwrap();
    }

    fn ids(page: &TranscriptionPage) -> Vec<&str> {
        page.transcriptions
            .iter()
            .map(|transcription| transcription.id.as_str())
            .collect()
    }

    #[test]
    fn transcriptions_are_paged_newest_first() {
        let state = DbState::in_memory();
        insert_transcription(&state, "first", "2026-01-01T10:00:00+00:00");
        insert_transcription(&state, "second", "2026-01-02T10:00:00+00:00");
        insert_transcription(&state, "third", "2026-01-03T10:00:00+00:00");

        let page = list_transcriptions(&state, 2, None).unwrap();
        assert_eq!(ids(&page), ["third", "second"]);
        assert!(page.has_more);

        let cursor = page.transcriptions[1].created_at.clone();
        let page = list_transcriptions(&state, 2, Some(cursor)).unwrap();
        assert_eq!(ids(&page), ["first"]);
        assert!(!page.has_more);

        let page = list_transcriptions(&state, 3, None).unwrap();
        assert_eq!(ids(&page), ["third", "second", "first"]);
        assert!(!page.has_more);
    }

    #[test]
    fn a_negative_limit_lists_nothing() {
        let state = DbState::in_memory();
        insert_transcription(&state, "only", "2026-01-01T10:00:00+00:00");

        let page = list_transcriptions(&state, -1, None).unwrap();
        assert!(page.transcriptions.is_empty());
        assert!(page.has_more);
    }

    #[test]
    fn a_saved_transcription_is_listed_as_it_was_saved() {
        let state = DbState::in_memory();
        let result = TranscriptionResult {
            text: "Hello there".into(),
            word_count: 2,
            processing_time_ms: 420,
            audio_duration_ms: Some(1800),
        };

        let saved = save_transcription(&state, &result).unwrap();
        assert_eq!(saved.text, "Hello there");
        assert_eq!(saved.word_count, 2);
        assert_eq!(saved.processing_time_ms, 420);
        assert_eq!(saved.audio_duration_ms, Some(1800));

        let page = list_transcriptions(&state, 20, None).unwrap();
        assert_eq!(page.transcriptions, [saved]);
    }

    #[test]
    fn dictionary_words_are_trimmed_validated_and_editable() {
        let state = DbState::in_memory();

        let added = add_dictionary_word(&state, "  VoxFusion ").unwrap();
        assert_eq!(added.word, "VoxFusion");
        assert_eq!(list_dictionary_words(&state).unwrap(), [added.clone()]);

        let updated = update_dictionary_word(&state, &added.id, "Vox Fusion").unwrap();
        assert_eq!(updated.id, added.id);
        assert_eq!(list_dictionary_words(&state).unwrap()[0].word, "Vox Fusion");

        assert!(add_dictionary_word(&state, "   ").is_err());
        assert!(add_dictionary_word(&state, &"x".repeat(101)).is_err());
        assert_eq!(
            update_dictionary_word(&state, "missing", "word").unwrap_err(),
            "Word not found"
        );

        delete_dictionary_word(&state, &added.id).unwrap();
        assert!(list_dictionary_words(&state).unwrap().is_empty());
    }
}
