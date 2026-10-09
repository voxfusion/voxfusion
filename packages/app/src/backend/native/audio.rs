use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamError};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::fs::{File, create_dir_all};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::microphones::{self, InputKind};
use crate::backend::{AppEvent, AudioDevice, EventSender};

type Wav = WavWriter<BufWriter<File>>;
type WavWriterHandle = Arc<Mutex<Option<Wav>>>;

struct SafeStream(Stream);

unsafe impl Send for SafeStream {}
unsafe impl Sync for SafeStream {}

/// The dictation currently being captured. Stream callbacks and the input
/// watchdog carry the recording `id`, so late events from an earlier stream
/// cannot touch a newer recording.
struct Recording {
    sink: Sink,
    input: Input,
    /// Set while another microphone is being opened to take over from the
    /// built-in one, which the lid has disconnected; the watchdog waits.
    switching_input: bool,
    save_path: PathBuf,
}

/// Where a recording's samples go, whichever microphone they come from.
#[derive(Clone)]
struct Sink {
    id: u64,
    writer: WavWriterHandle,
    /// The file's format, which a microphone that takes over is converted to.
    spec: WavSpec,
    events: EventSender,
}

/// A microphone feeding a recording.
struct Input {
    stream: SafeStream,
    kind: InputKind,
    clock: Arc<InputClock>,
    /// Cleared when another microphone takes over the recording. A callback
    /// of this stream still in flight then writes nothing, and its errors no
    /// longer end the recording.
    current: Arc<AtomicBool>,
}

/// When an input was opened and when it last delivered samples.
struct InputClock {
    opened_ms: u64,
    last_input_ms: AtomicU64,
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

/// The microphone a recording uses: the one chosen in the settings, or the
/// system default, unless that is the built-in microphone and the lid is
/// closed.
pub(super) fn input_device(
    host: &cpal::Host,
    device_name: Option<&str>,
) -> Result<(cpal::Device, InputKind), String> {
    let device = chosen_input_device(host, device_name)?;
    let kind = microphones::kind(&device);
    if kind != InputKind::BuiltIn || !microphones::lid_closed() {
        return Ok((device, kind));
    }

    match microphones::replacement(host) {
        Some((replacement, replacement_kind)) => {
            log::info!(
                target: "audio",
                "built_in_microphone_skipped reason=lid_closed device={:?} kind={replacement_kind:?}",
                read_device_name(&replacement).unwrap_or_default()
            );
            Ok((replacement, replacement_kind))
        }
        None => {
            log::warn!(target: "audio", "built_in_microphone_lid_closed no_other_microphone");
            Ok((device, kind))
        }
    }
}

fn chosen_input_device(
    host: &cpal::Host,
    device_name: Option<&str>,
) -> Result<cpal::Device, String> {
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
    let (device, kind) = input_device(&host, device_name.as_deref())?;
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
    let sink = Sink {
        id,
        writer,
        spec,
        events: events.clone(),
    };
    let input = match open_input(&sink, &device, kind, &config, true) {
        Ok(input) => input,
        Err(err) => {
            discard_recording_file(&sink.writer, &save_path);
            return Err(err);
        }
    };

    // Only publish the recording once the stream is running — any error above
    // leaves the recorder ready for a retry.
    *active = Some(Recording {
        sink,
        input,
        switching_input: false,
        save_path,
    });
    drop(active);

    log::info!(
        target: "audio",
        "recording_started id={id} device={:?} kind={kind:?} sample_rate={} channels={}",
        read_device_name(&device).unwrap_or_default(),
        config.sample_rate,
        config.channels
    );

    let watchdog_events = events.clone();
    let spawned = std::thread::Builder::new()
        .name("audio-input-watchdog".into())
        .spawn(move || watch_input(id, watchdog_events));
    if let Err(err) = spawned {
        log::warn!(target: "audio", "input watchdog not started id={id} error={err}");
    }

    Ok(())
}

/// Opens `device` to write into `sink`, converted to its format when the
/// device's differs. Unless it is `current`, the stream writes nothing until
/// it is made so.
fn open_input(
    sink: &Sink,
    device: &cpal::Device,
    kind: InputKind,
    config: &cpal::StreamConfig,
    current: bool,
) -> Result<Input, String> {
    let id = sink.id;
    let clock = Arc::new(InputClock {
        opened_ms: elapsed_ms(),
        last_input_ms: AtomicU64::new(NO_INPUT_YET),
    });
    let current = Arc::new(AtomicBool::new(current));

    let data_writer = sink.writer.clone();
    let data_events = sink.events.clone();
    let data_clock = clock.clone();
    let data_current = current.clone();
    let mut converter = Converter::new(
        config.channels,
        config.sample_rate,
        sink.spec.channels,
        sink.spec.sample_rate,
    );
    let mut last_level_emit_ms = 0;

    // Surface stream errors (e.g. mic unplugged mid-recording) to the interface
    // and release the recorder so the next hotkey press can start fresh.
    let err_events = sink.events.clone();
    let err_current = current.clone();
    let err_fn = move |err: StreamError| {
        if matches!(err, StreamError::BufferUnderrun) {
            log::warn!(target: "audio", "recording stream overrun id={id}");
            return;
        }
        // A microphone that was taken over from may fail as it goes away.
        if !err_current.load(Ordering::Relaxed) {
            log::info!(target: "audio", "inactive input stream error id={id}: {err}");
            return;
        }
        // Errors can arrive on CoreAudio's realtime thread; tear down elsewhere.
        let events = err_events.clone();
        let message = err.to_string();
        std::thread::spawn(move || fail_recording(id, &events, message));
    };

    let stream = device
        .build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                data_clock
                    .last_input_ms
                    .store(elapsed_ms(), Ordering::Relaxed);
                // `current` is read under the writer's lock, which switching
                // microphones takes: no sample of the previous microphone
                // follows one of the next.
                if let Ok(mut guard) = data_writer.try_lock()
                    && data_current.load(Ordering::Relaxed)
                    && let Some(writer) = guard.as_mut()
                {
                    write_input_data_with_levels(
                        converter.convert(data),
                        writer,
                        &data_events,
                        &mut last_level_emit_ms,
                    );
                }
            },
            err_fn,
            None,
        )
        .map_err(|err| err.to_string())?;
    stream.play().map_err(|err| err.to_string())?;

    Ok(Input {
        stream: SafeStream(stream),
        kind,
        clock,
        current,
    })
}

