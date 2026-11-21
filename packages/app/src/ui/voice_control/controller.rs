//! Runs dictation: the hotkeys start and end a recording, the recording is
//! transcribed, and the text is typed into the app in front. The controller
//! belongs to the app, not to a window; the overlay only shows its state.

use gpui_kit::{
    App, AppContext as _, AsyncApp, Context, Entity, Result, Subscription, Task, WeakEntity,
};
use std::path::PathBuf;
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
use super::state::{Command, Dictation, DictationError, Pill, RecordingMode, Retry, Target};

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

pub struct VoiceController {
    dictation: Dictation,
    audio_level: f32,
    wave_offset: usize,
    /// Where the running recording will be typed.
    target: Target,
    /// The active onboarding step 7 practice session, if step 7 is showing.
    learning_session: Option<u64>,
    /// The practice session the current recording started in, if any.
    recording_session: Option<u64>,
    /// The active transcription token and the practice session it belongs to.
    active_loading: Option<(u64, Option<u64>)>,
    /// The loading token currently holding `dictation.stopping`, so an
    /// invalidated practice transcription can release the controls once the
    /// microphone has stopped without affecting normal dictation sequencing.
    stopping_loading_token: Option<u64>,
    next_loading_token: u64,
    /// The practice session the shown error belongs to, if any.
    error_session: Option<u64>,
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
                hotkeys.register_dictation(toggle, hold, cx)
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
            audio_level: 0.,
            wave_offset: 0,
            target: Target::default(),
            learning_session: None,
            recording_session: None,
            active_loading: None,
            stopping_loading_token: None,
            next_loading_token: 0,
            error_session: None,
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
    /// and a running recording is dropped.
    pub fn shut_down(&mut self, cx: &App) {
        let backend = backend::backend(cx);

        if let Err(error) = backend.restore_media_after_recording() {
            log::error!(target: "voice", "restore_media_on_exit_failed error={error}");
        }
        if self.dictation.recording.is_some() || self.dictation.starting {
            let _ = backend.stop_recording();
        }
    }

