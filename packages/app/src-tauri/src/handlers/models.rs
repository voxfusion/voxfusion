use futures_util::StreamExt;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
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
    /// Expected SHA-256 of the model file (lowercase hex), from the Hugging
    /// Face LFS metadata. Verified after download, before the temp file is
    /// moved into place.
    pub sha256: Option<&'static str>,
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
        sha256: Some("1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"),
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
        sha256: Some("e8bc983c89342a1f36a5bfa1a7a2dc6fab8f9ebdc2e305738f36e3ff60cbc313"),
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
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
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

/// How long to wait for the TCP/TLS connection to come up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Abort when no data arrives for this long — replaces the old 1-hour total
/// request timeout that killed slow-but-alive connections.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);
/// How often the download loop wakes up to check the cancellation flag while
/// waiting for the next chunk.
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// In-flight downloads: model id -> cancellation flag. Guards against two
/// concurrent downloads of the same model interleaving into one temp file.
static ACTIVE_DOWNLOADS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Removes the model from [`ACTIVE_DOWNLOADS`] when the download completes,
/// fails, or is cancelled.
struct DownloadGuard {
    model_id: String,
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_DOWNLOADS.lock() {
            active.remove(&self.model_id);
        }
    }
}

/// Registers a download as in-flight, failing if one is already running for
/// this model. Returns the guard (hold it for the download's lifetime) and
/// the cancellation flag [`cancel_model_download`] sets.
fn begin_download(model_id: &str) -> Result<(DownloadGuard, Arc<AtomicBool>), String> {
    let mut active = ACTIVE_DOWNLOADS.lock().map_err(|e| e.to_string())?;
    if active.contains_key(model_id) {
        return Err("download already in progress".to_string());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    active.insert(model_id.to_string(), cancel.clone());
    Ok((
        DownloadGuard {
            model_id: model_id.to_string(),
        },
        cancel,
    ))
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    model_id: String,
    progress: u32,
    downloaded_bytes: u64,
    total_bytes: u64,
}

fn emit_progress(
    app_handle: &tauri::AppHandle,
    model_id: &str,
    progress: u32,
    downloaded_bytes: u64,
    total_bytes: u64,
) {
    let _ = app_handle.emit(
        "model-download-progress",
        DownloadProgress {
            model_id: model_id.to_string(),
            progress,
            downloaded_bytes,
            total_bytes,
        },
    );
}

/// Streams a model's file to disk, emitting `model-download-progress` events
/// (`{ modelId, progress, downloadedBytes, totalBytes }`) at most once per
/// whole-percent change. The download goes to a `.download.tmp` file that is
/// checksum-verified and atomically renamed into place. A leftover temp file
/// from an earlier failed or cancelled attempt is resumed with an HTTP Range
/// request instead of being restarted from scratch.
pub async fn download_model_file(
    app_handle: &tauri::AppHandle,
    model: &ModelInfo,
) -> Result<(), String> {
    let destination = model_path(app_handle, model)?;

    if is_downloaded(app_handle, model) {
        let size = fs::metadata(&destination).map(|m| m.len()).unwrap_or(0);
        emit_progress(app_handle, model.id, 100, size, size);
        return Ok(());
    }

    // Guard against concurrent downloads of the same model. Dropped (and the
    // model unregistered) on every exit path below.
    let (_guard, cancel) = begin_download(model.id)?;

    // Drop a mismatched previous final file (wrong size — see is_downloaded).
    if destination.exists() {
        let _ = fs::remove_file(&destination);
    }

    let temp_path = destination.with_extension("download.tmp");

    // Resume from a previous partial download when possible.
    let mut existing_len = tokio::fs::metadata(&temp_path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    if let Some(expected) = model.expected_size {
        if existing_len > expected {
            // An oversized partial cannot be valid; start over.
            let _ = tokio::fs::remove_file(&temp_path).await;
            existing_len = 0;
        }
    }

    // A temp file that already has every byte only needs the validation
    // below — a Range request for it would fail with 416.
    let already_complete = existing_len > 0 && model.expected_size == Some(existing_len);
    if !already_complete {
        stream_to_temp(app_handle, model, &temp_path, existing_len, &cancel).await?;
    }

    let actual_size = tokio::fs::metadata(&temp_path)
        .await
        .map_err(|e| e.to_string())?
        .len();
    match model.expected_size {
        Some(expected) if actual_size != expected => {
            // Keep the short temp file so the next attempt resumes from here.
            return Err(format!(
                "Download incomplete: got {} bytes, expected {}",
                actual_size, expected
            ));
        }
        None if actual_size == 0 => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err("Download incomplete: empty file".to_string());
        }
        _ => {}
    }

    verify_checksum(model, &temp_path).await?;

    tokio::fs::rename(&temp_path, &destination)
        .await
        .map_err(|e| format!("Failed to finalize model: {}", e))?;

    emit_progress(app_handle, model.id, 100, actual_size, actual_size);

    Ok(())
}

/// Streams the model URL into `temp_path`, resuming from `existing_len` bytes
/// via a Range request when possible (HTTP 206 appends; 200 restarts from
/// zero). Checks the cancellation flag between chunks; on cancel or error the
/// temp file is kept so a later attempt can resume.
async fn stream_to_temp(
    app_handle: &tauri::AppHandle,
    model: &ModelInfo,
    temp_path: &Path,
    existing_len: u64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut request = client.get(model.url);
    if existing_len > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={}-", existing_len));
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to start download: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Download failed: HTTP {}", status));
    }
    // 206 = the server honored the Range header, append to the partial file;
    // 200 = full body, truncate and restart from zero.
    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT && existing_len > 0;

    let mut downloaded = if resumed { existing_len } else { 0 };
    let total_size = response
        .content_length()
        .map(|len| if resumed { len + existing_len } else { len })
        .or(model.expected_size)
        .unwrap_or(0);

    let mut file = if resumed {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(temp_path)
            .await
    } else {
        tokio::fs::File::create(temp_path).await
    }
    .map_err(|e| format!("Failed to open download file: {}", e))?;

    let mut stream = response.bytes_stream();
    let mut last_percent: Option<u32> = None;
    let mut last_emit = std::time::Instant::now();

    loop {
        // Wait for the next chunk in short slices so cancellation stays
        // responsive; STALL_TIMEOUT with no data at all aborts the attempt.
        let mut waited = Duration::ZERO;
        let item = loop {
            if cancel.load(Ordering::SeqCst) {
                // Keep the temp file so a later download resumes from here.
                let _ = file.flush().await;
                log::info!(target: "models", "download cancelled model_id={}", model.id);
                return Err("Download cancelled".to_string());
            }
            match tokio::time::timeout(CANCEL_POLL_INTERVAL, stream.next()).await {
                Ok(item) => break item,
                Err(_) => {
                    waited += CANCEL_POLL_INTERVAL;
                    if waited >= STALL_TIMEOUT {
                        let _ = file.flush().await;
                        return Err(format!(
                            "Download stalled: no data received for {} seconds",
                            STALL_TIMEOUT.as_secs()
                        ));
                    }
                }
            }
        };
        let Some(chunk) = item else {
            break; // stream finished
        };
        let chunk = chunk.map_err(|e| format!("Download failed: {}", e))?;

        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write failed: {}", e))?;

        downloaded += chunk.len() as u64;

        // Throttle progress events to whole-percent changes (or one per 300ms
        // when the total is unknown) instead of the per-chunk firehose.
        if total_size > 0 {
            let percent = ((downloaded as f64 / total_size as f64) * 100.0).min(99.0) as u32;
            if last_percent != Some(percent) {
                last_percent = Some(percent);
                emit_progress(app_handle, model.id, percent, downloaded, total_size);
            }
        } else if last_emit.elapsed() >= Duration::from_millis(300) {
            last_emit = std::time::Instant::now();
            emit_progress(app_handle, model.id, 0, downloaded, 0);
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush download: {}", e))?;

    Ok(())
}

/// Verifies the downloaded temp file against the model's expected SHA-256
/// (hashing runs on a blocking thread). Deletes the temp file on mismatch so
/// the next attempt starts clean.
async fn verify_checksum(model: &ModelInfo, temp_path: &Path) -> Result<(), String> {
    let Some(expected) = model.sha256 else {
        return Ok(());
    };

    let path = temp_path.to_path_buf();
    let actual = tokio::task::spawn_blocking(move || -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let mut file = std::fs::File::open(&path)
            .map_err(|e| format!("Failed to open file for checksum: {}", e))?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher).map_err(|e| format!("Failed to hash file: {}", e))?;
        Ok(format!("{:x}", hasher.finalize()))
    })
    .await
    .map_err(|e| format!("Checksum task failed: {}", e))??;

    if actual != expected {
        let _ = tokio::fs::remove_file(temp_path).await;
        return Err(format!(
            "Checksum mismatch for {}: expected sha256 {}, got {}. The download was corrupted — please try again.",
            model.name, expected, actual
        ));
    }
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
    // A Parakeet model is only usable when the crispasr engine is present
    // (bundled in release builds); fail here rather than on every transcription.
    if model.engine == Engine::Parakeet && !crate::handlers::parakeet::engine_available(&app_handle)
    {
        return Err(format!(
            "{} cannot be activated: the crispasr engine that runs Parakeet models is missing from this build. Reinstall VoxFusion, or run packages/app/scripts/build-parakeet-engine.sh in development.",
            model.name
        ));
    }

    {
        let mut guard = active.0.lock().map_err(|e| e.to_string())?;
        *guard = model.id.to_string();
    }

    let file = active_model_file(&app_handle)?;
    fs::write(&file, model.id).map_err(|e| format!("Failed to persist model selection: {}", e))?;

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

/// Requests cancellation of an in-flight model download. The in-progress
/// `download_model` call returns `Err("Download cancelled")` and keeps its
/// partial `.download.tmp` file so a later download resumes where it left off.
#[tauri::command]
pub async fn cancel_model_download(model_id: String) -> Result<(), String> {
    let active = ACTIVE_DOWNLOADS.lock().map_err(|e| e.to_string())?;
    match active.get(&model_id) {
        Some(cancel) => {
            cancel.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(format!("No download in progress for {}", model_id)),
    }
}
