//! Runs dictation: the hotkeys start and end a recording, the recording is
//! transcribed, and the text is typed into the app in front. The controller
//! belongs to the app, not to a window; the overlay only shows its state.

use gpui_kit::{
    App, AppContext as _, AsyncApp, Context, Entity, Result, Subscription, Task, WeakEntity,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use crate::backend::{self, AppEvent};
use crate::events;
use crate::settings::SettingsStore;
use crate::sounds::{self, Cue};
use crate::ui::t;
use crate::ui::widgets::animations_frozen;

use super::bars::{NUM_BARS, WAVE_STEP, bar_heights, next_wave_offset};
use super::hotkeys::{
    HotkeyEvent, HotkeyPlan, HotkeyRole, Hotkeys, dictation_hotkeys, hotkey_plan,
};
use super::state::{
    Command, Dictation, DictationError, HistoryRetention, Pill, RecordingMode, Retry, Target,
};

pub const WINDOW_WIDTH_COMPACT: f32 = 100.;
const WINDOW_WIDTH_HANDS_FREE: f32 = 140.;
const WINDOW_WIDTH_ERROR: f32 = 260.;

/// The key code of Escape, which has its own shortcut while recording.
const ESCAPE_KEY_CODE: i64 = 53;

/// How long a transcription error, with its retry button, stays if untouched.
const TRANSCRIPTION_ERROR_HIDE: Duration = Duration::from_millis(5000);
/// How long a recording or typing error stays.
const RECORDING_ERROR_HIDE: Duration = Duration::from_millis(4000);
/// How long after the start cue began media is quieted, so the cue is heard
/// at full volume.
const START_CUE_MEDIA_DELAY: Duration = Duration::from_millis(180);
/// How long after a dictation the history is told about its new entry.
const TRANSCRIPTION_CREATED_DELAY: Duration = Duration::from_millis(1000);

const RECORDING_FAILED: &str = "voiceControl.recordingFailed";
const TRANSCRIPTION_FAILED: &str = "voiceControl.transcriptionFailed";
const TYPING_FAILED: &str = "voiceControl.typingFailed";

/// The window the overlay needs for the current state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowRequest {
    pub visible: bool,
    pub width: f32,
}

/// How long media keeps its volume once a recording runs whose start cue
/// began at `cue_started_at`.
fn media_quiet_delay(cue_started_at: Option<Instant>, now: Instant) -> Duration {
    cue_started_at.map_or(Duration::ZERO, |started_at| {
        START_CUE_MEDIA_DELAY.saturating_sub(now.saturating_duration_since(started_at))
    })
}

/// Where an in-flight recording start is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartPhase {
    /// Looking up the frontmost app; the microphone has not been touched yet.
    Frontmost,
    /// Opening the microphone stream on the backend.
    OpeningMic,
    /// The start was cancelled while opening the microphone; waiting for the
    /// backend call to return so the stream can be closed cleanly.
    CancellingMic,
}

/// Ownership and phase of an in-flight recording start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveStart {
    id: u64,
    mode: RecordingMode,
    phase: StartPhase,
}

/// Recordings on disk that have not been saved to history or discarded yet.
/// Shared with worker threads and app shutdown so a quit during stop or
/// transcription never leaves a temporary recording behind.
#[derive(Default)]
struct PendingRecordings {
    shutting_down: bool,
    paths: Vec<PathBuf>,
}

impl PendingRecordings {
    /// Tracks `path` unless shutdown has started (in which case the caller
    /// must discard it immediately).
    fn track(&mut self, path: PathBuf) -> bool {
        if self.shutting_down {
            return false;
        }
        if !self.paths.contains(&path) {
            self.paths.push(path);
        }
        true
    }

    fn untrack(&mut self, path: &Path) {
        self.paths.retain(|tracked| tracked != path);
    }
}

