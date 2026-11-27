use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::{Arc, LazyLock, Mutex};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::db::DbState;
use super::{apps, audio, models, parakeet, sites};
use crate::backend::TranscriptionResult;

#[cfg(target_os = "windows")]
struct CachedWhisper {
    path: PathBuf,
    ctx: Arc<WhisperContext>,
}

#[cfg(target_os = "windows")]
static WHISPER_CACHE: LazyLock<Mutex<Option<CachedWhisper>>> = LazyLock::new(|| Mutex::new(None));

/// Drops the cached Whisper model context (for example when switching or
/// deleting models) so its memory is released immediately.
pub fn clear_cache() {
    #[cfg(target_os = "windows")]
    if let Ok(mut guard) = WHISPER_CACHE.lock() {
        *guard = None;
    }
}

#[cfg(target_os = "windows")]
fn load_whisper_context(model_path: &Path) -> Result<Arc<WhisperContext>, String> {
    if let Ok(guard) = WHISPER_CACHE.lock()
        && let Some(cached) = guard.as_ref()
        && cached.path == model_path
    {
        return Ok(cached.ctx.clone());
    }

    let mut ctx_params = WhisperContextParameters::default();
    // CPU builds run significantly faster through standard SGEMM attention
    // than CPU Flash Attention.
    ctx_params.flash_attn(false);

    // MinGW's narrow `std::ifstream` inside whisper.cpp uses the active ANSI
    // codepage rather than UTF-8, so non-ASCII profile paths fail to open by
    // path. Read through Rust's wide Win32 path APIs in that case.
    let raw_ctx = if model_path.to_str().is_some_and(|s| s.is_ascii()) {
        WhisperContext::new_with_params(model_path, ctx_params)
            .map_err(|e| format!("Failed to load model: {}", e))?
    } else {
        let bytes = std::fs::read(model_path)
            .map_err(|e| format!("Failed to read model file: {}", e))?;
        WhisperContext::new_from_buffer_with_params(&bytes, ctx_params)
            .map_err(|e| format!("Failed to load model from buffer: {}", e))?
    };
    let ctx = Arc::new(raw_ctx);

    if let Ok(mut guard) = WHISPER_CACHE.lock() {
        *guard = Some(CachedWhisper {
            path: model_path.to_path_buf(),
            ctx: ctx.clone(),
        });
    }
    Ok(ctx)
}

#[cfg(not(target_os = "windows"))]
fn load_whisper_context(model_path: &Path) -> Result<WhisperContext, String> {
    let mut ctx_params = WhisperContextParameters::default();
    // whisper-rs disables Flash Attention by default, so enable it explicitly.
    ctx_params.flash_attn(true);
    WhisperContext::new_with_params(model_path, ctx_params)
        .map_err(|e| format!("Failed to load model: {}", e))
}

fn cap_prompt_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect()
}

#[cfg(not(target_os = "windows"))]
const WHISPER_BEAM_SIZE: i32 = 8;

/// Whether the active model is on disk and ready to transcribe.
pub fn check_model_status(active: &models::ActiveModel) -> Result<bool, String> {
    let active_id = models::active_model_id(active);
    let model = models::find_model(&active_id).ok_or_else(|| "Unknown active model".to_string())?;
    Ok(models::is_downloaded(model))
}

fn read_wav_as_f32(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("Failed to open WAV: {}", e))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = read_samples_as_f32(&mut reader, spec)?;

    let mono: Vec<f32> = if channels > 1 {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    if sample_rate != 16000 {
        let ratio = sample_rate as f64 / 16000.0;
        let output_len = (mono.len() as f64 / ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);
        for i in 0..output_len {
            let src_pos = i as f64 * ratio;
            let src_idx = src_pos as usize;
            let frac = (src_pos - src_idx as f64) as f32;
            let sample = if src_idx + 1 < mono.len() {
                mono[src_idx] * (1.0 - frac) + mono[src_idx + 1] * frac
            } else if src_idx < mono.len() {
                mono[src_idx]
            } else {
                0.0
            };
            resampled.push(sample);
        }
        Ok(resampled)
    } else {
        Ok(mono)
    }
}

fn read_samples_as_f32<R: std::io::Read>(
    reader: &mut hound::WavReader<R>,
    spec: hound::WavSpec,
) -> Result<Vec<f32>, String> {
    match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.map_err(|e| format!("Failed to read float sample: {}", e)))
            .collect(),
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i16>()
                .map(|sample| {
                    sample
                        .map(|value| value as f32 / max_val)
                        .map_err(|e| format!("Failed to read 16-bit sample: {}", e))
                })
                .collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| {
                    sample
                        .map(|value| value as f32 / max_val)
                        .map_err(|e| format!("Failed to read int sample: {}", e))
                })
                .collect()
        }
    }
}

