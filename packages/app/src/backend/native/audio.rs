use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamError};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::fs::{File, create_dir_all};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::backend::{AppEvent, AudioDevice, EventSender};

type WavWriterHandle = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;

struct SafeStream(Stream);

unsafe impl Send for SafeStream {}
unsafe impl Sync for SafeStream {}

/// The dictation currently being captured. Stream callbacks and the input
/// watchdog carry the recording `id`, so late events from an earlier stream
/// cannot touch a newer recording.
struct Recording {
    id: u64,
    stream: SafeStream,
    writer: WavWriterHandle,
    save_path: PathBuf,
}

static RECORDING: Mutex<Option<Recording>> = Mutex::new(None);
static NEXT_RECORDING_ID: AtomicU64 = AtomicU64::new(1);

/// How long a newly opened input may take to deliver its first samples.
/// Bluetooth microphones switch profiles when opened and can take seconds.
const FIRST_INPUT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a running input may stop delivering samples before the device
/// is treated as lost. cpal does not report an unplugged device when it is
/// also the system default input; the stream only goes quiet.
const INPUT_STALL_TIMEOUT: Duration = Duration::from_secs(2);
const INPUT_WATCHDOG_INTERVAL: Duration = Duration::from_millis(250);
const NO_INPUT_YET: u64 = u64::MAX;

/// How many dictation WAVs to keep on disk (newest first). Everything older
/// is pruned after each successful transcription.
pub const RECORDINGS_TO_KEEP: usize = 20;

fn read_device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

