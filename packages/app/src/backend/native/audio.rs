use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamError};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::fs::{File, create_dir_all};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use super::microphones::{self, InputKind};
use super::permissions;
use crate::backend::{
    AppEvent, AudioDevice, EventSender, MICROPHONE_PERMISSION_DENIED_ERROR,
    MICROPHONE_TEST_RECORDING_IN_PROGRESS_ERROR, PermissionState, next_microphone_test_session,
};

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

/// Coordinates exclusive capture ownership and detached teardown between
/// dictation and live microphone tests without ever holding a lock across
/// CoreAudio open, `play()`, `pause()`, or stream drop calls.
///
/// Lock discipline: `RECORDING` and `CAPTURE` are never held simultaneously.
struct CaptureState {
    /// Set while dictation is starting or actively recording.
    recording: bool,
    /// Lid-close handover threads currently opening a replacement dictation
    /// stream off-lock, before the stream is installed or handed to teardown.
    opening_recording: usize,
    /// Detached `audio-stream-close` threads currently closing a dictation stream.
    closing_recording: usize,
    /// Highest microphone test session id seen from any start or stop request.
    /// Older start requests are discarded so out-of-order threads cannot reopen
    /// a stopped or superseded test.
    latest_test_session: u64,
    /// Microphone test session currently inside `open_test_input` off-lock.
    opening_test: Option<u64>,
    /// The live microphone test stream, if any.
    active_test: Option<MicTest>,
    /// Detached `audio-stream-close` threads currently closing a test stream.
    closing_test: usize,
}

struct MicTest {
    id: u64,
    stream: SafeStream,
    clock: Arc<InputClock>,
    /// Cleared before teardown so callbacks stop emitting levels or errors.
    current: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
enum StreamKind {
    Recording,
    MicTest,
}

/// Accounts for one stream currently being torn down on a detached thread.
/// Created while holding `CAPTURE`; dropping it decrements the matching count
/// and wakes any caller waiting for exclusive capture access.
struct CloseToken(StreamKind);

impl Drop for CloseToken {
    fn drop(&mut self) {
        if let Ok(mut state) = CAPTURE.lock() {
            match self.0 {
                StreamKind::Recording => {
                    state.closing_recording = state.closing_recording.saturating_sub(1);
                }
                StreamKind::MicTest => {
                    state.closing_test = state.closing_test.saturating_sub(1);
                }
            }
            CAPTURE_CHANGED.notify_all();
        }
    }
}

fn begin_close(state: &mut CaptureState, kind: StreamKind) -> CloseToken {
    match kind {
        StreamKind::Recording => state.closing_recording += 1,
        StreamKind::MicTest => state.closing_test += 1,
    }
    CloseToken(kind)
}

/// Holds the dictation capture reservation during `start_recording_with_device`
/// and automatically releases it on any early error return.
struct RecordingReservation {
    active: bool,
}

impl RecordingReservation {
    fn commit(mut self) {
        self.active = false;
    }
}

impl Drop for RecordingReservation {
    fn drop(&mut self) {
        if self.active
            && let Ok(mut state) = CAPTURE.lock()
        {
            state.recording = false;
            CAPTURE_CHANGED.notify_all();
        }
    }
}

/// Tracks a lid-close replacement input open until the replacement stream is
/// installed into `RECORDING` or handed off to tracked teardown.
struct RecordingSwitchGuard;

impl Drop for RecordingSwitchGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = CAPTURE.lock() {
            state.opening_recording = state.opening_recording.saturating_sub(1);
            CAPTURE_CHANGED.notify_all();
        }
    }
}

/// How long starting dictation or a microphone test waits for an opposing
/// capture stream open or detached teardown to finish before rejecting instead
/// of opening an overlapping stream.
const CAPTURE_HANDOFF_WAIT: Duration = Duration::from_millis(500);

