use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, Stream};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde::Serialize;
use std::fs::{File, create_dir_all};
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;
use tauri::Emitter;

type WavWriterHandle = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;

struct SafeStream(Stream);

unsafe impl Send for SafeStream {}
unsafe impl Sync for SafeStream {}

struct RecordingState {
    is_recording: Arc<AtomicBool>,
    save_path: Arc<Mutex<Option<PathBuf>>>,
    writer: WavWriterHandle,
    stream: Arc<Mutex<Option<SafeStream>>>,
    app_handle: Arc<Mutex<Option<tauri::AppHandle>>>,
    last_emit_time: Arc<AtomicU64>,
}

impl RecordingState {
    fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            save_path: Arc::new(Mutex::new(None)),
            writer: Arc::new(Mutex::new(None)),
            stream: Arc::new(Mutex::new(None)),
            app_handle: Arc::new(Mutex::new(None)),
            last_emit_time: Arc::new(AtomicU64::new(0)),
        }
    }
}

static RECORDING_STATE: LazyLock<Arc<Mutex<RecordingState>>> =
    LazyLock::new(|| Arc::new(Mutex::new(RecordingState::new())));

#[derive(Serialize, Clone, Default)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
}

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();

    let default_device_name = host.default_input_device().and_then(|d| d.name().ok());

    let devices = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .filter_map(|device| {
            device.name().ok().and_then(|name| {
                // Filter out inactive capture devices
                if name.contains("Capture Inactive") {
                    return None;
                }
                Some(AudioDevice {
                    is_default: default_device_name.as_ref() == Some(&name),
                    name,
                })
            })
        })
        .collect();

    Ok(devices)
}

fn default_input_device(host: &cpal::Host) -> Result<cpal::Device, String> {
    host.default_input_device()
        .ok_or("No default input device available".to_string())
}

fn input_device_by_name(host: &cpal::Host, name: &str) -> Result<Option<cpal::Device>, String> {
    let device = host
        .input_devices()
        .map_err(|err| err.to_string())?
        .find(|device| {
            device
                .name()
                .map(|device_name| device_name == name)
                .unwrap_or(false)
        });

    Ok(device)
}

fn input_device_or_default(
    host: &cpal::Host,
    device_name: Option<&str>,
) -> Result<cpal::Device, String> {
    match device_name {
        Some(name) if !name.is_empty() && name != "default" => {
            input_device_by_name(host, name)?.map_or_else(|| default_input_device(host), Ok)
        }
        _ => default_input_device(host),
    }
}

#[tauri::command]
pub async fn start_recording_with_device(
    app_handle: tauri::AppHandle,
    device_name: Option<String>,
) -> Result<(), String> {
    let mut state = RECORDING_STATE.lock().map_err(|err| err.to_string())?;
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Recording is already in progress.".to_string());
    }
    state.is_recording.store(true, Ordering::SeqCst);

    // Store app handle for emitting events
    *state.app_handle.lock().map_err(|err| err.to_string())? = Some(app_handle.clone());

    let host = cpal::default_host();
    let device = input_device_or_default(&host, device_name.as_deref())?;

    let config = device
        .default_input_config()
        .map_err(|err| err.to_string())?;

    let save_path = get_save_path(&app_handle)?;
    let spec = wav_spec_from_config(&config);
    let writer = WavWriter::create(&save_path, spec).map_err(|err| err.to_string())?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    let writer_2 = writer.clone();
    let app_handle_2 = state.app_handle.clone();
    let last_emit_time = state.last_emit_time.clone();

    let err_fn = move |err: cpal::StreamError| {
        eprintln!("an error occurred on stream: {}", err);
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => device
            .build_input_stream(
                &config.into(),
                move |data, _: &_| {
                    write_input_data_with_levels::<i8, i8>(
                        data,
                        &writer_2,
                        &app_handle_2,
                        &last_emit_time,
                    )
                },
                err_fn,
                None,
            )
            .map_err(|err| err.to_string())?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &config.into(),
                move |data, _: &_| {
                    write_input_data_with_levels::<i16, i16>(
                        data,
                        &writer_2,
                        &app_handle_2,
                        &last_emit_time,
                    )
                },
                err_fn,
                None,
            )
            .map_err(|err| err.to_string())?,
        cpal::SampleFormat::I32 => device
            .build_input_stream(
                &config.into(),
                move |data, _: &_| {
                    write_input_data_with_levels::<i32, i32>(
                        data,
                        &writer_2,
                        &app_handle_2,
                        &last_emit_time,
                    )
                },
                err_fn,
                None,
            )
            .map_err(|err| err.to_string())?,
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &config.into(),
                move |data, _: &_| {
                    write_input_data_with_levels::<f32, f32>(
                        data,
                        &writer_2,
                        &app_handle_2,
                        &last_emit_time,
                    )
                },
                err_fn,
                None,
            )
            .map_err(|err| err.to_string())?,
        _ => return Err("Unsupported sample format".to_string()),
    };

    stream.play().map_err(|err| err.to_string())?;

    *state.save_path.lock().map_err(|err| err.to_string())? = Some(save_path);
    state.writer = writer;
    *state.stream.lock().map_err(|err| err.to_string())? = Some(SafeStream(stream));

    Ok(())
}

