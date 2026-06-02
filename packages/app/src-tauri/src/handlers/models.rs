use futures_util::StreamExt;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Speech-to-text engine that backs a model. Each engine has its own
/// inference path in [`crate::handlers::whisper::transcribe_audio`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Whisper,
    Parakeet,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Whisper => "whisper",
            Engine::Parakeet => "parakeet",
        }
    }
}

/// Static metadata describing a downloadable transcription model.
pub struct ModelInfo {
    /// Stable identifier used by the frontend and persisted as the active model.
    pub id: &'static str,
    pub name: &'static str,
    pub engine: Engine,
    /// Local filename inside the models directory.
    pub filename: &'static str,
    /// Direct (single-file) download URL.
    pub url: &'static str,
    /// Exact size in bytes when known; used for download validation and as a
    /// progress fallback when the server omits `Content-Length`.
    pub expected_size: Option<u64>,
    pub size_label: &'static str,
    pub languages: &'static str,
    /// Marks a model whose engine cannot transcribe yet. Such a model can be
    /// downloaded but cannot be made the active transcription model.
    pub experimental: bool,
    pub recommended: bool,
}

pub const DEFAULT_MODEL_ID: &str = "whisper-large-v3-turbo";

/// The model registry — the single source of truth for available models.
const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "whisper-large-v3-turbo",
        name: "Whisper Large v3 Turbo",
        engine: Engine::Whisper,
        filename: "ggml-large-v3-turbo.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        expected_size: Some(1_624_555_275),
        size_label: "~1.5 GB",
        languages: "99 languages",
        experimental: false,
        recommended: true,
    },
    ModelInfo {
        id: "parakeet-tdt-0.6b-v3",
        name: "Parakeet TDT 0.6B v3",
        engine: Engine::Parakeet,
        filename: "parakeet-tdt-0.6b-v3-q8_0.gguf",
        // GGUF port of NVIDIA's Parakeet TDT 0.6B v3 (q8_0), run via the crispasr
        // engine (see handlers::parakeet).
        url: "https://huggingface.co/cstr/parakeet-tdt-0.6b-v3-GGUF/resolve/main/parakeet-tdt-0.6b-v3-q8_0.gguf",
        expected_size: Some(745_121_600),
        size_label: "~745 MB",
        languages: "25 European languages",
        experimental: false,
        recommended: false,
    },
];

pub fn all_models() -> &'static [ModelInfo] {
    MODELS
}

pub fn find_model(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|model| model.id == id)
}

pub fn get_models_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let model_dir = data_dir.join("models");
    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;
    Ok(model_dir)
}

pub fn model_path(app_handle: &tauri::AppHandle, model: &ModelInfo) -> Result<PathBuf, String> {
    Ok(get_models_dir(app_handle)?.join(model.filename))
}