fn fetch_dictionary_words(conn: &rusqlite::Connection) -> Option<String> {
    let mut stmt = conn
        .prepare("SELECT word FROM dictionary_words ORDER BY created_at DESC LIMIT 50")
        .ok()?;
    let words: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(", "))
    }
}

/// Deletes `path` when dropped if `active`, so opting out of history leaves no
/// temporary recording on disk on any exit path (success, error, or panic).
struct DiscardOnDrop<'a> {
    path: &'a Path,
    active: bool,
}

impl Drop for DiscardOnDrop<'_> {
    fn drop(&mut self) {
        if self.active
            && let Err(err) = audio::discard_recording(self.path)
        {
            log::warn!(target: "audio", "discard_recording_failed error={err}");
        }
    }
}

pub fn transcribe_audio(
    db_state: &DbState,
    active: &models::ActiveModel,
    audio_path: &Path,
    bundle_id: Option<&str>,
    domain: Option<&str>,
    fallback_style: &str,
    retain_recording: bool,
) -> Result<TranscriptionResult, String> {
    // Declared before `_discard` so `_discard` runs first while `audio_path` is
    // still protected from a concurrent Clear History.
    let _transcribing = audio::transcribing(audio_path);
    let _discard = DiscardOnDrop {
        path: audio_path,
        active: !retain_recording,
    };
    let (model, model_path, _model_use) = models::begin_transcription(active)?;

    let start = std::time::Instant::now();

    let audio_data = read_wav_as_f32(audio_path)?;
    let audio_duration_ms = if !audio_data.is_empty() {
        Some((audio_data.len() as f64 / 16.0) as i64)
    } else {
        None
    };

    let (style_key, dictionary) = {
        let conn = db_state.conn.lock().map_err(|e| e.to_string())?;
        let style_key = apps::resolve_style_key(&conn, bundle_id, domain, Some(fallback_style));
        (style_key, resolve_dictionary(&conn, bundle_id, domain))
    };

    let text = match model.engine {
        models::Engine::Parakeet => {
            parakeet::transcribe(&model_path, audio_path, &audio_data, dictionary.as_deref())
        }
        models::Engine::Whisper => {
            transcribe_whisper(&model_path, &audio_data, &style_key, dictionary.as_deref())
        }
    }?;
    let processing_time_ms = start.elapsed().as_millis() as i64;

    // One transcription lifecycle for both engines. When history saving is
    // off, `_discard` deletes this temporary recording on exit without pruning
    // older saved recordings. When history saving is on, pruning happens only
    // after `save_transcription` succeeds so an opt-out mid-transcription
    // never evicts an older saved recording.
    Ok(TranscriptionResult {
        word_count: text.split_whitespace().count() as i64,
        text,
        processing_time_ms,
        audio_duration_ms,
    })
}

fn resolve_dictionary(
    conn: &rusqlite::Connection,
    bundle_id: Option<&str>,
    domain: Option<&str>,
) -> Option<String> {
    let default_dict = fetch_dictionary_words(conn);
    let app_dict = bundle_id
        .filter(|s| !s.is_empty())
        .and_then(|id| apps::fetch_app_dictionary_words(conn, id));
    let site_dict = domain
        .filter(|s| !s.is_empty())
        .and_then(|site| sites::fetch_site_dictionary_words(conn, site));
    merge_dictionaries(
        default_dict.as_deref(),
        app_dict.as_deref(),
        site_dict.as_deref(),
    )
}