#[tauri::command]
pub async fn stop_recording_with_device() -> Result<PathBuf, String> {
    // Extract resources while holding the lock, then release lock before dropping
    let (stream_to_drop, writer_to_finalize, save_path) = {
        let state = RECORDING_STATE.lock().map_err(|err| err.to_string())?;
        if !state.is_recording.load(Ordering::SeqCst) {
            return Err("No recording in progress.".to_string());
        }
        state.is_recording.store(false, Ordering::SeqCst);

        // Take the stream
        let stream = state.stream.lock().map_err(|err| err.to_string())?.take();

        // Take the writer
        let writer = state.writer.lock().map_err(|err| err.to_string())?.take();

        // Clear app handle
        *state.app_handle.lock().map_err(|err| err.to_string())? = None;

        // Get and clear the save path
        let save_path = state
            .save_path
            .lock()
            .map_err(|err| err.to_string())?
            .take()
            .ok_or("No recording in progress or save path not set.".to_string())?;

        (stream, writer, save_path)
    };
    // Lock is now released

    // Stop the stream - pause first, then drop to ensure macOS releases microphone
    if let Some(stream) = stream_to_drop {
        let _ = stream.0.pause();
        drop(stream.0);
    }

    // Finalize the writer
    if let Some(writer) = writer_to_finalize {
        writer.finalize().map_err(|err| err.to_string())?;
    }

    Ok(save_path)
}

fn get_save_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;

    let save_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|err| err.to_string())?
        .join("recordings");

    create_dir_all(&save_dir).map_err(|err| err.to_string())?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    let save_path = save_dir.join(format!("{timestamp}.wav"));

    Ok(save_path)
}

fn sample_format(format: cpal::SampleFormat) -> SampleFormat {
    if format.is_float() {
        SampleFormat::Float
    } else {
        SampleFormat::Int
    }
}

fn wav_spec_from_config(config: &cpal::SupportedStreamConfig) -> WavSpec {
    WavSpec {
        channels: config.channels() as _,
        sample_rate: config.sample_rate().0 as _,
        bits_per_sample: (config.sample_format().sample_size() * 8) as _,
        sample_format: sample_format(config.sample_format()),
    }
}

// Timestamp tracking for throttling - using a static Instant for reference
static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

fn write_input_data_with_levels<T, U>(
    input: &[T],
    writer: &WavWriterHandle,
    app_handle: &Arc<Mutex<Option<tauri::AppHandle>>>,
    last_emit_time: &Arc<AtomicU64>,
) where
    T: Sample,
    U: Sample + hound::Sample + FromSample<T>,
{
    if let Ok(mut guard) = writer.try_lock() {
        if let Some(writer) = guard.as_mut() {
            // Calculate RMS (root mean square) for audio level
            let mut sum: f64 = 0.0;
            let mut peak: f64 = 0.0;

            for &sample in input.iter() {
                let sample_u: U = U::from_sample(sample);
                writer.write_sample(sample_u).ok();

                // Convert to f32 first, then to f64 for level calculation
                let value: f32 = sample.to_float_sample().to_sample();
                let value_f64 = value as f64;
                sum += value_f64 * value_f64;
                peak = peak.max(value_f64.abs());
            }

            // Calculate RMS and normalize to 0-100 range
            let rms = (sum / input.len() as f64).sqrt();
            // Use a combination of RMS and peak for more responsive visualization
            let level = ((rms * 0.7 + peak * 0.3) * 150.0).min(100.0);

            // Throttle to ~30fps (every ~33ms)
            let now = START_TIME.elapsed().as_millis() as u64;
            let last = last_emit_time.load(Ordering::Relaxed);
            if now - last >= 33 {
                last_emit_time.store(now, Ordering::Relaxed);

                // Emit to frontend
                if let Ok(handle_guard) = app_handle.try_lock() {
                    if let Some(h) = handle_guard.as_ref() {
                        let _ = h.emit("audio-level", level);
                    }
                }
            }
        }
    }
}