pub struct VoiceController {
    dictation: Dictation,
    /// The in-flight recording start, if any. Async completions check `id`
    /// before mutating controller state so stale starts never interfere with a
    /// newer one.
    active_start: Option<ActiveStart>,
    /// The id of the currently running recording, used to reject late errors
    /// emitted by an earlier recording stream.
    active_recording_id: Option<u64>,
    next_start_id: u64,
    audio_level: f32,
    wave_offset: usize,
    /// Where the running recording will be typed.
    target: Target,
    /// Whether saving transcription history has stayed enabled since the
    /// running recording started. Once turned off, stays false until the next
    /// recording starts.
    recording_save_history: bool,
    /// Per-transcription opt-in locks for in-flight transcriptions. Turning
    /// history off locks and clears every live flag so opt-out cannot overlap
    /// with the final storage commit, and even a brief opt-out discards it.
    active_transcriptions: Vec<Weak<Mutex<bool>>>,
    /// Serializes recording start/stop/cancel transitions with app shutdown.
    lifecycle: Arc<Mutex<()>>,
    /// Unsaved recordings owned by active dictations, cleaned up on shutdown.
    pending: Arc<Mutex<PendingRecordings>>,
    /// The onboarding step that lets dictation be tried is showing.
    learning_active: bool,
    onboarding_complete: bool,
    window: WindowRequest,
    hotkeys: Entity<Hotkeys>,
    error_hide_timer: Option<Task<()>>,
    media_quiet_timer: Option<Task<()>>,
    wave_timer: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl VoiceController {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // A stored hotkey that cannot be registered is replaced for good.
        let (toggle, hold) = dictation_hotkeys(SettingsStore::get(cx));
        SettingsStore::update(cx, {
            let (toggle, hold) = (toggle.clone(), hold.clone());
            move |settings| {
                settings.hotkey = toggle;
                settings.hold_to_speak_hotkey = hold;
            }
        });

        let onboarding_complete = SettingsStore::get(cx).onboarding_complete;
        let hotkeys = cx.new(Hotkeys::new);
        if onboarding_complete {
            hotkeys.update(cx, |hotkeys, cx| {
                hotkeys.register_dictation(toggle, hold, true, cx)
            });
        }

        let subscriptions = vec![
            cx.subscribe(&events::hub(cx), |this, _, event, cx| {
                this.handle_event(event, cx);
            }),
            cx.subscribe(&hotkeys, |this, _, event, cx| {
                this.handle_hotkey(*event, cx);
            }),
            cx.observe(&SettingsStore::entity(cx), |this, _, cx| {
                this.settings_changed(cx);
            }),
        ];

        Self {
            dictation: Dictation::default(),
            active_start: None,
            active_recording_id: None,
            next_start_id: 0,
            audio_level: 0.,
            wave_offset: 0,
            target: Target::default(),
            recording_save_history: true,
            active_transcriptions: Vec::new(),
            lifecycle: Arc::new(Mutex::new(())),
            pending: Arc::new(Mutex::new(PendingRecordings::default())),
            learning_active: false,
            onboarding_complete,
            window: WindowRequest {
                visible: false,
                width: WINDOW_WIDTH_COMPACT,
            },
            hotkeys,
            error_hide_timer: None,
            media_quiet_timer: None,
            wave_timer: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn pill(&self) -> Pill {
        self.dictation.pill()
    }

    pub fn error(&self) -> Option<&DictationError> {
        self.dictation.error.as_ref()
    }

    pub fn bar_heights(&self) -> [f32; NUM_BARS] {
        bar_heights(self.wave_offset, self.audio_level, self.dictation.loading)
    }

    pub fn window_request(&self) -> WindowRequest {
        self.window
    }

    /// Holds the wave at `wave_offset`; with animations frozen it never moves.
    #[cfg(feature = "fixture")]
    pub fn set_wave_offset(&mut self, wave_offset: usize) {
        self.wave_offset = wave_offset;
    }

    /// Leaves nothing behind when the app exits: media gets its volume back
    /// and any running, stopping, or unsaved recording is dropped.
    pub fn shut_down(&mut self, cx: &App) {
        let backend = backend::backend(cx);

        if let Err(error) = backend.restore_media_after_recording() {
            log::error!(target: "voice", "restore_media_on_exit_failed error={error}");
        }
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
        let paths_to_discard = {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            pending.shutting_down = true;
            std::mem::take(&mut pending.paths)
        };
        // Block new recordings and Parakeet scratch WAVs before deleting any
        // files so a worker thread cannot create a scratch WAV afterward.
        backend.begin_shutdown();

        if self.dictation.recording.is_some() || self.dictation.starting || self.dictation.stopping
        {
            if let Ok(audio_path) = backend.stop_recording() {
                let _ = backend.discard_recording(&audio_path);
            }
        }

        for path in paths_to_discard {
            let _ = backend.discard_recording(&path);
        }
    }

    fn handle_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::AudioLevel(level) => {
                self.audio_level = *level;
                cx.notify();
            }
            AppEvent::LearningStepActive(active) => {
                self.learning_active = *active;
                self.plan_hotkeys(*active || self.onboarding_complete, false, cx);
                self.cancel_unstoppable_recording(cx);
            }
            AppEvent::KeyboardKeyPressed { key_code } => {
                if *key_code == ESCAPE_KEY_CODE || !self.hotkeys_live() {
                    return;
                }
                if let Some(command) = self.dictation.other_key_pressed() {
                    self.run(command, cx);
                }
            }
            // The default output may have moved; the next cue must not play
            // into a device that is gone.
            AppEvent::AudioDevicesChanged => sounds::reset_output(),
            AppEvent::RecordingError {
                message,
                recording_id,
            } => self.recording_failed(message, *recording_id, cx),
            _ => {}
        }
    }

