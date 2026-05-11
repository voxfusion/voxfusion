use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub id: String,
    pub text: String,
    pub word_count: i64,
    pub processing_time_ms: i64,
    pub audio_duration_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryWord {
    pub id: String,
    pub word: String,
    pub created_at: String,
    pub updated_at: String,
}

fn get_db_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("voxfusion.db"))
}

pub fn init_db(app_handle: &tauri::AppHandle) -> Result<DbState, String> {
    let db_path = get_db_path(app_handle)?;
    let conn =
        Connection::open(&db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;",
    )
    .map_err(|e| e.to_string())?;

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

    Ok(DbState {
        conn: Mutex::new(conn),
    })
}

#[tauri::command]
pub fn save_transcription(
    state: tauri::State<'_, DbState>,
    text: String,
    word_count: i64,
    processing_time_ms: i64,
    audio_duration_ms: Option<i64>,
) -> Result<Transcription, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO transcriptions (id, text, word_count, processing_time_ms, audio_duration_ms, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, text, word_count, processing_time_ms, audio_duration_ms, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(Transcription {
        id,
        text,
        word_count,
        processing_time_ms,
        audio_duration_ms,
        created_at: now,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionPage {
    pub transcriptions: Vec<Transcription>,
    pub has_more: bool,
}

#[tauri::command]
pub fn list_transcriptions(
    state: tauri::State<'_, DbState>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<TranscriptionPage, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(20);
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

    let has_more = rows.len() > limit as usize;
    let transcriptions: Vec<Transcription> = rows.into_iter().take(limit as usize).collect();

    Ok(TranscriptionPage {
        transcriptions,
        has_more,
    })
}

#[tauri::command]
pub fn list_dictionary_words(
    state: tauri::State<'_, DbState>,
) -> Result<Vec<DictionaryWord>, String> {
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

#[tauri::command]
pub fn add_dictionary_word(
    state: tauri::State<'_, DbState>,
    word: String,
) -> Result<DictionaryWord, String> {
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

#[tauri::command]
pub fn update_dictionary_word(
    state: tauri::State<'_, DbState>,
    id: String,
    word: String,
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
        id,
        word: word.to_string(),
        created_at: String::new(),
        updated_at: now,
    })
}

#[tauri::command]
pub fn delete_dictionary_word(
    state: tauri::State<'_, DbState>,
    id: String,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM dictionary_words WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_dictionary_prompt(
    state: tauri::State<'_, DbState>,
) -> Result<Option<String>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT word FROM dictionary_words ORDER BY created_at DESC LIMIT 50",
        )
        .map_err(|e| e.to_string())?;

    let words: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    if words.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!("Specialized terms: {}", words.join(", "))))
    }
}