/// Reports a lid change. When the lid closes during a recording from the
/// built-in microphone, another microphone takes over.
pub fn lid_changed(closed: bool) {
    log::info!(target: "audio", "lid_changed closed={closed}");
    if !closed {
        return;
    }
    // This runs on IOKit's notification queue, and opening a microphone can
    // take seconds.
    let spawned = std::thread::Builder::new()
        .name("audio-input-switch".into())
        .spawn(take_over_from_built_in_microphone);
    if let Err(err) = spawned {
        log::warn!(target: "audio", "failed to spawn input switch thread: {err}");
    }
}

/// Moves the running recording from the built-in microphone to another. What
/// was said so far stays in the file, and the next microphone's samples are
/// converted to its format.
fn take_over_from_built_in_microphone() {
    let sink = RECORDING.lock().ok().and_then(|mut active| {
        let recording = active
            .as_mut()
            .filter(|recording| recording.input.kind == InputKind::BuiltIn)?;
        recording.switching_input = true;
        Some(recording.sink.clone())
    });
    let Some(sink) = sink else {
        return;
    };
    let id = sink.id;

    let host = cpal::default_host();
    let input = microphones::replacement(&host)
        .ok_or_else(|| "no other microphone".to_string())
        .and_then(|(device, kind)| {
            let config = device
                .default_input_config()
                .map_err(|err| err.to_string())?
                .config();
            let input = open_input(&sink, &device, kind, &config, false)?;
            Ok((input, read_device_name(&device).unwrap_or_default(), config))
        });

    let Ok(mut active) = RECORDING.lock() else {
        return;
    };
    let Some(recording) = active.as_mut().filter(|recording| recording.sink.id == id) else {
        // The recording ended meanwhile.
        drop(active);
        if let Ok((input, _, _)) = input {
            close_stream(input.stream);
        }
        return;
    };
    recording.switching_input = false;

    let (input, device_name, config) = match input {
        Ok(input) => input,
        Err(err) => {
            log::warn!(target: "audio", "input_switch_failed id={id} reason=lid_closed error={err}");
            return;
        }
    };
    {
        let _writer = recording.sink.writer.lock();
        recording.input.current.store(false, Ordering::Relaxed);
        input.current.store(true, Ordering::Relaxed);
    }
    let kind = input.kind;
    let previous = std::mem::replace(&mut recording.input, input);
    drop(active);
    close_stream(previous.stream);

    log::info!(
        target: "audio",
        "input_switched id={id} reason=lid_closed device={device_name:?} kind={kind:?} sample_rate={} channels={}",
        config.sample_rate,
        config.channels
    );
}