    fn handle_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::AudioLevel(level) => {
                self.audio_level = *level;
                cx.notify();
            }
            AppEvent::LearningStepStarted { session } => {
                self.learning_session = Some(*session);
                self.plan_hotkeys(true, false, cx);
                self.cancel_unstoppable_recording(cx);
                self.invalidate_practice_state(cx);
            }
            AppEvent::LearningStepEnded { session } => {
                if self.learning_session == Some(*session) {
                    self.learning_session = None;
                }
                self.plan_hotkeys(self.hotkeys_live(), false, cx);
                self.cancel_unstoppable_recording(cx);
                self.invalidate_practice_state(cx);
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
            AppEvent::RecordingError { message } => self.recording_failed(message, cx),
            _ => {}
        }
    }

    fn handle_hotkey(&mut self, event: HotkeyEvent, cx: &mut Context<Self>) {
        let command = match event {
            HotkeyEvent::Escape => Some(self.dictation.escape_pressed()),
            _ if !self.hotkeys_live() => None,
            HotkeyEvent::Pressed(HotkeyRole::Toggle) => self.dictation.toggle_pressed(),
            HotkeyEvent::Pressed(HotkeyRole::Hold) => self.dictation.hold_pressed(),
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
        self.onboarding_complete || self.learning_session.is_some()
    }

    /// Identifies the active onboarding practice session, or `None` for normal
    /// dictation outside step 7.
    fn current_practice_session(&self) -> Option<u64> {
        self.learning_session
    }

    /// Whether a dictation started in `session` is still allowed to deliver
    /// its result. Normal dictations (`None`) are always active; practice ones
    /// expire as soon as the user leaves step 7.
    fn session_active(&self, session: Option<u64>) -> bool {
        session.is_none_or(|id| self.learning_session == Some(id))
    }

    fn begin_loading(&mut self, session: Option<u64>, hold_stopping: bool) -> u64 {
        self.next_loading_token = self.next_loading_token.wrapping_add(1);
        let token = self.next_loading_token;
        self.dictation.loading = true;
        self.active_loading = Some((token, session));
        if hold_stopping {
            self.stopping_loading_token = Some(token);
        }
        token
    }

    fn is_loading_token(&self, token: u64) -> bool {
        self.active_loading.is_some_and(|(id, _)| id == token)
    }

    /// Ends loading (and any stop lock held for this transcription) if `token`
    /// is still the active transcription.
    fn finish_loading(&mut self, token: u64) -> bool {
        if self.stopping_loading_token == Some(token) {
            self.stopping_loading_token = None;
            self.dictation.stopping = false;
        }
        if self.is_loading_token(token) {
            self.active_loading = None;
            self.dictation.loading = false;
            true
        } else {
            false
        }
    }

    /// Drops any loading pill, stop lock, or error from an ended onboarding
    /// practice session; any in-flight transcription checks `session_active`
    /// and discards its practice delivery when it returns.
    fn invalidate_practice_state(&mut self, cx: &mut Context<Self>) {
        if let Some((token, session)) = self.active_loading
            && !self.session_active(session)
        {
            self.finish_loading(token);
            if self.dictation.recording.is_none() && self.dictation.error.is_none() {
                self.window.visible = false;
            }
            cx.notify();
        }

        if self.dictation.error.is_some() && !self.session_active(self.error_session) {
            self.dismiss_error(cx);
        }
    }

    fn settings_changed(&mut self, cx: &mut Context<Self>) {
        let now_complete = SettingsStore::get(cx).onboarding_complete;
        let finished_onboarding = !self.onboarding_complete && now_complete;
        self.onboarding_complete = now_complete;

        self.plan_hotkeys(self.hotkeys_live(), finished_onboarding, cx);
    }

    /// Brings the registered hotkeys in line with the settings.
    fn plan_hotkeys(&mut self, live: bool, force: bool, cx: &mut Context<Self>) {
        let (toggle, hold) = dictation_hotkeys(SettingsStore::get(cx));
        let plan = hotkey_plan(
            live,
            &toggle,
            &hold,
            self.hotkeys.read(cx).registered(),
            force,
        );

        self.hotkeys.update(cx, |hotkeys, cx| match plan {
            HotkeyPlan::Register { toggle, hold } => hotkeys.register_dictation(toggle, hold, cx),
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

    fn clear_error_state(&mut self) {
        self.error_hide_timer = None;
        self.dictation.error = None;
        self.error_session = None;
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
        let session = self.current_practice_session();
        self.recording_session = session;
        self.clear_error_state();
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
        let frontmost = backend::call(cx, |backend| backend.get_frontmost_app());

        cx.spawn(async move |this, cx| {
            let frontmost = frontmost.await;

            let start_request = this.update(cx, |this, cx| {
                if !this.session_active(session) {
                    this.dictation.starting = false;
                    this.recording_session = None;
                    cx.notify();
                    return None;
                }

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

                Some((
                    backend::call(cx, move |backend| backend.start_recording(device)),
                    has_selected_microphone,
                ))
            })?;
            let Some((start, has_selected_microphone)) = start_request else {
                return Ok(());
            };

            if let Err(error) = start.await {
                log::error!(
                    target: "voice",
                    "start_recording_failed error={error} has_selected_microphone={has_selected_microphone}"
                );
                let active = this.update(cx, |this, cx| {
                    this.dictation.starting = false;
                    this.recording_session = None;
                    let active = this.session_active(session);
                    if !active
                        && this.dictation.recording.is_none()
                        && !this.dictation.loading
                        && this.dictation.error.is_none()
                    {
                        this.window.visible = false;
                        cx.notify();
                    }
                    active
                })?;
                if active {
                    return Self::show_error(
                        &this,
                        cx,
                        RECORDING_FAILED,
                        RECORDING_ERROR_HIDE,
                        None,
                        session,
                        None,
                    )
                    .await;
                }
                return Ok(());
            }

            this.update(cx, |this, cx| {
                // The input has failed already, and that failure was reported.
                if !this.dictation.starting {
                    return;
                }

                this.dictation.starting = false;
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
                this.cancel_unstoppable_recording(cx);
            })
        })
        .detach();
    }

    /// Cancels a trial dictation whose onboarding step went away (the user
    /// moved on, or closed the window): its hotkeys went with the step, so
    /// they could no longer stop it.
    fn cancel_unstoppable_recording(&mut self, cx: &mut Context<Self>) {
        if !self.hotkeys_live() || !self.session_active(self.recording_session) {
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
        let session = self.recording_session.take();
        self.cancel_pending_media_quiet();
        self.dictation.stopping = true;
        self.set_recording(None, cx);
        self.set_escape(false, cx);
        self.window.width = WINDOW_WIDTH_COMPACT;

        // Media is restored before the recorder is touched, so that a failed
        // stop cannot leave the output muted or muffled.
        let restore = Self::restore_media(cx);

        enum StopOutcome {
            Transcribe {
                audio_path: PathBuf,
                target: Target,
                loading_token: u64,
            },
            Discarded,
            Failed {
                error: String,
                active: bool,
            },
        }

        cx.spawn(async move |this, cx| {
            restore.await;

            let stop = cx.update(|cx| backend::call(cx, |backend| backend.stop_recording()));
            let stop_result = stop.await;
            // Finish restoring media before releasing `stopping`, so a new
            // recording cannot start and have its media unmuted by this stop.
            cx.update(|cx| Self::restore_media(cx)).await;

            let outcome = this.update(cx, |this, cx| {
                let target = std::mem::take(&mut this.target);

                match stop_result {
                    Ok(audio_path) if this.session_active(session) => {
                        // Keep `stopping` held until this transcription ends
                        // (or its practice session is invalidated), preserving
                        // normal dictation sequencing while allowing an ended
                        // practice session to release controls immediately.
                        let loading_token = this.begin_loading(session, true);
                        cx.notify();
                        StopOutcome::Transcribe {
                            audio_path,
                            target,
                            loading_token,
                        }
                    }
                    Ok(_) => {
                        this.dictation.stopping = false;
                        if this.dictation.recording.is_none()
                            && !this.dictation.loading
                            && this.dictation.error.is_none()
                        {
                            this.window.visible = false;
                        }
                        cx.notify();
                        StopOutcome::Discarded
                    }
                    Err(error) => {
                        let active = this.session_active(session);
                        if !active {
                            this.dictation.stopping = false;
                            if this.dictation.recording.is_none()
                                && !this.dictation.loading
                                && this.dictation.error.is_none()
                            {
                                this.window.visible = false;
                            }
                            cx.notify();
                        }
                        StopOutcome::Failed { error, active }
                    }
                }
            })?;

            match outcome {
                StopOutcome::Transcribe {
                    audio_path,
                    target,
                    loading_token,
                } => {
                    Self::transcribe_and_type(
                        &this,
                        cx,
                        audio_path,
                        target,
                        session,
                        loading_token,
                    )
                    .await?;

                    this.update(cx, |this, cx| {
                        if this.finish_loading(loading_token)
                            && this.dictation.recording.is_none()
                            && this.dictation.error.is_none()
                        {
                            this.window.visible = false;
                        }
                        cx.notify();
                        log::info!(target: "voice", "stop_recording_completed");
                    })
                }
                StopOutcome::Discarded => {
                    log::info!(target: "voice", "practice_transcription_discarded");
                    log::info!(target: "voice", "stop_recording_completed");
                    Ok(())
                }
                StopOutcome::Failed { error, active } => {
                    log::error!(target: "voice", "stop_recording_failed error={error}");
                    if active {
                        Self::show_error(
                            &this,
                            cx,
                            RECORDING_FAILED,
                            RECORDING_ERROR_HIDE,
                            None,
                            session,
                            None,
                        )
                        .await?;
                        return this.update(cx, |this, cx| {
                            this.dictation.stopping = false;
                            if this.dictation.recording.is_none()
                                && !this.dictation.loading
                                && this.dictation.error.is_none()
                            {
                                this.window.visible = false;
                            }
                            cx.notify();
                            log::info!(target: "voice", "stop_recording_completed");
                        });
                    }
                    log::info!(target: "voice", "stop_recording_completed");
                    Ok(())
                }
            }
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
        self.recording_session = None;
        self.cancel_pending_media_quiet();
        self.dictation.stopping = true;
        self.set_recording(None, cx);
        self.target = Target::default();
        self.set_escape(false, cx);
        self.window.width = WINDOW_WIDTH_COMPACT;

        let restore = Self::restore_media(cx);

        cx.spawn(async move |this, cx| {
            restore.await;

            let stop = cx.update(|cx| backend::call(cx, |backend| backend.stop_recording()));
            let _ = stop.await;

            this.update(cx, |this, cx| {
                this.dictation.stopping = false;
                if !this.dictation.loading && this.dictation.error.is_none() {
                    this.window.visible = false;
                }
                cx.notify();
            })
        })
        .detach();
    }

    /// The microphone stream died. The recorder has reset itself already;
    /// this resets the state here, so the next press of a hotkey starts
    /// afresh, and reports the failure.
    fn recording_failed(&mut self, message: &str, cx: &mut Context<Self>) {
        log::error!(target: "voice", "recording_error_event message={message}");
        if self.dictation.recording.is_none() && !self.dictation.starting {
            return;
        }

        let session = self.recording_session.take();
        self.set_recording(None, cx);
        self.target = Target::default();
        self.dictation.starting = false;
        self.dictation.stopping = false;

        let restore = Self::restore_media(cx);

        cx.spawn(async move |this, cx| {
            restore.await;
            let active = this.read_with(cx, |this, _| this.session_active(session))?;
            if active {
                Self::show_error(
                    &this,
                    cx,
                    RECORDING_FAILED,
                    RECORDING_ERROR_HIDE,
                    None,
                    session,
                    None,
                )
                .await
            } else {
                this.update(cx, |this, cx| {
                    if this.dictation.recording.is_none()
                        && !this.dictation.loading
                        && this.dictation.error.is_none()
                    {
                        this.window.visible = false;
                    }
                    cx.notify();
                })
            }
        })
        .detach();
    }

    /// Transcribes the recording at `audio_path`, saves the result to the
    /// history and delivers it. When `session` is `Some`, the transcription
    /// belongs to an onboarding practice session: its text is routed directly
    /// to that step 7 session on the UI thread and discarded if the user left
    /// step 7 before delivery.
    async fn transcribe_and_type(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        audio_path: PathBuf,
        target: Target,
        session: Option<u64>,
        loading_token: u64,
    ) -> Result<()> {
        let style = cx.update(|cx| SettingsStore::get(cx).default_style.clone());
        let transcription = cx.update(|cx| {
            let (audio_path, target, style) = (audio_path.clone(), target.clone(), style.clone());
            backend::call(cx, move |backend| {
                backend.transcribe_audio(audio_path, target.bundle_id, target.domain, style)
            })
        });

        let result = match transcription.await {
            Ok(result) => result,
            Err(error) => {
                log::error!(target: "voice", "transcription_failed error={error}");
                return Self::show_error(
                    this,
                    cx,
                    TRANSCRIPTION_FAILED,
                    TRANSCRIPTION_ERROR_HIDE,
                    Some(Retry { audio_path, target }),
                    session,
                    Some(loading_token),
                )
                .await;
            }
        };

        if !this.read_with(cx, |this, _| this.session_active(session))? {
            log::info!(target: "voice", "practice_transcription_discarded");
            return Ok(());
        }

        let saved = cx.update(|cx| {
            let result = result.clone();
            backend::call(cx, move |backend| {
                backend.save_transcription(&result, &audio_path)
            })
        });
        let saved_ok = match saved.await {
            Ok(_) => true,
            Err(error) => {
                log::error!(target: "voice", "save_transcription_failed error={error}");
                false
            }
        };
        log::info!(
            target: "voice",
            "transcription_completed word_count={} processing_time_ms={} audio_duration_ms={:?} has_app_context={} has_site_context={} style={style}",
            result.word_count,
            result.processing_time_ms,
            result.audio_duration_ms,
            target.bundle_id.is_some(),
            target.domain.is_some(),
        );

        // Practice dictations are validated and delivered synchronously on the
        // UI thread so they can never type into another app or a later step
        // after Skip, Back, Next or closing the window.
        if let Some(practice_session) = session {
            // Refresh Home whenever the row was persisted, even if the user
            // left step 7 while the database write was in flight.
            if saved_ok {
                Self::emit_transcription_created(cx);
            }

            let delivered = this.update(cx, |this, cx| {
                if !this.session_active(Some(practice_session)) {
                    return false;
                }
                if SettingsStore::get(cx).recording_sounds_enabled {
                    sounds::play(Cue::Bloom);
                }
                events::emit(
                    cx,
                    AppEvent::PracticeTranscription {
                        session: practice_session,
                        text: result.text.clone(),
                    },
                );
                true
            })?;

            if !delivered {
                log::info!(target: "voice", "practice_transcription_discarded");
            }
            return Ok(());
        }

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
                    let still_active = this.update(cx, |this, cx| {
                        if this.session_active(session) {
                            events::emit(cx, AppEvent::AccessibilityPermissionNeeded);
                            true
                        } else {
                            false
                        }
                    })?;
                    if !still_active {
                        return Ok(());
                    }
                }

                Self::show_error(
                    this,
                    cx,
                    TYPING_FAILED,
                    RECORDING_ERROR_HIDE,
                    None,
                    session,
                    Some(loading_token),
                )
                .await?;
            }
        }

        Self::emit_transcription_created(cx);
        Ok(())
    }

    fn emit_transcription_created(cx: &mut AsyncApp) {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(TRANSCRIPTION_CREATED_DELAY)
                .await;
            cx.update(|cx| events::emit(cx, AppEvent::TranscriptionCreated));
        })
        .detach();
    }

    pub fn retry_transcription(&mut self, cx: &mut Context<Self>) {
        let Some(retry) = self.error().and_then(|error| error.retry.clone()) else {
            return;
        };
        if self.dictation.stopping || self.dictation.loading {
            return;
        }

        let session = self.error_session;
        if !self.session_active(session) {
            self.dismiss_error(cx);
            return;
        }

        log::info!(target: "voice", "transcription_retry_requested");
        self.clear_error_state();
        let loading_token = self.begin_loading(session, false);
        self.window = WindowRequest {
            visible: true,
            width: WINDOW_WIDTH_COMPACT,
        };
        cx.notify();

        cx.spawn(async move |this, cx| {
            Self::transcribe_and_type(
                &this,
                cx,
                retry.audio_path,
                retry.target,
                session,
                loading_token,
            )
            .await?;

            this.update(cx, |this, cx| {
                if this.finish_loading(loading_token)
                    && this.dictation.error.is_none()
                    && this.dictation.recording.is_none()
                {
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
        session: Option<u64>,
        loading_token: Option<u64>,
    ) -> Result<()> {
        let restore = this.update(cx, |this, cx| {
            let recording_active = this.dictation.recording.is_some() || this.dictation.starting;
            if !this.session_active(session)
                || loading_token
                    .is_some_and(|token| !this.is_loading_token(token) || recording_active)
            {
                return None;
            }
            this.clear_error_state();
            let restore = if !recording_active {
                this.cancel_pending_media_quiet();
                Some(Self::restore_media(cx))
            } else {
                None
            };
            cx.notify();
            Some(restore)
        })?;
        let Some(restore) = restore else {
            return Ok(());
        };
        if let Some(restore) = restore {
            restore.await;
        }

        this.update(cx, |this, cx| {
            let recording_active = this.dictation.recording.is_some() || this.dictation.starting;
            if !this.session_active(session)
                || loading_token
                    .is_some_and(|token| !this.is_loading_token(token) || recording_active)
            {
                return;
            }
            if SettingsStore::get(cx).recording_sounds_enabled {
                sounds::play(Cue::Error);
            }

            this.dictation.error = Some(DictationError {
                message: t(cx, message),
                retry,
            });
            this.error_session = session;
            if !recording_active {
                this.window = WindowRequest {
                    visible: true,
                    width: WINDOW_WIDTH_ERROR,
                };
                // Escape dismisses the error.
                this.set_escape(true, cx);
            }
            cx.notify();

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

        self.clear_error_state();
        if self.dictation.recording.is_none() {
            self.set_escape(false, cx);
            if !self.dictation.loading {
                self.window.visible = false;
            }
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
