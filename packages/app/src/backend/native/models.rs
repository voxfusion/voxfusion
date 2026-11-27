use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use crate::backend::{
    AppEvent, DEFAULT_MODEL_ID, DOWNLOAD_CANCELLED_ERROR, DOWNLOAD_IN_PROGRESS_ERROR, EventSender,
    ModelDownloadProgress, ModelInfo,
};

/// Speech-to-text engine that backs a model. Each engine has its own
/// inference path in [`super::whisper::transcribe_audio`].
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

    /// Whether this build can run the engine: Whisper is compiled in, while
    /// Parakeet's engine is a binary that release builds bundle.
    pub fn is_available(self) -> bool {
        match self {
            Engine::Whisper => true,
            Engine::Parakeet => super::parakeet::engine_available(),
        }
    }
}

/// Static metadata describing a downloadable transcription model.
pub struct ModelSpec {
    /// Stable identifier used by the interface and persisted as the active model.
    pub id: &'static str,
    pub name: &'static str,
    pub engine: Engine,
    /// Local filename inside the models directory.
    pub filename: &'static str,
    /// Direct (single-file) download URL, at a fixed revision of its
    /// repository: a file replaced upstream would fail the size and checksum
    /// checks below.
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

/// The model registry — the single source of truth for available models.
const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: DEFAULT_MODEL_ID,
        name: "Whisper Large v3 Turbo",
        engine: Engine::Whisper,
        filename: "ggml-large-v3-turbo.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin",
        expected_size: Some(1_624_555_275),
        sha256: Some("1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"),
        size_label: "~1.5 GB",
        languages: "99 languages",
        experimental: false,
        recommended: true,
    },
    ModelSpec {
        id: "parakeet-tdt-0.6b-v3",
        name: "Parakeet TDT 0.6B v3",
        engine: Engine::Parakeet,
        filename: "parakeet-tdt-0.6b-v3-q8_0.gguf",
        // GGUF port of NVIDIA's Parakeet TDT 0.6B v3 (q8_0), run via the crispasr
        // engine (see `super::parakeet`). The file on `main` was requantized on
        // 2026-07-11 (674,342,336 bytes); this is the revision before it.
        url: "https://huggingface.co/cstr/parakeet-tdt-0.6b-v3-GGUF/resolve/815cc1bcb5cfb92f9cd0bbd22c7bc2f6e2d4ec13/parakeet-tdt-0.6b-v3-q8_0.gguf",
        expected_size: Some(745_121_600),
        sha256: Some("e8bc983c89342a1f36a5bfa1a7a2dc6fab8f9ebdc2e305738f36e3ff60cbc313"),
        size_label: "~745 MB",
        languages: "25 European languages",
        experimental: false,
        recommended: false,
    },
];

pub fn find_model(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|model| model.id == id)
}

pub fn get_models_dir() -> Result<PathBuf, String> {
    let model_dir = crate::paths::data_dir().join("models");
    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;
    Ok(model_dir)
}

pub fn model_path(model: &ModelSpec) -> Result<PathBuf, String> {
    Ok(get_models_dir()?.join(model.filename))
}

fn temp_model_path(destination: &Path) -> PathBuf {
    destination.with_extension("download.tmp")
}

/// Every file on disk belonging to `model`: its partial `.download.tmp` file
/// first, then its final model file. Removing the temp file first keeps an
/// installed model intact if deleting the temp file fails.
fn model_files(model: &ModelSpec) -> Result<Vec<PathBuf>, String> {
    let destination = model_path(model)?;
    let temp_path = temp_model_path(&destination);
    let mut files = Vec::with_capacity(2);
    if temp_path != destination {
        files.push(temp_path);
    }
    files.push(destination);
    Ok(files)
}

/// Bytes reclaimed on disk by unlinking a path with metadata `meta` (from
/// `symlink_metadata`). Returns 0 for non-regular files (such as symlinks) and
/// hard-linked files (`nlink > 1`), and caps logical length by allocated
/// 512-byte blocks so sparse files do not overstate freed space.
fn reclaimable_file_size(meta: &fs::Metadata) -> u64 {
    if !meta.is_file() {
        return 0;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            return 0;
        }
        let allocated = meta.blocks().saturating_mul(512);
        meta.len().min(allocated)
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

/// Reclaimable bytes on disk for `path` without dereferencing symlinks.
fn reclaimable_path_size(path: &Path) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(meta) => reclaimable_file_size(&meta),
        Err(_) => 0,
    }
}

