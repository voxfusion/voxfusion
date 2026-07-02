use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::handlers::models;

fn cap_prompt_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect()
}

const WHISPER_BEAM_SIZE: i32 = 8;

#[tauri::command]
pub async fn check_model_status(
    app_handle: tauri::AppHandle,
    active: tauri::State<'_, models::ActiveModel>,
) -> Result<bool, String> {
    let active_id = models::active_model_id(&active);
    let model = models::find_model(&active_id).ok_or_else(|| "Unknown active model".to_string())?;
    Ok(models::is_downloaded(&app_handle, model))
}

#[tauri::command]
pub async fn download_whisper_model(app_handle: tauri::AppHandle) -> Result<(), String> {
    let model = models::find_model(models::DEFAULT_MODEL_ID)
        .ok_or_else(|| "Default model not found".to_string())?;
    models::download_model_file(&app_handle, model).await
}

fn read_wav_as_f32(path: &str) -> Result<Vec<f32>, String> {
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

#[derive(serde::Serialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub word_count: i64,
    pub processing_time_ms: i64,
    pub audio_duration_ms: Option<i64>,
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

#[tauri::command]
pub async fn transcribe_audio(
    app_handle: tauri::AppHandle,
    db_state: tauri::State<'_, crate::handlers::db::DbState>,
    active: tauri::State<'_, models::ActiveModel>,
    audio_path: String,
    bundle_id: Option<String>,
    domain: Option<String>,
    fallback_style: Option<String>,
) -> Result<TranscriptionResult, String> {
    let active_id = models::active_model_id(&active);
    let model = models::find_model(&active_id)
        .ok_or_else(|| format!("Unknown active model: {}", active_id))?;
    let model_path = models::model_path(&app_handle, model)?;

    if !model_path.exists() {
        return Err("Model not found. Please download it first.".to_string());
    }

    let start = std::time::Instant::now();

    let audio_data = read_wav_as_f32(&audio_path)?;
    let audio_duration_ms = if audio_data.len() > 0 {
        Some((audio_data.len() as f64 / 16.0) as i64)
    } else {
        None
    };

    // Engine dispatch. Whisper runs in-process via whisper-rs; Parakeet (a TDT
    // transducer that whisper-rs cannot run) is transcribed by the crispasr
    // engine on the already-downloaded GGUF. Style/dictionary prompts are
    // whisper-only — Parakeet auto-detects language and takes no prompt.
    if model.engine == models::Engine::Parakeet {
        let text = crate::handlers::parakeet::transcribe(
            &app_handle,
            &model_path,
            &audio_path,
            &audio_data,
        )?;
        let word_count = text.split_whitespace().count() as i64;
        let processing_time_ms = start.elapsed().as_millis() as i64;
        return Ok(TranscriptionResult {
            text,
            word_count,
            processing_time_ms,
            audio_duration_ms,
        });
    }

    let ctx_params = WhisperContextParameters::default();
    let ctx = WhisperContext::new_with_params(&model_path, ctx_params)
        .map_err(|e| format!("Failed to load model: {}", e))?;

    let thread_count = std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).max(1) as i32)
        .unwrap_or(4);

    let mut state = ctx
        .create_state()
        .map_err(|e| format!("Failed to create state: {}", e))?;

    // Step 1: encode mel + detect language so we can pick a same-language prompt.
    state
        .pcm_to_mel(&audio_data, thread_count as usize)
        .map_err(|e| format!("Failed to compute mel: {:?}", e))?;
    let detected_lang = match state.lang_detect(0, thread_count as usize) {
        Ok((lang_id, _probs)) => whisper_rs::get_lang_str(lang_id)
            .unwrap_or(crate::handlers::apps::DEFAULT_LOCALE)
            .to_string(),
        Err(e) => {
            eprintln!("[whisper] lang_detect failed: {:?}", e);
            crate::handlers::apps::DEFAULT_LOCALE.to_string()
        }
    };

    // Step 2: resolve the style key and compose the prompt in the detected language.
    let composed_prompt = {
        let conn = db_state.conn.lock().map_err(|e| e.to_string())?;
        let style_key = crate::handlers::apps::resolve_style_key(
            &conn,
            bundle_id.as_deref(),
            domain.as_deref(),
            fallback_style.as_deref(),
        );
        let style_text =
            crate::handlers::apps::style_prompt_text(&style_key, &detected_lang).to_string();
        let default_dict = fetch_dictionary_words(&conn);
        let app_dict = bundle_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|bid| crate::handlers::apps::fetch_app_dictionary_words(&conn, bid));
        let site_dict = domain
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|d| crate::handlers::sites::fetch_site_dictionary_words(&conn, d));
        let dictionary = merge_dictionaries(
            default_dict.as_deref(),
            app_dict.as_deref(),
            site_dict.as_deref(),
        );
        eprintln!(
            "[style] bundle={:?} domain={:?} style={} lang={} style_chars={} dict_words={} app_dict_words={} site_dict_words={}",
            bundle_id,
            domain,
            style_key,
            detected_lang,
            style_text.chars().count(),
            default_dict
                .as_ref()
                .map(|d| d.split(',').count())
                .unwrap_or(0),
            app_dict.as_ref().map(|d| d.split(',').count()).unwrap_or(0),
            site_dict
                .as_ref()
                .map(|d| d.split(',').count())
                .unwrap_or(0)
        );
        compose_prompt(&style_text, dictionary.as_deref())
    };

    // Step 3: run full transcription with the resolved prompt.
    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: WHISPER_BEAM_SIZE,
        patience: -1.0,
    });
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
    params.set_language(Some(&detected_lang));

    let capped_prompt = composed_prompt.as_deref().map(|p| cap_prompt_chars(p, 700));
    if let Some(ref p) = capped_prompt {
        let preview: String = p.chars().take(80).collect();
        eprintln!(
            "[whisper] initial_prompt: {} chars lang={} (head: {:?})",
            p.chars().count(),
            detected_lang,
            preview
        );
        params.set_initial_prompt(p);
    } else {
        eprintln!("[whisper] no initial_prompt lang={}", detected_lang);
    }

    state
        .full(params, &audio_data)
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

    let text = text.trim().to_string();
    let word_count = text.split_whitespace().count() as i64;
    let processing_time_ms = start.elapsed().as_millis() as i64;

    crate::handlers::audio::cleanup_old_recordings(
        &app_handle,
        crate::handlers::audio::RECORDINGS_TO_KEEP,
    );

    Ok(TranscriptionResult {
        text,
        word_count,
        processing_time_ms,
        audio_duration_ms,
    })
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
        parts.push(format!("{}", dict));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}