static CAPTURE: Mutex<CaptureState> = Mutex::new(CaptureState {
    recording: false,
    opening_recording: 0,
    closing_recording: 0,
    latest_test_session: 0,
    opening_test: None,
    active_test: None,
    closing_test: 0,
});
static CAPTURE_CHANGED: Condvar = Condvar::new();

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
    // Reserve capture exclusively for dictation, stop any running or opening
    // microphone test, and wait boundedly for test open/teardown to complete
    // so dictation never blocks indefinitely and never overlaps a test stream.
    let reservation = reserve_recording_capture(events)?;

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
    // drops `reservation` and leaves the recorder ready for a retry.
    {
        let mut active = RECORDING.lock().map_err(|err| err.to_string())?;
        *active = Some(Recording {
            sink,
            input,
            switching_input: false,
            save_path,
        });
    }
    reservation.commit();

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
    if let Err(err) = stream.play() {
        close_stream(SafeStream(stream), StreamKind::Recording);
        return Err(err.to_string());
    }

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
    // Register the in-flight replacement open before checking `RECORDING` so
    // stopping dictation mid-switch cannot let a microphone test open before
    // this replacement stream is either installed or handed off to teardown.
    let _switch_guard = match CAPTURE.lock() {
        Ok(mut state) => {
            state.opening_recording += 1;
            RecordingSwitchGuard
        }
        Err(_) => return,
    };

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
        if let Ok((input, _, _)) = input {
            close_stream(input.stream, StreamKind::Recording);
        }
        return;
    };
    let Some(recording) = active.as_mut().filter(|recording| recording.sink.id == id) else {
        // The recording ended meanwhile.
        drop(active);
        if let Ok((input, _, _)) = input {
            close_stream(input.stream, StreamKind::Recording);
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
    close_stream(previous.stream, StreamKind::Recording);

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
    finish_recording_stream(recording.input.stream);

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
    finish_recording_stream(recording.input.stream);
    events.emit(AppEvent::RecordingError { message });
}

/// Atomically marks the dictation stream as tearing down and releases the
/// dictation capture reservation before spawning detached teardown.
fn finish_recording_stream(stream: SafeStream) {
    let token = CAPTURE.lock().ok().map(|mut state| {
        let token = begin_close(&mut state, StreamKind::Recording);
        state.recording = false;
        CAPTURE_CHANGED.notify_all();
        token
    });
    spawn_stream_close(stream, token);
}

/// Pauses and drops a capture stream on a detached thread, tracking teardown in
/// `CAPTURE` so opposing capture operations wait or reject instead of
/// overlapping while CoreAudio still holds the device.
fn close_stream(stream: SafeStream, kind: StreamKind) {
    let token = CAPTURE
        .lock()
        .ok()
        .map(|mut state| begin_close(&mut state, kind));
    spawn_stream_close(stream, token);
}

fn spawn_stream_close(stream: SafeStream, token: Option<CloseToken>) {
    // Tuple fields drop in declaration order, so `stream` is always dropped
    // before `token` decrements the teardown counter — even if spawning fails.
    let closing = (stream, token);
    let spawned = std::thread::Builder::new()
        .name("audio-stream-close".into())
        .spawn(move || {
            let (stream, _token) = closing;
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

/// Resolves the microphone to test. An explicitly chosen device must exist
/// rather than silently testing a fallback; `"default"` / `None` follows the
/// same resolution as dictation (including clamshell replacement).
fn test_input_device(
    host: &cpal::Host,
    device_name: Option<&str>,
) -> Result<(cpal::Device, InputKind), String> {
    if let Some(name) = device_name.filter(|name| *name != "default" && !name.is_empty()) {
        let device = host
            .input_devices()
            .map_err(|err| err.to_string())?
            .find(|device| read_device_name(device).as_deref() == Some(name))
            .ok_or_else(|| format!("Selected microphone is unavailable: {name}"))?;
        let kind = microphones::kind(&device);
        return Ok((device, kind));
    }

    input_device(host, None)
}

/// Reserves capture for dictation, preempting any active or opening microphone
/// test and waiting up to [`CAPTURE_HANDOFF_WAIT`] for test capture to release
/// the audio device. Rejects instead of overlapping if a test is still stalled.
fn reserve_recording_capture(events: &EventSender) -> Result<RecordingReservation, String> {
    let (previous, stop_cutoff) = {
        let mut state = CAPTURE.lock().map_err(|err| err.to_string())?;
        if state.recording {
            return Err("Recording is already in progress.".to_string());
        }
        state.recording = true;

        let cutoff = next_microphone_test_session();
        let had_test = state.active_test.is_some() || state.opening_test.is_some();
        state.latest_test_session = state.latest_test_session.max(cutoff);
        let previous = state.active_test.take().map(|test| {
            let token = begin_close(&mut state, StreamKind::MicTest);
            (test, token)
        });
        CAPTURE_CHANGED.notify_all();
        (previous, had_test.then_some(cutoff))
    };

    let reservation = RecordingReservation { active: true };

    if let Some((test, token)) = previous {
        test.current.store(false, Ordering::Release);
        spawn_stream_close(test.stream, Some(token));
        log::info!(
            target: "audio",
            "microphone_test_stopped id={} reason=recording_started",
            test.id
        );
    }
    if let Some(cutoff) = stop_cutoff {
        events.emit(AppEvent::MicrophoneTestStopped {
            up_to_session: Some(cutoff),
        });
    }

    let deadline = Instant::now() + CAPTURE_HANDOFF_WAIT;
    let mut state = CAPTURE.lock().map_err(|err| err.to_string())?;
    while state.opening_test.is_some() || state.closing_test > 0 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(
                "Audio input device is still busy closing the microphone test.".to_string(),
            );
        }
        let (guard, result) = CAPTURE_CHANGED
            .wait_timeout(state, remaining)
            .map_err(|err| err.to_string())?;
        state = guard;
        if result.timed_out() && (state.opening_test.is_some() || state.closing_test > 0) {
            return Err(
                "Audio input device is still busy closing the microphone test.".to_string(),
            );
        }
    }

    Ok(reservation)
}

/// Opens a capture-only stream for testing input levels. Does not write WAV
/// files, trigger media mute/muffle, or play cues. Returns `Ok(true)` when
/// capture started, or `Ok(false)` if stopped or superseded before opening.
pub fn start_microphone_test(
    events: &EventSender,
    device_name: Option<String>,
    session_id: u64,
) -> Result<bool, String> {
    if permissions::microphone_permission() != PermissionState::Granted {
        return Err(MICROPHONE_PERMISSION_DENIED_ERROR.to_string());
    }

    // Stop any earlier microphone test and wake earlier queued starters.
    let previous = {
        let mut state = CAPTURE.lock().map_err(|err| err.to_string())?;
        if state.recording {
            return Err(MICROPHONE_TEST_RECORDING_IN_PROGRESS_ERROR.to_string());
        }
        if session_id <= state.latest_test_session {
            return Ok(false);
        }
        state.latest_test_session = session_id;
        let previous = state.active_test.take().map(|test| {
            let token = begin_close(&mut state, StreamKind::MicTest);
            (test, token)
        });
        CAPTURE_CHANGED.notify_all();
        previous
    };

    if let Some((prev, token)) = previous {
        prev.current.store(false, Ordering::Release);
        spawn_stream_close(prev.stream, Some(token));
        log::info!(
            target: "audio",
            "microphone_test_stopped id={} reason=replaced",
            prev.id
        );
    }

    // Wait boundedly for any in-flight open or detached teardown (from either
    // dictation or an earlier test) so streams never overlap on the device.
    let deadline = Instant::now() + CAPTURE_HANDOFF_WAIT;
    {
        let mut state = CAPTURE.lock().map_err(|err| err.to_string())?;
        loop {
            if state.recording {
                return Err(MICROPHONE_TEST_RECORDING_IN_PROGRESS_ERROR.to_string());
            }
            if state.latest_test_session != session_id {
                return Ok(false);
            }
            if state.opening_test.is_none()
                && state.closing_test == 0
                && state.opening_recording == 0
                && state.closing_recording == 0
            {
                state.opening_test = Some(session_id);
                break;
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(
                    "Audio input device is still releasing the previous stream; try again."
                        .to_string(),
                );
            }
            let (guard, result) = CAPTURE_CHANGED
                .wait_timeout(state, remaining)
                .map_err(|err| err.to_string())?;
            state = guard;
            if result.timed_out()
                && !state.recording
                && state.latest_test_session == session_id
                && (state.opening_test.is_some()
                    || state.closing_test > 0
                    || state.opening_recording > 0
                    || state.closing_recording > 0)
            {
                return Err(
                    "Audio input device is still releasing the previous stream; try again."
                        .to_string(),
                );
            }
        }
    }

    let opened = open_test_input(events, device_name.as_deref(), session_id);
    let (stream, clock, current, early_error, resolved_name, kind, config) = match opened {
        Ok(opened) => opened,
        Err(err) => {
            if let Ok(mut state) = CAPTURE.lock() {
                if state.opening_test == Some(session_id) {
                    state.opening_test = None;
                }
                CAPTURE_CHANGED.notify_all();
            }
            return Err(err);
        }
    };

    let (discard_token, early_err) = {
        let mut state = CAPTURE.lock().map_err(|err| err.to_string())?;
        if state.opening_test == Some(session_id) {
            state.opening_test = None;
        }

        if state.recording || state.latest_test_session != session_id {
            let token = begin_close(&mut state, StreamKind::MicTest);
            CAPTURE_CHANGED.notify_all();
            (Some(token), None)
        } else {
            let early_err = {
                let mut guard = early_error.lock().map_err(|err| err.to_string())?;
                if let Some(err) = guard.take() {
                    Some(err)
                } else {
                    current.store(true, Ordering::Release);
                    None
                }
            };
            if let Some(err) = early_err {
                let token = begin_close(&mut state, StreamKind::MicTest);
                CAPTURE_CHANGED.notify_all();
                (Some(token), Some(err))
            } else {
                state.active_test = Some(MicTest {
                    id: session_id,
                    stream,
                    clock,
                    current,
                });
                CAPTURE_CHANGED.notify_all();
                // Published: drop `state` and continue to logging/watchdog.
                drop(state);
                log::info!(
                    target: "audio",
                    "microphone_test_started id={session_id} device={resolved_name:?} kind={kind:?} sample_rate={} channels={}",
                    config.sample_rate,
                    config.channels
                );

                let watchdog_events = events.clone();
                let spawned = std::thread::Builder::new()
                    .name("audio-mic-test-watchdog".into())
                    .spawn(move || watch_mic_test(session_id, watchdog_events));
                if let Err(err) = spawned {
                    log::warn!(
                        target: "audio",
                        "microphone test watchdog not started id={session_id} error={err}"
                    );
                }

                return Ok(true);
            }
        }
    };

    current.store(false, Ordering::Release);
    spawn_stream_close(stream, discard_token);
    if let Some(err) = early_err {
        return Err(err);
    }
    Ok(false)
}

type OpenedTestInput = (
    SafeStream,
    Arc<InputClock>,
    Arc<AtomicBool>,
    Arc<Mutex<Option<String>>>,
    String,
    InputKind,
    cpal::StreamConfig,
);

fn open_test_input(
    events: &EventSender,
    device_name: Option<&str>,
    session_id: u64,
) -> Result<OpenedTestInput, String> {
    let host = cpal::default_host();
    let (device, kind) = test_input_device(&host, device_name)?;
    let supported_config = device
        .default_input_config()
        .map_err(|err| err.to_string())?;
    let sample_format = supported_config.sample_format();
    let config = supported_config.config();
    let resolved_name = read_device_name(&device).unwrap_or_default();

    let clock = Arc::new(InputClock {
        opened_ms: elapsed_ms(),
        last_input_ms: AtomicU64::new(NO_INPUT_YET),
    });
    let current = Arc::new(AtomicBool::new(false));
    let early_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let err_events = events.clone();
    let err_current = current.clone();
    let err_early = early_error.clone();
    let err_fn = move |err: StreamError| {
        if matches!(err, StreamError::BufferUnderrun) {
            log::warn!(target: "audio", "microphone test stream overrun id={session_id}");
            return;
        }

        let should_fail = if err_current.load(Ordering::Acquire) {
            true
        } else {
            let Ok(mut guard) = err_early.lock() else {
                return;
            };
            if err_current.load(Ordering::Acquire) {
                true
            } else {
                if guard.is_none() {
                    *guard = Some(err.to_string());
                }
                false
            }
        };
        if !should_fail {
            return;
        }

        // Errors can arrive on CoreAudio's realtime thread; tear down elsewhere.
        let events = err_events.clone();
        let message = err.to_string();
        std::thread::spawn(move || fail_mic_test(session_id, &events, message));
    };

    let stream = build_test_stream(
        &device,
        sample_format,
        &config,
        session_id,
        events.clone(),
        clock.clone(),
        current.clone(),
        err_fn,
    )?;
    if let Err(err) = stream.play() {
        // Route failed streams through detached teardown so a stalled driver
        // cannot block the caller while dropping `stream`.
        close_stream(SafeStream(stream), StreamKind::MicTest);
        return Err(err.to_string());
    }

    Ok((
        SafeStream(stream),
        clock,
        current,
        early_error,
        resolved_name,
        kind,
        config,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_test_stream<E>(
    device: &cpal::Device,
    sample_format: cpal::SampleFormat,
    config: &cpal::StreamConfig,
    session_id: u64,
    events: EventSender,
    clock: Arc<InputClock>,
    current: Arc<AtomicBool>,
    err_fn: E,
) -> Result<Stream, String>
where
    E: FnMut(StreamError) + Send + 'static,
{
    let mut last_emit_ms = elapsed_ms().saturating_sub(33);
    let mut peak_since_emit = 0.0f32;

    let mut report_level = move |level: f32| {
        clock.last_input_ms.store(elapsed_ms(), Ordering::Relaxed);
        if !current.load(Ordering::Acquire) {
            return;
        }

        peak_since_emit = peak_since_emit.max(level);
        let now = elapsed_ms();
        if now.saturating_sub(last_emit_ms) >= 33 {
            last_emit_ms = now;
            let emitted = std::mem::take(&mut peak_since_emit);
            events.emit(AppEvent::MicrophoneTestLevel {
                level: emitted,
                session_id: Some(session_id),
            });
        }
    };

    match sample_format {
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let level =
                    compute_audio_level(data.iter().map(|&s| f32::from(s) / 32768.0), data.len());
                report_level(level);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let level = compute_audio_level(
                    data.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0),
                    data.len(),
                );
                report_level(level);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream(
            config,
            move |data: &[i32], _: &cpal::InputCallbackInfo| {
                let level = compute_audio_level(
                    data.iter().map(|&s| s as f32 / i32::MAX as f32),
                    data.len(),
                );
                report_level(level);
            },
            err_fn,
            None,
        ),
        _ => device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let level = compute_audio_level(data.iter().copied(), data.len());
                report_level(level);
            },
            err_fn,
            None,
        ),
    }
    .map_err(|err| err.to_string())
}

pub fn stop_microphone_test(up_to_session: u64) -> Result<(), String> {
    let previous = {
        let mut state = CAPTURE.lock().map_err(|err| err.to_string())?;
        if up_to_session < state.latest_test_session {
            return Ok(());
        }
        state.latest_test_session = state.latest_test_session.max(up_to_session);
        // Keep `opening_test` intact until `open_test_input` returns so no
        // overlapping stream opens while the cancelled open is still in flight.
        let previous = if state
            .active_test
            .as_ref()
            .is_some_and(|test| test.id <= up_to_session)
        {
            state.active_test.take().map(|test| {
                let token = begin_close(&mut state, StreamKind::MicTest);
                (test, token)
            })
        } else {
            None
        };
        CAPTURE_CHANGED.notify_all();
        previous
    };

    if let Some((test, token)) = previous {
        test.current.store(false, Ordering::Release);
        spawn_stream_close(test.stream, Some(token));
        log::info!(target: "audio", "microphone_test_stopped id={}", test.id);
    }

    Ok(())
}

fn fail_mic_test(session_id: u64, events: &EventSender, message: String) {
    let previous = match CAPTURE.lock() {
        Ok(mut state)
            if state
                .active_test
                .as_ref()
                .is_some_and(|test| test.id == session_id) =>
        {
            let previous = state.active_test.take().map(|test| {
                let token = begin_close(&mut state, StreamKind::MicTest);
                (test, token)
            });
            CAPTURE_CHANGED.notify_all();
            previous
        }
        _ => None,
    };
    let Some((test, token)) = previous else {
        return;
    };

    log::error!(target: "audio", "microphone_test_error id={session_id}: {message}");
    test.current.store(false, Ordering::Release);
    spawn_stream_close(test.stream, Some(token));
    events.emit(AppEvent::MicrophoneTestError {
        message,
        session_id: Some(session_id),
    });
}

fn watch_mic_test(session_id: u64, events: EventSender) {
    loop {
        std::thread::sleep(INPUT_WATCHDOG_INTERVAL);

        let clock = match CAPTURE.lock() {
            Ok(state) => match state
                .active_test
                .as_ref()
                .filter(|test| test.id == session_id)
            {
                Some(test) => test.clock.clone(),
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
            fail_mic_test(
                session_id,
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

/// The `.wav` files in the recordings directory, with when each was last
/// written.
fn recordings() -> std::io::Result<Vec<(SystemTime, PathBuf)>> {
    Ok(std::fs::read_dir(recordings_dir())?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("wav") {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect())
}

/// Prunes the recordings directory down to the `keep` newest `.wav` files by
/// modification time. Called after each successful transcription so dictation
/// audio is not retained forever. IO failures are logged and otherwise
/// ignored — cleanup must never fail a transcription.
pub fn cleanup_old_recordings(keep: usize) {
    let mut recordings = match recordings() {
        Ok(recordings) => recordings,
        Err(err) => {
            log::warn!(target: "audio", "recordings cleanup failed to read dir: {err}");
            return;
        }
    };

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

/// The name `path` has in the recordings directory, if it is one of the
/// recordings.
pub fn recording_name(path: &Path) -> Option<String> {
    if path.parent()? != recordings_dir() {
        return None;
    }
    path.file_name()?.to_str().map(str::to_owned)
}

/// The newest recording last written between `from` and `to`.
pub fn recording_finished_between(from: SystemTime, to: SystemTime) -> Option<String> {
    recordings()
        .ok()?
        .into_iter()
        .filter(|(modified, _)| (from..=to).contains(modified))
        .max_by_key(|(modified, _)| *modified)
        .and_then(|(_, path)| recording_name(&path))
}

/// Deletes the recording called `name`, if it is still kept.
pub fn delete_recording(name: &str) -> Result<(), String> {
    // A bare file name only, so nothing outside the directory is reached.
    if Path::new(name).file_name() != Some(name.as_ref()) {
        return Err(format!("Not a recording: {name}"));
    }

    match std::fs::remove_file(recordings_dir().join(name)) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("Failed to delete recording {name}: {err}"))
        }
        _ => Ok(()),
    }
}

/// Recordings being transcribed. Clearing the history leaves them, and the
/// files made from them, to the transcription.
static TRANSCRIBING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Marks `path` as being transcribed until the guard is dropped.
pub struct Transcribing(PathBuf);

pub fn transcribing(path: &Path) -> Transcribing {
    if let Ok(mut transcribing) = TRANSCRIBING.lock() {
        transcribing.push(path.to_path_buf());
    }
    Transcribing(path.to_path_buf())
}

impl Drop for Transcribing {
    fn drop(&mut self) {
        if let Ok(mut transcribing) = TRANSCRIBING.lock()
            && let Some(index) = transcribing.iter().position(|path| *path == self.0)
        {
            transcribing.remove(index);
        }
    }
}

/// Whether `path` is `source` or a file made from it, such as the copy
/// Parakeet reads (`<stem>.parakeet16k.wav`).
fn made_from(path: &Path, source: &Path) -> bool {
    let (Some(name), Some(stem)) = (path.file_name(), source.file_stem()) else {
        return false;
    };
    path.parent() == source.parent()
        && name
            .to_string_lossy()
            .starts_with(&format!("{}.", stem.to_string_lossy()))
}

/// Deletes every recording kept on disk, except the one being made and those
/// being transcribed. Fails if any recording could not be listed or deleted.
pub fn delete_recordings() -> Result<(), String> {
    let entries = match std::fs::read_dir(recordings_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("Failed to list recordings: {err}")),
    };
    let mut keep: Vec<PathBuf> = RECORDING
        .lock()
        .ok()
        .and_then(|active| Some(active.as_ref()?.save_path.clone()))
        .into_iter()
        .collect();
    // Held while deleting, so a transcription cannot start on a file that
    // is about to go.
    let transcribing = TRANSCRIBING.lock().map_err(|err| err.to_string())?;
    keep.extend(transcribing.iter().cloned());

    let mut failed = 0;
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(err) => return Err(format!("Failed to list recordings: {err}")),
        };
        if path.extension().and_then(|ext| ext.to_str()) != Some("wav")
            || keep.iter().any(|source| made_from(&path, source))
        {
            continue;
        }
        if let Err(err) = std::fs::remove_file(&path) {
            log::warn!(
                target: "audio",
                "failed to delete recording {}: {err}",
                path.display()
            );
            failed += 1;
        }
    }

    if failed > 0 {
        return Err(format!("Failed to delete {failed} recordings"));
    }
    Ok(())
}

// Timestamp tracking for throttling - using a static Instant for reference
static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

fn elapsed_ms() -> u64 {
    START_TIME.elapsed().as_millis() as u64
}

fn compute_audio_level<I>(samples: I, count: usize) -> f32
where
    I: IntoIterator<Item = f32>,
{
    if count == 0 {
        return 0.0;
    }

    let mut sum: f64 = 0.0;
    let mut peak: f64 = 0.0;
    for sample in samples {
        let value = f64::from(sample);
        if !value.is_finite() {
            continue;
        }
        sum += value * value;
        peak = peak.max(value.abs());
    }

    let rms = (sum / count as f64).sqrt();
    // Use a combination of RMS and peak for responsive visualization (0..=100).
    ((rms * 0.7 + peak * 0.3) * 150.0).min(100.0) as f32
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

    for &sample in input.iter() {
        writer.write_sample(sample).ok();
    }

    let level = compute_audio_level(input.iter().copied(), input.len());

    // Throttle to ~30fps (every ~33ms)
    let now = elapsed_ms();
    if now.saturating_sub(*last_emit_ms) >= 33 {
        *last_emit_ms = now;
        events.emit(AppEvent::AudioLevel(level));
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