/// Total reclaimable bytes occupied on disk by `model`'s files (final file and
/// any partial `.download.tmp`).
pub fn disk_size_bytes(model: &ModelSpec) -> u64 {
    let Ok(files) = model_files(model) else {
        return 0;
    };
    files.iter().map(|path| reclaimable_path_size(path)).sum()
}

/// Whether any file belonging to `model` (final file or partial `.download.tmp`)
/// exists on disk, including symlinks, sparse files, and hard-linked files.
pub fn has_model_files(model: &ModelSpec) -> bool {
    let Ok(files) = model_files(model) else {
        return false;
    };
    files.iter().any(|path| fs::symlink_metadata(path).is_ok())
}

/// Whether the model's file exists on disk and matches its expected size
/// (when known). Models without a known size are considered present when the
/// file is non-empty.
pub fn is_downloaded(model: &ModelSpec) -> bool {
    let Ok(path) = model_path(model) else {
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
// Active model selection and in-flight transcription tracking
// ---------------------------------------------------------------------------

// Lock ordering across model state (never invert):
// 1. `ActiveModel.0`
// 2. `TRANSCRIBING_MODELS`
// 3. `ACTIVE_DOWNLOADS`

/// The transcription model currently in use. Mirrored to `<models>/active-model.txt`
/// so the choice survives restarts and is readable from the transcription path.
pub struct ActiveModel(pub Mutex<String>);

/// Refcount of in-flight transcriptions per model id. Prevents removing a
/// model file that a transcription started before the user switched models.
static TRANSCRIBING_MODELS: LazyLock<Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Holds a model registered as in-use for the lifetime of one transcription.
pub struct TranscriptionGuard {
    model_id: String,
}

impl Drop for TranscriptionGuard {
    fn drop(&mut self) {
        if let Ok(mut in_use) = TRANSCRIBING_MODELS.lock()
            && let Some(count) = in_use.get_mut(&self.model_id)
        {
            if *count <= 1 {
                in_use.remove(&self.model_id);
            } else {
                *count -= 1;
            }
        }
    }
}

/// Resolves the active model and marks it as in-use for transcription under
/// the active-model lock, so another thread cannot switch models and remove
/// its file before transcription opens it.
pub fn begin_transcription(
    active: &ActiveModel,
) -> Result<(&'static ModelSpec, PathBuf, TranscriptionGuard), String> {
    let (model, guard) = {
        let active_guard = active.0.lock().map_err(|e| e.to_string())?;
        let active_id = active_guard.clone();
        let model =
            find_model(&active_id).ok_or_else(|| format!("Unknown active model: {}", active_id))?;

        let mut in_use = TRANSCRIBING_MODELS.lock().map_err(|e| e.to_string())?;
        *in_use.entry(model.id.to_string()).or_insert(0) += 1;
        let guard = TranscriptionGuard {
            model_id: model.id.to_string(),
        };
        (model, guard)
    };

    let path = model_path(model)?;
    if !path.exists() {
        return Err("Model not found. Please download it first.".to_string());
    }

    Ok((model, path, guard))
}

fn active_model_file() -> Result<PathBuf, String> {
    Ok(get_models_dir()?.join("active-model.txt"))
}

/// Reads the persisted active model id, falling back to the default when it is
/// missing or no longer a known model.
fn load_active_model_id() -> String {
    active_model_file()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|raw| raw.trim().to_string())
        .filter(|id| find_model(id).is_some())
        .unwrap_or_else(|| DEFAULT_MODEL_ID.to_string())
}

/// Builds the [`ActiveModel`] state during app setup.
pub fn init_active_model() -> ActiveModel {
    ActiveModel(Mutex::new(load_active_model_id()))
}

/// The active model id, defensively falling back to the default if the lock is
/// poisoned.
pub fn active_model_id(state: &ActiveModel) -> String {
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
/// How a failed checksum verification's error starts.
const CHECKSUM_MISMATCH_ERROR: &str = "Checksum mismatch";
/// How much of the file is hashed between two checks of the cancellation
/// flag during verification.
const HASH_CHUNK_SIZE: usize = 1024 * 1024;

/// In-flight downloads: model id -> cancellation flag. Guards against two
/// concurrent downloads of the same model interleaving into one temp file.
static ACTIVE_DOWNLOADS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Removes the model from [`ACTIVE_DOWNLOADS`] when the download completes,
/// fails, or is cancelled, and emits [`AppEvent::ModelDownloadStopped`] if it
/// did not finish so every open view clears its downloading state.
struct DownloadGuard {
    model_id: String,
    events: EventSender,
    finished: bool,
}

impl DownloadGuard {
    fn mark_finished(mut self) {
        self.finished = true;
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_DOWNLOADS.lock() {
            active.remove(&self.model_id);
        }
        if !self.finished {
            self.events.emit(AppEvent::ModelDownloadStopped {
                model_id: self.model_id.clone(),
            });
        }
    }
}

/// Registers a download as in-flight, failing if one is already running for
/// this model. Returns the guard (hold it for the download's lifetime) and
/// the cancellation flag [`cancel_model_download`] sets.
fn begin_download(
    events: &EventSender,
    model_id: &str,
) -> Result<(DownloadGuard, Arc<AtomicBool>), String> {
    let mut active = ACTIVE_DOWNLOADS.lock().map_err(|e| e.to_string())?;
    if active.contains_key(model_id) {
        return Err(DOWNLOAD_IN_PROGRESS_ERROR.to_string());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    active.insert(model_id.to_string(), cancel.clone());
    Ok((
        DownloadGuard {
            model_id: model_id.to_string(),
            events: events.clone(),
            finished: false,
        },
        cancel,
    ))
}

fn emit_progress(
    events: &EventSender,
    model_id: &str,
    progress: u32,
    downloaded_bytes: u64,
    total_bytes: u64,
) {
    events.emit(AppEvent::ModelDownloadProgress(ModelDownloadProgress {
        model_id: model_id.to_string(),
        progress,
        downloaded_bytes,
        total_bytes,
    }));
}

/// Streams a model's file to disk, emitting `model-download-progress` events
/// (`{ modelId, progress, downloadedBytes, totalBytes }`) at most once per
/// whole-percent change. The download goes to a `.download.tmp` file that is
/// checksum-verified (announced by a `model-verifying` event) and atomically
/// renamed into place. A leftover temp file from an earlier failed or
/// cancelled attempt is resumed with an HTTP Range request instead of being
/// restarted from scratch.
pub async fn download_model_file(events: &EventSender, model: &ModelSpec) -> Result<(), String> {
    let destination = model_path(model)?;

    // Guard against concurrent downloads or removal of the same model. Dropped
    // (and the model unregistered) on every exit path below.
    let (guard, cancel) = begin_download(events, model.id)?;

    if is_downloaded(model) {
        let size = fs::metadata(&destination).map(|m| m.len()).unwrap_or(0);
        guard.mark_finished();
        emit_progress(events, model.id, 100, size, size);
        return Ok(());
    }

    // Drop a mismatched previous final file (wrong size — see is_downloaded).
    if destination.exists() {
        let _ = fs::remove_file(&destination);
    }

    let temp_path = temp_model_path(&destination);

    // Resume from a previous partial download when possible.
    let mut existing_len = tokio::fs::metadata(&temp_path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    if let Some(expected) = model.expected_size
        && existing_len > expected
    {
        // An oversized partial cannot be valid; start over.
        let _ = tokio::fs::remove_file(&temp_path).await;
        existing_len = 0;
    }

    // A temp file that already has every byte only needs the validation
    // below — a Range request for it would fail with 416.
    let already_complete = existing_len > 0 && model.expected_size == Some(existing_len);
    if !already_complete {
        stream_to_temp(events, model, &temp_path, existing_len, &cancel).await?;
    }

    let mut actual_size = downloaded_size(model, &temp_path).await?;
    if let Err(error) = verify_checksum(events, model, &temp_path, &cancel).await {
        if existing_len == 0 || !error.starts_with(CHECKSUM_MISMATCH_ERROR) {
            return Err(error);
        }
        // The bytes kept from an earlier attempt may come from an older
        // upload of the file, which the checksum cannot tell apart from a
        // corrupted download: fetch the whole file once more.
        log::warn!(target: "models", "resumed_download_corrupt model_id={} restarting", model.id);
        stream_to_temp(events, model, &temp_path, 0, &cancel).await?;
        actual_size = downloaded_size(model, &temp_path).await?;
        verify_checksum(events, model, &temp_path, &cancel).await?;
    }

    tokio::fs::rename(&temp_path, &destination)
        .await
        .map_err(|e| format!("Failed to finalize model: {}", e))?;

    // Unregister before notifying listeners so a `list_models` triggered by the
    // 100% progress event sees `downloading == false`.
    guard.mark_finished();
    emit_progress(events, model.id, 100, actual_size, actual_size);

    Ok(())
}

/// The size of the downloaded temp file, if it is the size the model's file
/// should be.
async fn downloaded_size(model: &ModelSpec, temp_path: &Path) -> Result<u64, String> {
    let actual_size = tokio::fs::metadata(temp_path)
        .await
        .map_err(|e| e.to_string())?
        .len();
    match model.expected_size {
        Some(expected) if actual_size != expected => {
            // Keep the short temp file so the next attempt resumes from here.
            Err(format!(
                "Download incomplete: got {} bytes, expected {}",
                actual_size, expected
            ))
        }
        None if actual_size == 0 => {
            let _ = tokio::fs::remove_file(temp_path).await;
            Err("Download incomplete: empty file".to_string())
        }
        _ => Ok(actual_size),
    }
}

/// Streams the model URL into `temp_path`, resuming from `existing_len` bytes
/// via a Range request when possible (HTTP 206 appends; 200 restarts from
/// zero). Checks the cancellation flag between chunks; on cancel or error the
/// temp file is kept so a later attempt can resume.
async fn stream_to_temp(
    events: &EventSender,
    model: &ModelSpec,
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
                return Err(DOWNLOAD_CANCELLED_ERROR.to_string());
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
                emit_progress(events, model.id, percent, downloaded, total_size);
            }
        } else if last_emit.elapsed() >= Duration::from_millis(300) {
            last_emit = std::time::Instant::now();
            emit_progress(events, model.id, 0, downloaded, 0);
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush download: {}", e))?;

    Ok(())
}

/// Verifies the downloaded temp file against the model's expected SHA-256
/// (hashing runs on a blocking thread), after telling the interface the
/// transfer is over. Checks the cancellation flag between chunks; a cancelled
/// check keeps the complete temp file, so the next attempt only hashes it
/// again. Deletes the temp file on mismatch so the next attempt starts clean.
async fn verify_checksum(
    events: &EventSender,
    model: &ModelSpec,
    temp_path: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let Some(expected) = model.sha256 else {
        return Ok(());
    };

    events.emit(AppEvent::ModelVerifying {
        model_id: model.id.to_string(),
    });

    let path = temp_path.to_path_buf();
    let cancel = cancel.clone();
    let actual = tokio::task::spawn_blocking(move || -> Result<String, String> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        let mut file = std::fs::File::open(&path)
            .map_err(|e| format!("Failed to open file for checksum: {}", e))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; HASH_CHUNK_SIZE];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(DOWNLOAD_CANCELLED_ERROR.to_string());
            }
            let read = match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("Failed to hash file: {}", e)),
            };
            hasher.update(&buffer[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    })
    .await
    .map_err(|e| format!("Checksum task failed: {}", e))?
    .inspect_err(|error| {
        if error == DOWNLOAD_CANCELLED_ERROR {
            log::info!(target: "models", "verification cancelled model_id={}", model.id);
        }
    })?;

    if actual != expected {
        let _ = tokio::fs::remove_file(temp_path).await;
        return Err(format!(
            "{CHECKSUM_MISMATCH_ERROR} for {}: expected sha256 {}, got {}. The download was corrupted — please try again.",
            model.name, expected, actual
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Every model with its runtime state, for the interface.
pub fn list_models(active: &ActiveModel) -> Vec<ModelInfo> {
    let active_id = active_model_id(active);
    let active_downloads: HashSet<String> = ACTIVE_DOWNLOADS
        .lock()
        .map(|downloads| downloads.keys().cloned().collect())
        .unwrap_or_default();
    MODELS
        .iter()
        .map(|model| ModelInfo {
            id: model.id.to_string(),
            name: model.name.to_string(),
            engine: model.engine.as_str().to_string(),
            size_label: model.size_label.to_string(),
            languages: model.languages.to_string(),
            experimental: model.experimental,
            recommended: model.recommended,
            available: model.engine.is_available(),
            downloaded: is_downloaded(model),
            active: model.id == active_id,
            has_files: has_model_files(model),
            disk_size_bytes: Some(disk_size_bytes(model)),
            downloading: active_downloads.contains(model.id),
        })
        .collect()
}

pub fn set_active_model(active: &ActiveModel, model_id: &str) -> Result<(), String> {
    let model = find_model(model_id).ok_or_else(|| format!("Unknown model: {}", model_id))?;

    if model.experimental {
        return Err(format!(
            "{} is experimental and not yet available for transcription.",
            model.name
        ));
    }
    // A Parakeet model is only usable when the crispasr engine is present
    // (bundled in release builds); fail here rather than on every transcription.
    if !model.engine.is_available() {
        return Err(format!(
            "{} cannot be activated: the crispasr engine that runs Parakeet models is missing from this build. Reinstall VoxFusion, or run packages/app/scripts/build-parakeet-engine.sh in development.",
            model.name
        ));
    }

    let mut guard = active.0.lock().map_err(|e| e.to_string())?;
    if !is_downloaded(model) {
        return Err(format!("{} is not downloaded yet.", model.name));
    }

    let file = active_model_file()?;
    fs::write(&file, model.id).map_err(|e| format!("Failed to persist model selection: {}", e))?;
    *guard = model.id.to_string();
    super::whisper::clear_cache();
    Ok(())
}

/// Deletes every file on disk belonging to `model_id` (both its partial
/// `.download.tmp` and final file) and returns the exact number of bytes freed.
/// Refuses while the model is active, downloading, or used by a running
/// transcription.
pub fn remove_model(active: &ActiveModel, model_id: &str) -> Result<u64, String> {
    let model = find_model(model_id).ok_or_else(|| format!("Unknown model: {}", model_id))?;
    let files = model_files(model)?;

    let freed = {
        let active_guard = active.0.lock().map_err(|e| e.to_string())?;
        if *active_guard == model.id {
            return Err(format!(
                "Cannot remove {} while it is the active model. Select another model first.",
                model.name
            ));
        }

        let transcribing = TRANSCRIBING_MODELS.lock().map_err(|e| e.to_string())?;
        if transcribing.get(model.id).copied().unwrap_or(0) > 0 {
            return Err(format!(
                "Cannot remove {} while a transcription is using it.",
                model.name
            ));
        }

        let downloads = ACTIVE_DOWNLOADS.lock().map_err(|e| e.to_string())?;
        if downloads.contains_key(model.id) {
            return Err(format!(
                "Cannot remove {} while it is downloading.",
                model.name
            ));
        }

        let mut freed: u64 = 0;
        super::whisper::clear_cache();
        for path in &files {
            let meta = match fs::symlink_metadata(path) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("Failed to inspect {}: {}", model.name, e)),
            };
            let size = reclaimable_file_size(&meta);
            match fs::remove_file(path) {
                Ok(()) => freed = freed.saturating_add(size),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Failed to remove {}: {}", model.name, e)),
            }
        }
        freed
    };

    log::info!(
        target: "models",
        "model_removed model_id={} freed_bytes={}",
        model.id,
        freed
    );
    Ok(freed)
}

pub async fn download_model(events: &EventSender, model_id: &str) -> Result<(), String> {
    let model = find_model(model_id).ok_or_else(|| format!("Unknown model: {}", model_id))?;
    download_model_file(events, model).await
}

/// Requests cancellation of an in-flight model download, during the transfer
/// or the checksum verification after it. The in-progress `download_model`
/// call returns `Err("Download cancelled")` and keeps its `.download.tmp` file
/// so a later download resumes where it left off.
pub fn cancel_model_download(model_id: &str) -> Result<(), String> {
    let active = ACTIVE_DOWNLOADS.lock().map_err(|e| e.to_string())?;
    match active.get(model_id) {
        Some(cancel) => {
            cancel.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(format!("No download in progress for {}", model_id)),
    }
}