pub fn stop_recording_with_device() -> Result<PathBuf, String> {
    let recording = RECORDING
        .lock()
        .map_err(|err| err.to_string())?
        .take()
        .ok_or("No recording in progress.".to_string())?;

    // Take the writer first so callbacks still in flight stop writing.
    let writer = recording
        .sink
        .writer
        .lock()
        .map_err(|err| err.to_string())?
        .take();
    close_stream(recording.input.stream);

    if let Some(writer) = writer {
        writer.finalize().map_err(|err| err.to_string())?;
    }

    Ok(recording.save_path)
}

/// Ends recording `id` after its input failed: releases the device, drops the
/// partial WAV and tells the interface. A no-op if that recording already ended.
fn fail_recording(id: u64, events: &EventSender, message: String) {
    let recording = match RECORDING.lock() {
        Ok(mut active)
            if active
                .as_ref()
                .is_some_and(|recording| recording.sink.id == id) =>
        {
            active.take()
        }
        _ => None,
    };
    let Some(recording) = recording else {
        return;
    };

    log::error!(target: "audio", "recording stream error id={id}: {message}");
    discard_recording_file(&recording.sink.writer, &recording.save_path);
    close_stream(recording.input.stream);
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
fn watch_input(id: u64, events: EventSender) {
    loop {
        std::thread::sleep(INPUT_WATCHDOG_INTERVAL);

        // The recording's current input, unless it has ended.
        let clock = match RECORDING.lock() {
            Ok(active) => match active.as_ref().filter(|recording| recording.sink.id == id) {
                Some(recording) if recording.switching_input => continue,
                Some(recording) => recording.input.clock.clone(),
                None => return,
            },
            Err(_) => return,
        };

        let now_ms = elapsed_ms();
        let last_ms = clock.last_input_ms.load(Ordering::Relaxed);
        let (silent_for, timeout) = if last_ms == NO_INPUT_YET {
            (now_ms.saturating_sub(clock.opened_ms), FIRST_INPUT_TIMEOUT)
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
    writer: &mut Wav,
    events: &EventSender,
    last_emit_ms: &mut u64,
) {
    if input.is_empty() {
        return;
    }

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

/// Brings a microphone's samples to the recording's format when a microphone
/// with another format takes over mid-recording: mixes the channels down,
/// resamples linearly and repeats the result in every channel of the file.
/// Transcription mixes down and resamples to 16 kHz anyway.
struct Converter {
    from_channels: usize,
    to_channels: usize,
    /// Input frames per output frame.
    step: f64,
    /// Where the next output frame falls, in input frames from the start of
    /// the next buffer. Below zero it falls between the previous buffer's
    /// last frame and the next buffer's first.
    position: f64,
    /// The previous buffer's last frame, mixed down.
    previous: f32,
    output: Vec<f32>,
}

impl Converter {
    fn new(from_channels: u16, from_rate: u32, to_channels: u16, to_rate: u32) -> Self {
        Self {
            from_channels: usize::from(from_channels.max(1)),
            to_channels: usize::from(to_channels.max(1)),
            step: f64::from(from_rate.max(1)) / f64::from(to_rate.max(1)),
            position: 0.0,
            previous: 0.0,
            output: Vec::new(),
        }
    }

    fn convert<'a>(&'a mut self, input: &'a [f32]) -> &'a [f32] {
        if self.from_channels == self.to_channels && self.step == 1.0 {
            return input;
        }

        let channels = self.from_channels;
        let frames = input.len() / channels;
        let mono = |frame: usize| {
            input[frame * channels..(frame + 1) * channels]
                .iter()
                .sum::<f32>()
                / channels as f32
        };

        self.output.clear();
        if frames == 0 {
            return &self.output;
        }

        let last = (frames - 1) as f64;
        while self.position <= last {
            let index = self.position.floor();
            let fraction = (self.position - index) as f32;
            let before = if index < 0.0 {
                self.previous
            } else {
                mono(index as usize)
            };
            let sample = if fraction == 0.0 {
                before
            } else {
                let after = mono((index + 1.0) as usize);
                before + (after - before) * fraction
            };

            self.output
                .extend(std::iter::repeat_n(sample, self.to_channels));
            self.position += self.step;
        }

        self.position -= frames as f64;
        self.previous = mono(frames - 1);
        &self.output
    }
}

#[cfg(test)]
mod tests {
    use super::Converter;

    fn convert_in_chunks(converter: &mut Converter, input: &[f32], chunk: usize) -> Vec<f32> {
        input
            .chunks(chunk)
            .flat_map(|chunk| converter.convert(chunk).to_vec())
            .collect()
    }

    #[test]
    fn the_same_format_passes_through() {
        let mut converter = Converter::new(2, 48_000, 2, 48_000);
        let input = [0.1, 0.2, 0.3, 0.4];

        assert_eq!(converter.convert(&input), &input);
    }

    #[test]
    fn channels_are_mixed_down_and_repeated() {
        let mut stereo_to_mono = Converter::new(2, 48_000, 1, 48_000);
        assert_eq!(stereo_to_mono.convert(&[1.0, 3.0, 2.0, 4.0]), &[2.0, 3.0]);

        let mut mono_to_stereo = Converter::new(1, 48_000, 2, 48_000);
        assert_eq!(mono_to_stereo.convert(&[1.0, 2.0]), &[1.0, 1.0, 2.0, 2.0]);
    }

    #[test]
    fn a_faster_input_is_thinned_out() {
        let mut converter = Converter::new(1, 48_000, 1, 24_000);
        let ramp: Vec<f32> = (0..8).map(|n| n as f32).collect();

        assert_eq!(converter.convert(&ramp), &[0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn a_slower_input_is_interpolated_across_buffers() {
        let ramp: Vec<f32> = (0..60).map(|n| n as f32).collect();
        let output = convert_in_chunks(&mut Converter::new(1, 12_000, 1, 48_000), &ramp, 7);

        // Every output frame is a quarter of an input frame further along.
        assert_eq!(output.len(), 237);
        for (index, sample) in output.iter().enumerate() {
            assert_eq!(*sample, index as f32 / 4.0);
        }
    }

    #[test]
    fn buffer_boundaries_do_not_change_the_result() {
        let signal: Vec<f32> = (0..480).map(|n| (n as f32 * 0.37).sin()).collect();
        let whole = Converter::new(2, 48_000, 1, 32_000)
            .convert(&signal)
            .to_vec();
        assert_eq!(whole.len(), 160);

        for chunk in [2, 10, 64, 130] {
            let chunked =
                convert_in_chunks(&mut Converter::new(2, 48_000, 1, 32_000), &signal, chunk);
            assert_eq!(chunked.len(), whole.len(), "chunks of {chunk}");
            for (a, b) in chunked.iter().zip(&whole) {
                assert!((a - b).abs() < 1e-5, "chunks of {chunk}");
            }
        }
    }
}