    fn handle_hotkey(&mut self, event: HotkeyEvent, cx: &mut Context<Self>) {
        let command = match event {
            HotkeyEvent::Escape => Some(self.dictation.escape_pressed()),
            _ if !self.hotkeys_live() => None,
            HotkeyEvent::Pressed(HotkeyRole::Toggle) => self.dictation.toggle_pressed(),
            HotkeyEvent::Pressed(HotkeyRole::Hold) if Self::hold_enabled(cx) => {
                self.dictation.hold_pressed()
            }
            HotkeyEvent::Pressed(HotkeyRole::Hold) => None,
            HotkeyEvent::Released(HotkeyRole::Hold) => self.dictation.hold_released(),
            HotkeyEvent::Released(HotkeyRole::Toggle) => None,
        };

        if let Some(command) = command {
            self.run(command, cx);
        }
    }

    fn run(&mut self, command: Command, cx: &mut Context<Self>) {
        match command {
            Command::Start(mode) => self.start_recording(mode, cx),
            Command::Stop => self.stop_recording(cx),
            Command::Cancel => self.cancel_recording(cx),
            Command::DismissError => self.dismiss_error(cx),
        }
    }

    /// Dictation can be started once onboarding is done, and while its step
    /// for trying dictation is showing.
    fn hotkeys_live(&self) -> bool {
        self.onboarding_complete || self.learning_active
    }

    fn hold_enabled(cx: &App) -> bool {
        dictation_hotkeys(SettingsStore::get(cx)).1.is_some()
    }

    fn mode_allowed(&self, mode: RecordingMode, cx: &App) -> bool {
        self.hotkeys_live() && (mode != RecordingMode::Hold || Self::hold_enabled(cx))
    }

    fn settings_changed(&mut self, cx: &mut Context<Self>) {
        let settings = SettingsStore::get(cx);
        let now_complete = settings.onboarding_complete;
        let save_history = settings.save_transcription_history;
        let finished_onboarding = !self.onboarding_complete && now_complete;
        self.onboarding_complete = now_complete;

        if !save_history {
            self.recording_save_history = false;
            self.active_transcriptions.retain(|flag| {
                let Some(flag) = flag.upgrade() else {
                    return false;
                };
                *flag.lock().unwrap_or_else(|e| e.into_inner()) = false;
                true
            });
            if let Some(retry) = self
                .dictation
                .error
                .as_mut()
                .and_then(|error| error.retry.take())
            {
                self.discard_pending_async(retry.audio_path, cx);
                cx.notify();
            }
        }

        self.plan_hotkeys(
            now_complete || self.learning_active,
            finished_onboarding,
            cx,
        );
        self.cancel_unstoppable_recording(cx);
    }

    /// Brings the registered hotkeys in line with the settings.
    fn plan_hotkeys(&mut self, live: bool, force: bool, cx: &mut Context<Self>) {
        let (toggle, hold) = dictation_hotkeys(SettingsStore::get(cx));
        let plan = hotkey_plan(
            live,
            &toggle,
            hold.as_deref(),
            self.hotkeys.read(cx).registered(),
            force,
        );

        self.hotkeys.update(cx, |hotkeys, cx| match plan {
            HotkeyPlan::Register { toggle, hold } => {
                hotkeys.register_dictation(toggle, hold, force, cx)
            }
            HotkeyPlan::Unregister => hotkeys.unregister_dictation(cx),
            HotkeyPlan::Keep => {}
        });
    }