pub fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();

    let default_device_name = host
        .default_input_device()
        .and_then(|d| read_device_name(&d));

    let devices = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .filter_map(|device| {
            read_device_name(&device).and_then(|name| {
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

fn input_device(host: &cpal::Host, device_name: Option<&str>) -> Result<cpal::Device, String> {
    if let Some(name) = device_name.filter(|name| *name != "default" && !name.is_empty()) {
        let found = host
            .input_devices()
            .map_err(|err| err.to_string())?
            .find(|device| read_device_name(device).as_deref() == Some(name));
        if let Some(device) = found {
            return Ok(device);
        }
        // The saved device may have been unplugged; fall back to the system
        // default input instead of failing the dictation.
        log::warn!(
            target: "audio",
            "input device not found, falling back to default device_name={name:?}"
        );
    }

    host.default_input_device()
        .ok_or_else(|| "No input device available".to_string())
}

pub fn start_recording_with_device(
    events: &EventSender,
    device_name: Option<String>,
) -> Result<(), String> {
    let mut active = RECORDING.lock().map_err(|err| err.to_string())?;
    if active.is_some() {
        return Err("Recording is already in progress.".to_string());
    }

    let host = cpal::default_host();
    let device = input_device(&host, device_name.as_deref())?;
    let config: cpal::StreamConfig = device
        .default_input_config()
        .map_err(|err| err.to_string())?
        .config();

    // Capture as f32 whatever the hardware format is; CoreAudio converts, and
    // the WAV spec stays independent of the device.
    let spec = WavSpec {
        channels: config.channels,
        sample_rate: config.sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let save_path = get_save_path()?;
    let writer = WavWriter::create(&save_path, spec).map_err(|err| err.to_string())?;
    let writer: WavWriterHandle = Arc::new(Mutex::new(Some(writer)));

    let id = NEXT_RECORDING_ID.fetch_add(1, Ordering::Relaxed);
    let last_input_ms = Arc::new(AtomicU64::new(NO_INPUT_YET));

    let data_writer = writer.clone();
    let data_events = events.clone();
    let data_last_input_ms = last_input_ms.clone();
    let mut last_level_emit_ms = 0;

    // Surface stream errors (e.g. mic unplugged mid-recording) to the interface
    // and release the recorder so the next hotkey press can start fresh.
    let err_events = events.clone();
    let err_fn = move |err: StreamError| {
        if matches!(err, StreamError::BufferUnderrun) {
            log::warn!(target: "audio", "recording stream overrun id={id}");
            return;
        }
        // Errors can arrive on CoreAudio's realtime thread; tear down elsewhere.
        let events = err_events.clone();
        let message = err.to_string();
        std::thread::spawn(move || fail_recording(id, &events, message));
    };

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                data_last_input_ms.store(elapsed_ms(), Ordering::Relaxed);
                write_input_data_with_levels(
                    data,
                    &data_writer,
                    &data_events,
                    &mut last_level_emit_ms,
                );
            },
            err_fn,
            None,
        )
        .map_err(|err| err.to_string())
        .and_then(|stream| {
            stream.play().map_err(|err| err.to_string())?;
            Ok(stream)
        });
    let stream = match stream {
        Ok(stream) => stream,
        Err(err) => {
            discard_recording_file(&writer, &save_path);
            return Err(err);
        }
    };

    // Only publish the recording once the stream is running — any error above
    // leaves the recorder ready for a retry.
    *active = Some(Recording {
        id,
        stream: SafeStream(stream),
        writer,
        save_path,
    });
    drop(active);

    log::info!(
        target: "audio",
        "recording_started id={id} device={:?} sample_rate={} channels={}",
        read_device_name(&device).unwrap_or_default(),
        config.sample_rate,
        config.channels
    );

    let watchdog_events = events.clone();
    let spawned = std::thread::Builder::new()
        .name("audio-input-watchdog".into())
        .spawn(move || watch_input(id, watchdog_events, last_input_ms));
    if let Err(err) = spawned {
        log::warn!(target: "audio", "input watchdog not started id={id} error={err}");
    }

    Ok(())
}

pub fn stop_recording_with_device() -> Result<PathBuf, String> {
    let recording = RECORDING
        .lock()
        .map_err(|err| err.to_string())?
        .take()
        .ok_or("No recording in progress.".to_string())?;

    // Take the writer first so callbacks still in flight stop writing.
    let writer = recording
        .writer
        .lock()
        .map_err(|err| err.to_string())?
        .take();
    close_stream(recording.stream);

    if let Some(writer) = writer {
        writer.finalize().map_err(|err| err.to_string())?;
    }

    Ok(recording.save_path)
}

/// Ends recording `id` after its input failed: releases the device, drops the
/// partial WAV and tells the interface. A no-op if that recording already ended.
fn fail_recording(id: u64, events: &EventSender, message: String) {
    let recording = match RECORDING.lock() {
        Ok(mut active) if active.as_ref().is_some_and(|recording| recording.id == id) => {
            active.take()
        }
        _ => None,
    };
    let Some(recording) = recording else {
        return;
    };

    log::error!(target: "audio", "recording stream error id={id}: {message}");
    discard_recording_file(&recording.writer, &recording.save_path);
    close_stream(recording.stream);
    events.emit(AppEvent::RecordingError { message });
}

/// Pauses and drops a capture stream on a detached thread. CoreAudio teardown
/// can block on a device that has just disappeared, and that must not wedge
/// the recorder or the command that stopped it.
fn close_stream(stream: SafeStream) {
    let spawned = std::thread::Builder::new()
        .name("audio-stream-close".into())
        .spawn(move || {
            let _ = stream.0.pause();
            drop(stream);
        });
    if let Err(err) = spawned {
        log::warn!(target: "audio", "failed to spawn stream close thread: {err}");
    }
}

fn discard_recording_file(writer: &WavWriterHandle, save_path: &Path) {
    if let Ok(mut writer) = writer.lock() {
        writer.take();
    }
    if let Err(err) = std::fs::remove_file(save_path) {
        log::warn!(
            target: "audio",
            "failed to delete discarded recording {}: {err}",
            save_path.display()
        );
    }
}

/// Fails recording `id` if its input stops delivering samples, which is how
/// an unplugged system-default microphone shows up.
fn watch_input(id: u64, events: EventSender, last_input_ms: Arc<AtomicU64>) {
    let started_ms = elapsed_ms();
    loop {
        std::thread::sleep(INPUT_WATCHDOG_INTERVAL);

        let is_active = RECORDING
            .lock()
            .map(|active| active.as_ref().is_some_and(|recording| recording.id == id))
            .unwrap_or(false);
        if !is_active {
            return;
        }

        let now_ms = elapsed_ms();
        let last_ms = last_input_ms.load(Ordering::Relaxed);
        let (silent_for, timeout) = if last_ms == NO_INPUT_YET {
            (now_ms.saturating_sub(started_ms), FIRST_INPUT_TIMEOUT)
        } else {
            (now_ms.saturating_sub(last_ms), INPUT_STALL_TIMEOUT)
        };
        if silent_for > timeout.as_millis() as u64 {
            fail_recording(
                id,
                &events,
                format!("Input device delivered no audio for {silent_for} ms"),
            );
            return;
        }
    }
}

fn recordings_dir() -> PathBuf {
    crate::paths::data_dir().join("recordings")
}

fn get_save_path() -> Result<PathBuf, String> {
    let save_dir = recordings_dir();

    create_dir_all(&save_dir).map_err(|err| err.to_string())?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    let save_path = save_dir.join(format!("{timestamp}.wav"));

    Ok(save_path)
}

/// Prunes the recordings directory down to the `keep` newest `.wav` files by
/// modification time. Called after each successful transcription so dictation
/// audio is not retained forever. IO failures are logged and otherwise
/// ignored — cleanup must never fail a transcription.
pub fn cleanup_old_recordings(keep: usize) {
    let entries = match std::fs::read_dir(recordings_dir()) {
        Ok(entries) => entries,
        Err(err) => {
            log::warn!(target: "audio", "recordings cleanup failed to read dir: {err}");
            return;
        }
    };

    let mut recordings: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("wav") {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();

    if recordings.len() <= keep {
        return;
    }

    // Newest first; everything past `keep` gets deleted.
    recordings.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in recordings.drain(keep..) {
        if let Err(err) = std::fs::remove_file(&path) {
            log::warn!(
                target: "audio",
                "failed to delete old recording {}: {err}",
                path.display()
            );
        }
    }
}

// Timestamp tracking for throttling - using a static Instant for reference
static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

fn elapsed_ms() -> u64 {
    START_TIME.elapsed().as_millis() as u64
}

fn write_input_data_with_levels(
    input: &[f32],
    writer: &WavWriterHandle,
    events: &EventSender,
    last_emit_ms: &mut u64,
) {
    if input.is_empty() {
        return;
    }
    if let Ok(mut guard) = writer.try_lock()
        && let Some(writer) = guard.as_mut()
    {
        // Calculate RMS (root mean square) for audio level
        let mut sum: f64 = 0.0;
        let mut peak: f64 = 0.0;

        for &sample in input.iter() {
            writer.write_sample(sample).ok();

            let value_f64 = sample as f64;
            sum += value_f64 * value_f64;
            peak = peak.max(value_f64.abs());
        }

        // Calculate RMS and normalize to 0-100 range
        let rms = (sum / input.len() as f64).sqrt();
        // Use a combination of RMS and peak for more responsive visualization
        let level = ((rms * 0.7 + peak * 0.3) * 150.0).min(100.0);

        // Throttle to ~30fps (every ~33ms)
        let now = elapsed_ms();
        if now - *last_emit_ms >= 33 {
            *last_emit_ms = now;
            events.emit(AppEvent::AudioLevel(level as f32));
        }
    }
}