fn transcribe_whisper(
    model_path: &Path,
    audio_data: &[f32],
    style_key: &str,
    dictionary: Option<&str>,
) -> Result<String, String> {
    let ctx = load_whisper_context(model_path)?;

    let thread_count = std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).max(1) as i32)
        .unwrap_or(4);

    let mut state = ctx
        .create_state()
        .map_err(|e| format!("Failed to create state: {}", e))?;

    // Step 1 & 2: detect language so we can pick a same-language style prompt.
    // On Windows CPU builds, skip the extra encoder pass when using the default
    // style (which has no style prompt in any language) and let `state.full`
    // auto-detect the language in a single pass.
    let (detected_lang, style_text) =
        if cfg!(target_os = "windows") && style_key == apps::DEFAULT_STYLE_KEY {
            (None, "")
        } else {
            state
                .pcm_to_mel(audio_data, thread_count as usize)
                .map_err(|e| format!("Failed to compute mel: {:?}", e))?;
            let lang = match state.lang_detect(0, thread_count as usize) {
                Ok((lang_id, _probs)) => whisper_rs::get_lang_str(lang_id)
                    .unwrap_or(apps::DEFAULT_LOCALE)
                    .to_string(),
                Err(e) => {
                    eprintln!("[whisper] lang_detect failed: {:?}", e);
                    apps::DEFAULT_LOCALE.to_string()
                }
            };
            let style = apps::style_prompt_text(style_key, &lang);
            (Some(lang), style)
        };
    let composed_prompt = compose_prompt(style_text, dictionary);

    // Step 3: run full transcription with the resolved prompt.
    #[cfg(not(target_os = "windows"))]
    let strategy = SamplingStrategy::BeamSearch {
        beam_size: WHISPER_BEAM_SIZE,
        patience: -1.0,
    };
    #[cfg(target_os = "windows")]
    let strategy = SamplingStrategy::Greedy { best_of: 1 };

    let mut params = FullParams::new(strategy);
    params.set_n_threads(thread_count);
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_nst(true);
    params.set_temperature(0.0);
    params.set_temperature_inc(0.0);
    params.set_translate(false);
    params.set_language(detected_lang.as_deref());

    let capped_prompt = composed_prompt.as_deref().map(|p| cap_prompt_chars(p, 700));
    if let Some(ref p) = capped_prompt {
        let preview: String = p.chars().take(80).collect();
        eprintln!(
            "[whisper] initial_prompt: {} chars lang={} (head: {:?})",
            p.chars().count(),
            detected_lang.as_deref().unwrap_or("auto"),
            preview
        );
        params.set_initial_prompt(p);
    } else {
        eprintln!(
            "[whisper] no initial_prompt lang={}",
            detected_lang.as_deref().unwrap_or("auto")
        );
    }

    state
        .full(params, audio_data)
        .map_err(|e| format!("Transcription failed: {}", e))?;

    let num_segments = state.full_n_segments();

    let mut text = String::new();
    for i in 0..num_segments {
        if let Some(segment) = state.get_segment(i) {
            let segment_text = segment
                .to_str_lossy()
                .map_err(|e| format!("Failed to get segment: {}", e))?;
            text.push_str(&segment_text);
        }
    }

    Ok(text.trim().to_string())
}

fn merge_dictionaries(
    default_dict: Option<&str>,
    app_dict: Option<&str>,
    site_dict: Option<&str>,
) -> Option<String> {
    let parts: Vec<&str> = [default_dict, app_dict, site_dict]
        .into_iter()
        .filter_map(|d| d.map(|s| s.trim()).filter(|s| !s.is_empty()))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn compose_prompt(style_text: &str, dictionary: Option<&str>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let style_trimmed = style_text.trim();
    if !style_trimmed.is_empty() {
        parts.push(style_trimmed.to_string());
    }
    if let Some(dict) = dictionary.map(|d| d.trim()).filter(|d| !d.is_empty()) {
        parts.push(dict.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictionary_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE dictionary_words (word TEXT, created_at TEXT);
             CREATE TABLE app_dictionary_words (bundle_id TEXT, word TEXT, created_at TEXT);
             CREATE TABLE site_dictionary_words (domain TEXT, word TEXT, created_at TEXT);
             INSERT INTO dictionary_words VALUES ('VoxFusion', '2026-01-01');
             INSERT INTO app_dictionary_words VALUES ('test.editor', 'SolidJS', '2026-01-01');
             INSERT INTO app_dictionary_words VALUES ('other.app', 'UnrelatedApp', '2026-01-01');
             INSERT INTO site_dictionary_words VALUES ('project.test', 'CrispASR', '2026-01-01');
             INSERT INTO site_dictionary_words VALUES ('other.test', 'UnrelatedSite', '2026-01-01');",
        ).unwrap();
        conn
    }

    #[test]
    fn dictionary_includes_only_global_and_matching_contexts() {
        let conn = dictionary_db();
        assert_eq!(
            resolve_dictionary(&conn, Some("test.editor"), Some("project.test")).as_deref(),
            Some("VoxFusion, SolidJS, CrispASR"),
        );
        assert_eq!(
            resolve_dictionary(&conn, Some("test.editor"), None).as_deref(),
            Some("VoxFusion, SolidJS"),
        );
        assert_eq!(
            resolve_dictionary(&conn, None, Some("project.test")).as_deref(),
            Some("VoxFusion, CrispASR"),
        );
    }

    #[test]
    fn missing_context_keeps_global_dictionary_and_empty_dictionary_has_no_prompt() {
        let conn = dictionary_db();
        for context in [None, Some(""), Some("unknown")] {
            assert_eq!(
                resolve_dictionary(&conn, context, context).as_deref(),
                Some("VoxFusion")
            );
        }
        conn.execute("DELETE FROM dictionary_words", []).unwrap();
        assert_eq!(resolve_dictionary(&conn, None, None), None);
        assert_eq!(compose_prompt("", None), None);
    }

    #[test]
    fn whisper_keeps_style_and_dictionary_in_its_prompt() {
        assert_eq!(
            compose_prompt("Example prose.", Some("VoxFusion, SolidJS")).as_deref(),
            Some("Example prose.\n\nVoxFusion, SolidJS")
        );
        assert_eq!(cap_prompt_chars("Привет", 3), "При");
    }
}