    fn set_recording(&mut self, mode: Option<RecordingMode>, cx: &mut Context<Self>) {
        self.dictation.recording = mode;
        cx.notify();

        if mode.is_none() {
            self.wave_timer = None;
        } else if self.wave_timer.is_none() && !animations_frozen() {
            self.wave_timer = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(WAVE_STEP).await;

                    let moved = this.update(cx, |this, cx| {
                        this.wave_offset = next_wave_offset(this.wave_offset);
                        cx.notify();
                    });
                    if moved.is_err() {
                        break;
                    }
                }
            }));
        }
    }

    fn set_escape(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.hotkeys
            .update(cx, |hotkeys, cx| hotkeys.set_escape(enabled, cx));
    }

    /// Deletes `audio_path` in the background and untracks it from `pending`
    /// only after deletion succeeds, so a failed deletion or an immediate quit
    /// still cleans it up during shutdown.
    fn discard_pending_async(&self, audio_path: PathBuf, cx: &App) {
        let lifecycle = self.lifecycle.clone();
        let pending = self.pending.clone();
        backend::call(cx, move |backend| {
            let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
            match backend.discard_recording(&audio_path) {
                Ok(()) => {
                    pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .untrack(&audio_path);
                }
                Err(error) => {
                    log::warn!(target: "voice", "discard_recording_failed error={error}");
                }
            }
        })
        .detach();
    }

    fn clear_error_state(&mut self, cx: &App) {
        self.error_hide_timer = None;
        if let Some(retry) = self.dictation.error.take().and_then(|error| error.retry) {
            if !retry.retention.allowed() || !SettingsStore::get(cx).save_transcription_history {
                self.discard_pending_async(retry.audio_path, cx);
            } else {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .untrack(&retry.audio_path);
            }
        }
    }

    fn cancel_pending_media_quiet(&mut self) {
        self.media_quiet_timer = None;
    }

    /// Quiets other apps' audio `delay` from now, if the settings ask for it
    /// and the recording still runs then.
    fn schedule_media_quiet(&mut self, delay: Duration, cx: &mut Context<Self>) {
        self.cancel_pending_media_quiet();

        let settings = SettingsStore::get(cx);
        if !settings.mute_media_while_recording && !settings.muffle_media_while_recording {
            return;
        }
        if delay.is_zero() {
            Self::quiet_media(cx);
            return;
        }

        self.media_quiet_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;

            this.update(cx, |this, cx| {
                if this.dictation.recording.is_some() {
                    Self::quiet_media(cx);
                }
            })
            .ok();
        }));
    }

    /// Nothing waits for this: the recording already runs, and muffling
    /// fades on a thread of the backend.
    fn quiet_media(cx: &App) {
        let settings = SettingsStore::get(cx);

        if settings.mute_media_while_recording {
            backend::call(cx, |backend| backend.mute_media_for_recording()).detach();
        } else if settings.muffle_media_while_recording {
            backend::call(cx, |backend| backend.muffle_media_for_recording()).detach();
        }
    }

    fn restore_media(cx: &App) -> Task<()> {
        let restore = backend::call(cx, |backend| backend.restore_media_after_recording());

        cx.background_spawn(async move {
            if let Err(error) = restore.await {
                log::error!(target: "voice", "restore_media_failed error={error}");
            }
        })
    }

    fn start_recording(&mut self, mode: RecordingMode, cx: &mut Context<Self>) {
        if self.dictation.recording.is_some() || self.dictation.stopping || self.dictation.starting
        {
            return;
        }

        self.dictation.starting = true;
        self.next_start_id += 1;
        let start_id = self.next_start_id;
        self.active_start = Some(ActiveStart {
            id: start_id,
            mode,
            phase: StartPhase::Frontmost,
        });
        self.recording_save_history = SettingsStore::get(cx).save_transcription_history;
        self.clear_error_state(cx);
        self.cancel_pending_media_quiet();
        cx.notify();
        log::info!(target: "voice", "start_recording_started mode={}", mode.name());

        // The cue plays before the microphone is set up: quieting media for
        // the recording would cut off a cue started any later.
        let cue_started_at = SettingsStore::get(cx).recording_sounds_enabled.then(|| {
            sounds::play(Cue::Scan);
            Instant::now()
        });

        self.target = Target::default();
        let lifecycle = self.lifecycle.clone();
        let pending = self.pending.clone();
        let frontmost = backend::call(cx, |backend| backend.get_frontmost_app());

        cx.spawn(async move |this, cx| {
            let frontmost = frontmost.await;

            let Some((start, has_selected_microphone)) = this.update(cx, |this, cx| {
                let active = this.active_start.filter(|start| start.id == start_id)?;
                if !this.dictation.starting
                    || active.phase != StartPhase::Frontmost
                    || !this.mode_allowed(mode, cx)
                {
                    this.dictation.starting = false;
                    this.active_start = None;
                    this.target = Target::default();
                    cx.notify();
                    return None;
                }

                this.active_start = Some(ActiveStart {
                    phase: StartPhase::OpeningMic,
                    ..active
                });
                if let Ok(frontmost) = frontmost {
                    this.target = Target::of(frontmost);
                }
                this.window = WindowRequest {
                    visible: true,
                    width: match mode {
                        RecordingMode::Toggle => WINDOW_WIDTH_HANDS_FREE,
                        RecordingMode::Hold => WINDOW_WIDTH_COMPACT,
                    },
                };
                cx.notify();

                let device = SettingsStore::get(cx)
                    .selected_microphone_id
                    .clone()
                    .filter(|device| device != "default");
                let has_selected_microphone = device.is_some();
                let lifecycle = lifecycle.clone();
                let pending = pending.clone();

                Some((
                    backend::call(cx, move |backend| {
                        let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
                        if pending
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .shutting_down
                        {
                            return Err("shutting down".to_string());
                        }
                        backend.start_recording(device, start_id)
                    }),
                    has_selected_microphone,
                ))
            })?
            else {
                return Ok(());
            };

            if let Err(error) = start.await {
                if pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .shutting_down
                {
                    return Ok(());
                }
                let show_error = this.update(cx, |this, cx| {
                    let Some(active) = this.active_start.filter(|start| start.id == start_id)
                    else {
                        return false;
                    };
                    this.dictation.starting = false;
                    this.active_start = None;
                    if active.phase == StartPhase::CancellingMic {
                        this.target = Target::default();
                        if this.dictation.recording.is_none()
                            && !this.dictation.loading
                            && this.dictation.error.is_none()
                        {
                            this.window.visible = false;
                        }
                        cx.notify();
                        return false;
                    }
                    true
                })?;
                if !show_error {
                    return Ok(());
                }
                log::error!(
                    target: "voice",
                    "start_recording_failed error={error} has_selected_microphone={has_selected_microphone}"
                );
                return Self::show_error(&this, cx, RECORDING_FAILED, RECORDING_ERROR_HIDE, None)
                    .await;
            }

            let orphaned = this.update(cx, |this, cx| {
                // Only mutate state if this request still owns the start.
                let Some(active) = this.active_start.filter(|start| start.id == start_id) else {
                    return false;
                };
                this.dictation.starting = false;
                this.active_start = None;

                if active.phase == StartPhase::CancellingMic || !this.mode_allowed(mode, cx) {
                    // The start was cancelled while the mic was opening: keep
                    // `stopping` set while tearing down the stream so a new
                    // recording cannot collide with it.
                    this.dictation.stopping = true;
                    this.target = Target::default();
                    this.window.width = WINDOW_WIDTH_COMPACT;
                    cx.notify();
                    return true;
                }

                this.active_recording_id = Some(start_id);
                this.set_recording(Some(mode), cx);
                this.schedule_media_quiet(media_quiet_delay(cue_started_at, Instant::now()), cx);
                this.set_escape(true, cx);
                log::info!(
                    target: "voice",
                    "start_recording_completed mode={} has_app_context={} has_site_context={} has_selected_microphone={has_selected_microphone}",
                    mode.name(),
                    this.target.bundle_id.is_some(),
                    this.target.domain.is_some(),
                );
                false
            });

            match orphaned {
                Ok(false) => {}
                Ok(true) => {
                    let cleanup = cx.update(|cx| {
                        let lifecycle = lifecycle.clone();
                        let pending = pending.clone();
                        backend::call(cx, move |backend| {
                            let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
                            if let Ok(audio_path) = backend.stop_recording() {
                                let tracked = pending
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .track(audio_path.clone());
                                match backend.discard_recording(&audio_path) {
                                    Ok(()) if tracked => {
                                        pending
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner())
                                            .untrack(&audio_path);
                                    }
                                    Ok(()) => {}
                                    Err(error) => {
                                        log::warn!(
                                            target: "voice",
                                            "discard_recording_failed error={error}"
                                        );
                                    }
                                }
                            }
                        })
                    });
                    cleanup.await;
                    this.update(cx, |this, cx| {
                        this.dictation.stopping = false;
                        if this.dictation.recording.is_none()
                            && !this.dictation.loading
                            && this.dictation.error.is_none()
                        {
                            this.window.visible = false;
                        }
                        cx.notify();
                    })?;
                }
                Err(_) => {
                    let cleanup = cx.update(|cx| {
                        backend::call(cx, move |backend| {
                            let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
                            if let Ok(audio_path) = backend.stop_recording() {
                                let tracked = pending
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .track(audio_path.clone());
                                match backend.discard_recording(&audio_path) {
                                    Ok(()) if tracked => {
                                        pending
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner())
                                            .untrack(&audio_path);
                                    }
                                    Ok(()) => {}
                                    Err(error) => {
                                        log::warn!(
                                            target: "voice",
                                            "discard_recording_failed error={error}"
                                        );
                                    }
                                }
                            }
                        })
                    });
                    cleanup.await;
                }
            }
            Ok(())
        })
        .detach();
    }

    /// Cancels a recording (or invalidates an in-flight start) whose hotkey
    /// went away (the onboarding trial step closed, or the optional
    /// hold-to-speak shortcut was disabled): its hotkey could no longer stop it.
    fn cancel_unstoppable_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(active) = self.active_start
            && !self.mode_allowed(active.mode, cx)
        {
            match active.phase {
                StartPhase::Frontmost => {
                    // Before the microphone is opened, release `starting` at once.
                    self.active_start = None;
                    self.dictation.starting = false;
                    self.target = Target::default();
                }
                StartPhase::OpeningMic => {
                    // Once `backend.start_recording` is in flight, keep `starting`
                    // set until it returns and closes the stream so a new start
                    // cannot collide.
                    self.active_start = Some(ActiveStart {
                        phase: StartPhase::CancellingMic,
                        ..active
                    });
                    self.target = Target::default();
                }
                StartPhase::CancellingMic => {}
            }
            if self.dictation.recording.is_none()
                && !self.dictation.loading
                && self.dictation.error.is_none()
            {
                self.window.visible = false;
            }
            cx.notify();
        }

        if let Some(mode) = self.dictation.recording
            && !self.mode_allowed(mode, cx)
        {
            self.cancel_recording(cx);
        }
    }

    pub fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if self.dictation.stopping {
            return;
        }
        let Some(mode) = self.dictation.recording else {
            return;
        };

        log::info!(target: "voice", "stop_recording_started mode={}", mode.name());
        self.cancel_pending_media_quiet();
        self.dictation.stopping = true;
        self.active_recording_id = None;
        self.set_recording(None, cx);
        self.set_escape(false, cx);
        self.window.width = WINDOW_WIDTH_COMPACT;

        // Media is restored before the recorder is touched, so that a failed
        // stop cannot leave the output muted or muffled.
        let restore = Self::restore_media(cx);
        let lifecycle = self.lifecycle.clone();
        let pending = self.pending.clone();

        cx.spawn(async move |this, cx| {
            restore.await;

            let stop = cx.update(|cx| {
                let lifecycle = lifecycle.clone();
                let pending = pending.clone();
                backend::call(cx, move |backend| {
                    let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
                    let result = backend.stop_recording();
                    if let Ok(path) = &result {
                        let mut pending = pending.lock().unwrap_or_else(|e| e.into_inner());
                        if !pending.track(path.clone()) {
                            drop(pending);
                            let _ = backend.discard_recording(path);
                            return Err("shutting down".to_string());
                        }
                    }
                    result
                })
            });
            match stop.await {
                Ok(audio_path) => {
                    let Ok((target, retention)) = this.update(cx, |this, cx| {
                        this.dictation.loading = true;
                        cx.notify();
                        let save_history = this.recording_save_history
                            && SettingsStore::get(cx).save_transcription_history;
                        let retention = this.register_transcription(save_history);
                        (std::mem::take(&mut this.target), retention)
                    }) else {
                        Self::discard_pending(cx, audio_path, &lifecycle, &pending).await;
                        return Ok(());
                    };
                    Self::transcribe_and_type(
                        &this, cx, audio_path, target, retention, &lifecycle, &pending,
                    )
                    .await?;
                }
                Err(error) => {
                    if !pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .shutting_down
                    {
                        log::error!(target: "voice", "stop_recording_failed error={error}");
                        Self::show_error(&this, cx, RECORDING_FAILED, RECORDING_ERROR_HIDE, None)
                            .await?;
                    }
                }
            }

            cx.update(|cx| Self::restore_media(cx)).await;
            this.update(cx, |this, cx| {
                this.dictation.loading = false;
                this.dictation.stopping = false;
                if this.dictation.error.is_none() {
                    this.window.visible = false;
                }
                cx.notify();
                log::info!(target: "voice", "stop_recording_completed");
            })
        })
        .detach();
    }

    pub fn cancel_recording(&mut self, cx: &mut Context<Self>) {
        if self.dictation.stopping {
            return;
        }
        let Some(mode) = self.dictation.recording else {
            return;
        };

        log::warn!(target: "voice", "recording_cancelled mode={}", mode.name());
        self.cancel_pending_media_quiet();
        self.dictation.stopping = true;
        self.active_recording_id = None;
        self.set_recording(None, cx);
        self.target = Target::default();
        self.set_escape(false, cx);
        self.window.width = WINDOW_WIDTH_COMPACT;

        let restore = Self::restore_media(cx);
        let lifecycle = self.lifecycle.clone();
        let pending = self.pending.clone();

        cx.spawn(async move |this, cx| {
            restore.await;

            let stop_and_discard = cx.update(|cx| {
                backend::call(cx, move |backend| {
                    let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
                    if let Ok(audio_path) = backend.stop_recording() {
                        let tracked = pending
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .track(audio_path.clone());
                        match backend.discard_recording(&audio_path) {
                            Ok(()) if tracked => {
                                pending
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .untrack(&audio_path);
                            }
                            Ok(()) => {}
                            Err(error) => {
                                log::warn!(
                                    target: "voice",
                                    "discard_recording_failed error={error}"
                                );
                            }
                        }
                    }
                })
            });
            stop_and_discard.await;

            this.update(cx, |this, cx| {
                this.dictation.stopping = false;
                this.window.visible = false;
                cx.notify();
            })
        })
        .detach();
    }

    /// The microphone stream died. The recorder has reset itself already;
    /// this resets the state here, so the next press of a hotkey starts
    /// afresh, and reports the failure.
    fn recording_failed(
        &mut self,
        message: &str,
        recording_id: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        log::error!(
            target: "voice",
            "recording_error_event recording_id={recording_id:?} message={message}"
        );
        if let Some(id) = recording_id {
            let matches_recording = self.active_recording_id == Some(id);
            let matches_live_start = self
                .active_start
                .is_some_and(|start| start.id == id && start.phase == StartPhase::OpeningMic);
            if !matches_recording && !matches_live_start {
                return;
            }
        } else {
            if self
                .active_start
                .is_some_and(|start| start.phase != StartPhase::OpeningMic)
                && self.dictation.recording.is_none()
            {
                return;
            }
            if self.dictation.recording.is_none() && !self.dictation.starting {
                return;
            }
        }

        self.active_recording_id = None;
        self.set_recording(None, cx);
        self.target = Target::default();
        self.dictation.starting = false;
        self.active_start = None;
        self.dictation.stopping = false;
        self.dictation.loading = false;

        let restore = Self::restore_media(cx);

        cx.spawn(async move |this, cx| {
            restore.await;
            Self::show_error(&this, cx, RECORDING_FAILED, RECORDING_ERROR_HIDE, None).await
        })
        .detach();
    }

    /// Creates and tracks a retention flag for an in-flight dictation (and any
    /// retry prompt it produces) so an opt-out at any point permanently marks
    /// it for discard.
    fn register_transcription(&mut self, initial: bool) -> HistoryRetention {
        let retention = HistoryRetention::new(initial);
        self.active_transcriptions
            .retain(|flag| flag.strong_count() > 0);
        self.active_transcriptions.push(retention.downgrade());
        retention
    }

    /// Discards `audio_path` and untracks it from `pending` only if deletion
    /// succeeds, so shutdown retries cleanup if deletion failed.
    async fn discard_pending(
        cx: &mut AsyncApp,
        audio_path: PathBuf,
        lifecycle: &Arc<Mutex<()>>,
        pending: &Arc<Mutex<PendingRecordings>>,
    ) {
        let discard = cx.update(|cx| {
            let lifecycle = lifecycle.clone();
            let pending = pending.clone();
            backend::call(cx, move |backend| {
                let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
                match backend.discard_recording(&audio_path) {
                    Ok(()) => {
                        pending
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .untrack(&audio_path);
                    }
                    Err(error) => {
                        log::warn!(target: "voice", "discard_recording_failed error={error}");
                    }
                }
            })
        });
        discard.await;
    }

    /// Transcribes the recording at `audio_path`, saves the result to the
    /// history while `retention` remains allowed, and types it.
    async fn transcribe_and_type(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        audio_path: PathBuf,
        target: Target,
        retention: HistoryRetention,
        lifecycle: &Arc<Mutex<()>>,
        pending: &Arc<Mutex<PendingRecordings>>,
    ) -> Result<()> {
        let retain_recording = retention.allowed();
        let style = cx.update(|cx| SettingsStore::get(cx).default_style.clone());
        let transcription = cx.update(|cx| {
            let (audio_path, target, style) = (audio_path.clone(), target.clone(), style.clone());
            let pending = pending.clone();
            backend::call(cx, move |backend| {
                let result = backend.transcribe_audio(
                    audio_path.clone(),
                    target.bundle_id,
                    target.domain,
                    style,
                    retain_recording,
                );
                if !retain_recording && backend.discard_recording(&audio_path).is_ok() {
                    pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .untrack(&audio_path);
                }
                result
            })
        });

        let result = match transcription.await {
            Ok(result) => result,
            Err(error) => {
                log::error!(target: "voice", "transcription_failed error={error}");
                let keep_for_retry = retention.allowed()
                    && cx.update(|cx| SettingsStore::get(cx).save_transcription_history);
                let retry = if keep_for_retry {
                    Some(Retry {
                        audio_path,
                        target,
                        retention,
                    })
                } else {
                    if retain_recording {
                        Self::discard_pending(cx, audio_path, lifecycle, pending).await;
                    }
                    None
                };
                return Self::show_error(
                    this,
                    cx,
                    TRANSCRIPTION_FAILED,
                    TRANSCRIPTION_ERROR_HIDE,
                    retry,
                )
                .await;
            }
        };

        let save_history = retention.allowed()
            && cx.update(|cx| SettingsStore::get(cx).save_transcription_history);
        if save_history {
            let saved = cx.update(|cx| {
                let result = result.clone();
                let audio_path = audio_path.clone();
                let retention = retention.clone();
                let lifecycle = lifecycle.clone();
                let pending = pending.clone();
                backend::call(cx, move |backend| -> backend::CommandResult<()> {
                    let _lifecycle = lifecycle.lock().unwrap_or_else(|e| e.into_inner());
                    if pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .shutting_down
                    {
                        return Ok(());
                    }
                    if !retention.allowed() {
                        match backend.discard_recording(&audio_path) {
                            Ok(()) => {
                                pending
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .untrack(&audio_path);
                            }
                            Err(error) => {
                                log::warn!(
                                    target: "voice",
                                    "discard_recording_failed error={error}"
                                );
                            }
                        }
                        return Ok(());
                    }
                    backend.save_transcription(&result, &audio_path, retention.lock_flag())?;
                    pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .untrack(&audio_path);
                    Ok(())
                })
            });
            if let Err(error) = saved.await {
                log::error!(target: "voice", "save_transcription_failed error={error}");
            }
        } else if retain_recording {
            Self::discard_pending(cx, audio_path, lifecycle, pending).await;
        }
        log::info!(
            target: "voice",
            "transcription_completed word_count={} processing_time_ms={} audio_duration_ms={:?} has_app_context={} has_site_context={} style={style}",
            result.word_count,
            result.processing_time_ms,
            result.audio_duration_ms,
            target.bundle_id.is_some(),
            target.domain.is_some(),
        );

        let character_count = result.text.encode_utf16().count();
        let typed =
            cx.update(|cx| backend::call(cx, move |backend| backend.type_text(&result.text)));

        match typed.await {
            Ok(()) => {
                if cx.update(|cx| SettingsStore::get(cx).recording_sounds_enabled) {
                    sounds::play(Cue::Bloom);
                }
                log::info!(target: "voice", "text_typed character_count={character_count}");
            }
            Err(error) => {
                log::error!(target: "voice", "type_text_failed error={error}");

                let accessible =
                    cx.update(|cx| backend::call(cx, |backend| backend.check_accessibility()));
                if !accessible.await {
                    // The permission was revoked after onboarding. The main
                    // window opens the settings for it.
                    cx.update(|cx| events::emit(cx, AppEvent::AccessibilityPermissionNeeded));
                }

                Self::show_error(this, cx, TYPING_FAILED, RECORDING_ERROR_HIDE, None).await?;
            }
        }

        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(TRANSCRIPTION_CREATED_DELAY)
                .await;
            cx.update(|cx| events::emit(cx, AppEvent::TranscriptionCreated));
        })
        .detach();

        Ok(())
    }

    pub fn retry_transcription(&mut self, cx: &mut Context<Self>) {
        if self.dictation.stopping || self.dictation.loading {
            return;
        }
        let Some(retry) = self
            .dictation
            .error
            .as_mut()
            .and_then(|error| error.retry.take())
        else {
            return;
        };

        if !retry.retention.allowed() || !SettingsStore::get(cx).save_transcription_history {
            self.discard_pending_async(retry.audio_path, cx);
            cx.notify();
            return;
        }

        log::info!(target: "voice", "transcription_retry_requested");
        self.clear_error_state(cx);
        self.dictation.loading = true;
        self.window = WindowRequest {
            visible: true,
            width: WINDOW_WIDTH_COMPACT,
        };
        cx.notify();

        let retention = self.register_transcription(true);
        let lifecycle = self.lifecycle.clone();
        let pending = self.pending.clone();
        cx.spawn(async move |this, cx| {
            Self::transcribe_and_type(
                &this,
                cx,
                retry.audio_path,
                retry.target,
                retention,
                &lifecycle,
                &pending,
            )
            .await?;

            this.update(cx, |this, cx| {
                this.dictation.loading = false;
                if this.dictation.error.is_none() {
                    this.set_escape(false, cx);
                    this.window.visible = false;
                }
                cx.notify();
            })
        })
        .detach();
    }

    /// Shows the error named `message` for `auto_hide`, with a retry button
    /// if `retry` says what to retry.
    async fn show_error(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        message: &'static str,
        auto_hide: Duration,
        retry: Option<Retry>,
    ) -> Result<()> {
        let restore = this.update(cx, |this, cx| {
            this.clear_error_state(cx);
            this.cancel_pending_media_quiet();
            cx.notify();
            Self::restore_media(cx)
        })?;
        restore.await;

        this.update(cx, |this, cx| {
            if SettingsStore::get(cx).recording_sounds_enabled {
                sounds::play(Cue::Error);
            }

            let retry = match retry {
                Some(retry)
                    if !retry.retention.allowed()
                        || !SettingsStore::get(cx).save_transcription_history =>
                {
                    this.discard_pending_async(retry.audio_path, cx);
                    None
                }
                other => other,
            };

            this.dictation.error = Some(DictationError {
                message: t(cx, message),
                retry,
            });
            this.window = WindowRequest {
                visible: true,
                width: WINDOW_WIDTH_ERROR,
            };
            cx.notify();

            // Escape dismisses the error.
            this.set_escape(true, cx);
            this.error_hide_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(auto_hide).await;
                this.update(cx, |this, cx| this.dismiss_error(cx)).ok();
            }));
        })
    }

    pub fn dismiss_error(&mut self, cx: &mut Context<Self>) {
        if self.dictation.error.is_none() {
            return;
        }

        self.clear_error_state(cx);
        self.set_escape(false, cx);
        if self.dictation.recording.is_none() && !self.dictation.loading {
            self.window.visible = false;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_is_quieted_at_once_without_a_start_cue() {
        assert_eq!(media_quiet_delay(None, Instant::now()), Duration::ZERO);
    }

    #[test]
    fn media_is_quieted_once_the_start_cue_has_played() {
        let cue_started_at = Instant::now();
        let after = |elapsed: u64| cue_started_at + Duration::from_millis(elapsed);

        assert_eq!(
            media_quiet_delay(Some(cue_started_at), after(0)),
            Duration::from_millis(180)
        );
        assert_eq!(
            media_quiet_delay(Some(cue_started_at), after(50)),
            Duration::from_millis(130)
        );
        assert_eq!(
            media_quiet_delay(Some(cue_started_at), after(400)),
            Duration::ZERO
        );
    }
}