/// Whether the model's file exists on disk and matches its expected size
/// (when known). Models without a known size are considered present when the
/// file is non-empty.
pub fn is_downloaded(app_handle: &tauri::AppHandle, model: &ModelInfo) -> bool {
    let Ok(path) = model_path(app_handle, model) else {
        return false;
    };
    match fs::metadata(&path) {
        Ok(meta) => match model.expected_size {
            Some(size) => meta.len() == size,
            None => meta.len() > 0,
        },
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Active model selection
// ---------------------------------------------------------------------------

/// The transcription model currently in use. Mirrored to `<models>/active-model.txt`
/// so the choice survives restarts and is readable from the transcription path.
pub struct ActiveModel(pub Mutex<String>);

fn active_model_file(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(get_models_dir(app_handle)?.join("active-model.txt"))
}

/// Reads the persisted active model id, falling back to the default when it is
/// missing or no longer a known model.
fn load_active_model_id(app_handle: &tauri::AppHandle) -> String {
    active_model_file(app_handle)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|raw| raw.trim().to_string())
        .filter(|id| find_model(id).is_some())
        .unwrap_or_else(|| DEFAULT_MODEL_ID.to_string())
}

/// Builds the managed [`ActiveModel`] state during app setup.
pub fn init_active_model(app_handle: &tauri::AppHandle) -> ActiveModel {
    ActiveModel(Mutex::new(load_active_model_id(app_handle)))
}

/// The active model id, defensively falling back to the default if the lock is
/// poisoned.
pub fn active_model_id(state: &tauri::State<'_, ActiveModel>) -> String {
    state
        .0
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| DEFAULT_MODEL_ID.to_string())
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

#[derive(Clone, serde::Serialize)]
struct DownloadProgress {
    model_id: String,
    progress: u32,
}

fn emit_progress(app_handle: &tauri::AppHandle, model_id: &str, progress: u32) {
    let _ = app_handle.emit(
        "model-download-progress",
        DownloadProgress {
            model_id: model_id.to_string(),
            progress,
        },
    );
}

/// Streams a model's file to disk, emitting `model-download-progress` events
/// (`{ model_id, progress }`). The download is written to a temp file and
/// atomically renamed into place once validated.
pub async fn download_model_file(
    app_handle: &tauri::AppHandle,
    model: &ModelInfo,
) -> Result<(), String> {
    let destination = model_path(app_handle, model)?;

    if is_downloaded(app_handle, model) {
        emit_progress(app_handle, model.id, 100);
        return Ok(());
    }
    // Drop any partial/mismatched previous attempt.
    if destination.exists() {
        let _ = fs::remove_file(&destination);
    }

    let temp_path = destination.with_extension("download.tmp");
    let _ = fs::remove_file(&temp_path);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60 * 60))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let response = client
        .get(model.url)
        .send()
        .await
        .map_err(|e| format!("Failed to start download: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total_size = response
        .content_length()
        .or(model.expected_size)
        .unwrap_or(0);

    let mut file =
        fs::File::create(&temp_path).map_err(|e| format!("Failed to create file: {}", e))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!("Download failed: {}", e)
        })?;

        std::io::Write::write_all(&mut file, &chunk).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!("Write failed: {}", e)
        })?;

        downloaded += chunk.len() as u64;

        let progress = if total_size > 0 {
            ((downloaded as f64 / total_size as f64) * 100.0).min(99.0) as u32
        } else {
            0
        };
        emit_progress(app_handle, model.id, progress);
    }

    drop(file);

    let actual_size = fs::metadata(&temp_path).map_err(|e| e.to_string())?.len();
    match model.expected_size {
        Some(expected) if actual_size != expected => {
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "Download incomplete: got {} bytes, expected {}",
                actual_size, expected
            ));
        }
        None if actual_size == 0 => {
            let _ = fs::remove_file(&temp_path);
            return Err("Download incomplete: empty file".to_string());
        }
        _ => {}
    }

    fs::rename(&temp_path, &destination).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to finalize model: {}", e)
    })?;

    emit_progress(app_handle, model.id, 100);

    Ok(())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Serializable view of a model for the frontend, including runtime state.
#[derive(serde::Serialize)]
pub struct ModelDto {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub size_label: String,
    pub languages: String,
    pub experimental: bool,
    pub recommended: bool,
    pub downloaded: bool,
    pub active: bool,
}

#[tauri::command]
pub async fn list_models(
    app_handle: tauri::AppHandle,
    active: tauri::State<'_, ActiveModel>,
) -> Result<Vec<ModelDto>, String> {
    let active_id = active_model_id(&active);
    Ok(all_models()
        .iter()
        .map(|model| ModelDto {
            id: model.id.to_string(),
            name: model.name.to_string(),
            engine: model.engine.as_str().to_string(),
            size_label: model.size_label.to_string(),
            languages: model.languages.to_string(),
            experimental: model.experimental,
            recommended: model.recommended,
            downloaded: is_downloaded(&app_handle, model),
            active: model.id == active_id,
        })
        .collect())
}

#[tauri::command]
pub async fn get_active_model(active: tauri::State<'_, ActiveModel>) -> Result<String, String> {
    Ok(active_model_id(&active))
}

#[tauri::command]
pub async fn set_active_model(
    app_handle: tauri::AppHandle,
    active: tauri::State<'_, ActiveModel>,
    model_id: String,
) -> Result<(), String> {
    let model = find_model(&model_id).ok_or_else(|| format!("Unknown model: {}", model_id))?;

    if model.experimental {
        return Err(format!(
            "{} is experimental and not yet available for transcription.",
            model.name
        ));
    }
    if !is_downloaded(&app_handle, model) {
        return Err(format!("{} is not downloaded yet.", model.name));
    }

    {
        let mut guard = active.0.lock().map_err(|e| e.to_string())?;
        *guard = model.id.to_string();
    }

    let file = active_model_file(&app_handle)?;
    fs::write(&file, model.id)
        .map_err(|e| format!("Failed to persist model selection: {}", e))?;

    let _ = app_handle.emit("active-model-changed", model.id);
    Ok(())
}

#[tauri::command]
pub async fn check_model_downloaded(
    app_handle: tauri::AppHandle,
    model_id: String,
) -> Result<bool, String> {
    let model = find_model(&model_id).ok_or_else(|| format!("Unknown model: {}", model_id))?;
    Ok(is_downloaded(&app_handle, model))
}

#[tauri::command]
pub async fn download_model(app_handle: tauri::AppHandle, model_id: String) -> Result<(), String> {
    let model = find_model(&model_id).ok_or_else(|| format!("Unknown model: {}", model_id))?;
    download_model_file(&app_handle, model).await
}
