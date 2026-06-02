//! On-device Parakeet (TDT transducer) transcription.
//!
//! whisper-rs cannot run Parakeet, so we drive the GGUF through the `crispasr`
//! engine (CrispStrobe/CrispASR — a ggml/Metal C++ runtime, the same engine the
//! `cstr/parakeet-tdt-0.6b-v3-GGUF` files target). We invoke its CLI as a
//! subprocess: `crispasr -m <model.gguf> -f <audio.wav> -nt -np`, which prints
//! the transcription (no timestamps) to stdout. The model backend is
//! auto-detected from the GGUF metadata.

use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::Manager;

const BIN_NAME: &str = "crispasr";

/// Resolves the `crispasr` engine binary. Search order:
/// 1. `VOXFUSION_PARAKEET_BIN` env override (absolute path)
/// 2. bundled resource: `<resources>/bin/crispasr`
/// 3. app data: `<app_data>/bin/crispasr`
/// 4. bare name (resolved against `PATH`)
fn resolve_binary(app_handle: &tauri::AppHandle) -> PathBuf {
    if let Ok(path) = std::env::var("VOXFUSION_PARAKEET_BIN") {
        let pb = PathBuf::from(path);
        if pb.exists() {
            return pb;
        }
    }
    if let Ok(resources) = app_handle.path().resource_dir() {
        let pb = resources.join("bin").join(BIN_NAME);
        if pb.exists() {
            return pb;
        }
    }
    if let Ok(data_dir) = app_handle.path().app_data_dir() {
        let pb = data_dir.join("bin").join(BIN_NAME);
        if pb.exists() {
            return pb;
        }
    }
    PathBuf::from(BIN_NAME)
}

/// Writes mono 16 kHz f32 samples as a 16-bit PCM WAV the engine can read,
/// alongside the source file. Returns the temp path (caller deletes it).
fn write_temp_wav_16k(audio_path: &str, samples: &[f32]) -> Result<PathBuf, String> {
    let temp_path = PathBuf::from(audio_path).with_extension("parakeet16k.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&temp_path, spec)
        .map_err(|e| format!("Failed to create temp WAV: {}", e))?;
    for &sample in samples {
        let scaled = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        writer
            .write_sample(scaled)
            .map_err(|e| format!("Failed to write WAV sample: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize temp WAV: {}", e))?;
    Ok(temp_path)
}

/// Transcribes the given 16 kHz mono samples with the Parakeet GGUF at
/// `model_path` via the `crispasr` engine. Returns the trimmed transcript.
pub fn transcribe(
    app_handle: &tauri::AppHandle,
    model_path: &Path,
    audio_path: &str,
    samples: &[f32],
) -> Result<String, String> {
    let binary = resolve_binary(app_handle);
    let wav_path = write_temp_wav_16k(audio_path, samples)?;

    let thread_count = std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).max(1))
        .unwrap_or(4)
        .to_string();

    let result = Command::new(&binary)
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(&wav_path)
        .arg("-t")
        .arg(&thread_count)
        .arg("-nt") // no timestamps
        .arg("-np") // print only the transcription
        .output();

    let _ = std::fs::remove_file(&wav_path);

    let output = result.map_err(|e| {
        format!(
            "Failed to launch Parakeet engine ({}): {}. The 'crispasr' engine binary is required to run Parakeet models.",
            binary.display(),
            e
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.lines().rev().take(5).collect::<Vec<_>>().join(" | ");
        return Err(format!("Parakeet engine failed: {}", tail.trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("Parakeet engine produced no transcription.".to_string());
    }
    Ok(text)
}
