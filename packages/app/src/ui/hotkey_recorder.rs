//! Records a new dictation hotkey from the keys the user presses.
//!
//! Modifier-only hotkeys (`RightCommand`) are reported by the native key
//! watcher, which tells left keys from right ones. Combinations ending in a
//! regular key (`Command+Shift+K`) come from the window's key events, which
//! the view showing a recorder passes on with [`route_keys`].

use gpui_kit::{
    App, Context, Entity, EntityId, Global, InteractiveElement, KeyDownEvent, KeyUpEvent,
    Modifiers, ModifiersChangedEvent, SharedString, Subscription,
};

use crate::backend::{self, AppEvent, SystemKey, command_error};
use crate::events;
use crate::settings::SettingsStore;
use crate::ui::hotkeys::{
    hotkey_display_name, system_hotkey_from_keys, validate_hands_free_hotkey,
    validate_hold_to_speak_hotkey,
};

/// Which of the two dictation hotkeys a recorder sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyKind {
    HandsFree,
    HoldToSpeak,
}

impl HotkeyKind {
    fn validate(self, hotkey: &str) -> Option<&'static str> {
        match self {
            HotkeyKind::HandsFree => validate_hands_free_hotkey(hotkey),
            HotkeyKind::HoldToSpeak => validate_hold_to_speak_hotkey(hotkey),
        }
    }

    fn save(self, hotkey: String, cx: &mut App) {
        SettingsStore::update(cx, |settings| match self {
            HotkeyKind::HandsFree => settings.hotkey = hotkey,
            HotkeyKind::HoldToSpeak => settings.hold_to_speak_hotkey = hotkey,
        });
    }
}

/// The recorder that is recording. Only one may record at a time, in the
/// whole app, since they would all react to the same keys.
#[derive(Default)]
struct ActiveRecorder(Option<EntityId>);

impl Global for ActiveRecorder {}

impl ActiveRecorder {
    fn is_free_for(&self, recorder: EntityId) -> bool {
        self.0.is_none_or(|active| active == recorder)
    }

    fn release(&mut self, recorder: EntityId) {
        if self.0 == Some(recorder) {
            self.0 = None;
        }
    }
}

/// The name hotkey strings use for the key GPUI calls `key`: the key's own
/// name, whatever a held Shift turns it into.
fn hotkey_key(key: &str) -> String {
    let mut characters = key.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        return unshifted(character).to_uppercase().collect();
    }

    let named = match key {
        "space" => "Space",
        "tab" => "Tab",
        "enter" => "Enter",
        "escape" => "Escape",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "up" => "ArrowUp",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "insert" => "Insert",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_string();
    }

    let is_function_key = key
        .strip_prefix('f')
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
    if is_function_key {
        return key.to_uppercase();
    }

    key.to_string()
}

/// The character the key producing `character` carries without Shift, on
/// the US layout that hotkey strings are written in.
fn unshifted(character: char) -> char {
    const SHIFTED: &str = "~!@#$%^&*()_+{}|:\"<>?";
    const PLAIN: &str = "`1234567890-=[]\\;',./";

    SHIFTED
        .chars()
        .zip(PLAIN.chars())
        .find(|(shifted, _)| *shifted == character)
        .map_or(character, |(_, plain)| plain)
}

/// The hotkey string for `key` pressed while `held` modifiers are down.
fn combination(held: Modifiers, key: &str) -> String {
    [
        (held.platform, "Command"),
        (held.control, "Control"),
        (held.alt, "Alt"),
        (held.shift, "Shift"),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .map(|(_, name)| name)
    .chain([key])
    .collect::<Vec<_>>()
    .join("+")
}

/// The keys pressed since recording started.
#[derive(Debug, Default)]
struct Capture {
    /// What the recorder shows: the modifiers held, or the combination pressed.
    pending: String,
    /// The modifiers pressed since recording started. A combination takes
    /// its modifiers from here, so one held from before does not count.
    held: Modifiers,
    /// The modifier-only hotkey the native watcher last reported.
    system_hotkey: String,
}

impl Capture {
    fn modifiers_changed(&mut self, before: Modifiers, now: Modifiers) {
        let track = |held: &mut bool, before: bool, now: bool| {
            if now != before {
                *held = now;
            }
        };

        track(&mut self.held.platform, before.platform, now.platform);
        track(&mut self.held.control, before.control, now.control);
        track(&mut self.held.alt, before.alt, now.alt);
        track(&mut self.held.shift, before.shift, now.shift);
    }

    fn key_pressed(&mut self, key: &str) {
        self.pending = combination(self.held, &hotkey_key(key));
    }

    /// The combination to save now that a regular key was released.
    fn key_released(&self) -> Option<String> {
        self.pending.contains('+').then(|| self.pending.clone())
    }

    fn system_keys_pressed(&mut self, keys: &[SystemKey]) {
        self.system_hotkey = system_hotkey_from_keys(keys);
        self.pending = hotkey_display_name(&self.system_hotkey);
    }

    /// The modifier-only hotkey to save now that every modifier was released.
    fn system_keys_released(&self) -> Option<String> {
        (!self.system_hotkey.is_empty()).then(|| self.system_hotkey.clone())
    }
}

pub struct HotkeyRecorder {
    kind: HotkeyKind,
    recording: bool,
    capture: Capture,
    error: Option<SharedString>,
    /// The modifiers as of the latest change, to tell presses from releases.
    modifiers: Modifiers,
    starting_watcher: bool,
    _subscriptions: Vec<Subscription>,
}

impl HotkeyRecorder {
    pub fn new(kind: HotkeyKind, cx: &mut Context<Self>) -> Self {
        let recorder = cx.entity_id();
        let subscriptions = vec![
            cx.subscribe(&events::hub(cx), |this, _, event, cx| {
                this.handle_event(event, cx);
            }),
            cx.on_release(move |_, cx| {
                cx.default_global::<ActiveRecorder>().release(recorder);
                events::emit(cx, AppEvent::HotkeyRecorderActive { active: false });
            }),
        ];

        Self {
            kind,
            recording: false,
            capture: Capture::default(),
            error: None,
            modifiers: Modifiers::default(),
            starting_watcher: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// What has been pressed so far, as shown while recording. Empty until
    /// the first key.
    pub fn pending_hotkey(&self) -> &str {
        &self.capture.pending
    }

    pub fn error(&self) -> Option<SharedString> {
        self.error.clone()
    }

    pub fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording {
            self.finish_recording(cx);
            self.error = None;
        } else {
            self.start_recording(cx);
        }
    }

    fn start_recording(&mut self, cx: &mut Context<Self>) {
        let recorder = cx.entity_id();
        if !cx.default_global::<ActiveRecorder>().is_free_for(recorder) {
            self.error =
                Some("Another hotkey is being recorded. Please finish or cancel it first.".into());
            cx.notify();
            return;
        }
        if self.starting_watcher {
            return;
        }
        self.starting_watcher = true;

        let watcher = backend::call(cx, |backend| backend.start_system_key_watcher());
        cx.spawn(async move |this, cx| {
            let started = watcher.await;

            this.update(cx, |this, cx| {
                this.starting_watcher = false;

                if let Err(error) = started {
                    this.error = Some(command_error("start_system_key_watcher", &error).into());
                    cx.notify();
                    return;
                }

                cx.default_global::<ActiveRecorder>().0 = Some(recorder);
                this.recording = true;
                this.capture = Capture::default();
                this.error = None;
                events::emit(cx, AppEvent::HotkeyRecorderActive { active: true });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Ends the recording and lets another recorder start. Every way out of
    /// a recording goes through here, or the next recorder would be told
    /// that another hotkey is being recorded.
    fn finish_recording(&mut self, cx: &mut Context<Self>) {
        let recorder = cx.entity_id();
        cx.default_global::<ActiveRecorder>().release(recorder);

        self.recording = false;
        self.capture = Capture::default();
        events::emit(cx, AppEvent::HotkeyRecorderActive { active: false });
        cx.notify();
    }

    /// Saves `hotkey` and ends the recording, unless the hotkey is not
    /// allowed: then the reason is shown and recording goes on.
    fn save(&mut self, hotkey: String, cx: &mut Context<Self>) {
        if let Some(reason) = self.kind.validate(&hotkey) {
            self.error = Some(reason.into());
            cx.notify();
            return;
        }

        self.error = None;
        self.kind.save(hotkey, cx);
        self.finish_recording(cx);
    }

    fn handle_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }

        match event {
            AppEvent::SystemKeyPressed { pressed_keys, .. } => {
                self.capture.system_keys_pressed(pressed_keys);
                cx.notify();
            }
            AppEvent::SystemKeysReleased => {
                if let Some(hotkey) = self.capture.system_keys_released() {
                    self.save(hotkey, cx);
                }
            }
            _ => {}
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }

        // Nothing else acts on a key pressed for the recorder.
        cx.stop_propagation();
        self.capture.key_pressed(&event.keystroke.key);
        cx.notify();
    }

    fn key_up(&mut self, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }

        if let Some(hotkey) = self.capture.key_released() {
            self.save(hotkey, cx);
        }
    }

    fn modifiers_changed(&mut self, event: &ModifiersChangedEvent) {
        if self.recording {
            self.capture
                .modifiers_changed(self.modifiers, event.modifiers);
        }
        self.modifiers = event.modifiers;
    }
}

/// Passes the key events `element` receives on to `recorders`. The element
/// has to hold the keyboard focus, or contain what does.
pub fn route_keys<E: InteractiveElement>(element: E, recorders: &[Entity<HotkeyRecorder>]) -> E {
    let on_key_down = recorders.to_vec();
    let on_key_up = recorders.to_vec();
    let on_modifiers_changed = recorders.to_vec();

    element
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            for recorder in &on_key_down {
                recorder.update(cx, |recorder, cx| recorder.key_down(event, cx));
            }
        })
        .on_key_up(move |_: &KeyUpEvent, _, cx| {
            for recorder in &on_key_up {
                recorder.update(cx, |recorder, cx| recorder.key_up(cx));
            }
        })
        .on_modifiers_changed(move |event: &ModifiersChangedEvent, _, cx| {
            for recorder in &on_modifiers_changed {
                recorder.update(cx, |recorder, _| recorder.modifiers_changed(event));
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modifiers(platform: bool, control: bool, alt: bool, shift: bool) -> Modifiers {
        Modifiers {
            platform,
            control,
            alt,
            shift,
            function: false,
        }
    }

    #[test]
    fn keys_get_the_names_hotkey_strings_use() {
        assert_eq!(hotkey_key("k"), "K");
        assert_eq!(hotkey_key("1"), "1");
        assert_eq!(hotkey_key(";"), ";");
        assert_eq!(hotkey_key("`"), "`");
        assert_eq!(hotkey_key("space"), "Space");
        assert_eq!(hotkey_key("enter"), "Enter");
        assert_eq!(hotkey_key("escape"), "Escape");
        assert_eq!(hotkey_key("backspace"), "Backspace");
        assert_eq!(hotkey_key("delete"), "Delete");
        assert_eq!(hotkey_key("tab"), "Tab");
        assert_eq!(hotkey_key("down"), "ArrowDown");
        assert_eq!(hotkey_key("left"), "ArrowLeft");
        assert_eq!(hotkey_key("right"), "ArrowRight");
        assert_eq!(hotkey_key("up"), "ArrowUp");
        assert_eq!(hotkey_key("pageup"), "PageUp");
        assert_eq!(hotkey_key("f5"), "F5");
        assert_eq!(hotkey_key("f12"), "F12");
        assert_eq!(hotkey_key("ö"), "Ö");
        assert_eq!(hotkey_key("forward"), "forward");
    }

    #[test]
    fn a_shifted_key_keeps_its_own_name() {
        assert_eq!(hotkey_key("!"), "1");
        assert_eq!(hotkey_key(")"), "0");
        assert_eq!(hotkey_key(":"), ";");
        assert_eq!(hotkey_key("?"), "/");
        assert_eq!(hotkey_key("~"), "`");
        assert_eq!(hotkey_key("|"), "\\");
    }

    #[test]
    fn modifiers_come_in_a_fixed_order() {
        assert_eq!(
            combination(modifiers(true, true, true, true), "K"),
            "Command+Control+Alt+Shift+K"
        );
        assert_eq!(
            combination(modifiers(false, true, false, true), "F5"),
            "Control+Shift+F5"
        );
        assert_eq!(combination(Modifiers::default(), "K"), "K");
    }

    #[test]
    fn a_combination_is_saved_when_its_key_is_released() {
        let mut capture = Capture::default();
        capture.modifiers_changed(Modifiers::default(), modifiers(false, true, false, false));
        capture.key_pressed("k");

        assert_eq!(capture.pending, "Control+K");
        assert_eq!(capture.key_released().as_deref(), Some("Control+K"));
        assert!(crate::ui::hotkeys::is_valid_hotkey("Control+K"));
    }

    #[test]
    fn a_key_without_modifiers_is_shown_but_not_saved() {
        let mut capture = Capture::default();
        capture.key_pressed("k");

        assert_eq!(capture.pending, "K");
        assert_eq!(capture.key_released(), None);
    }

    #[test]
    fn a_modifier_held_since_before_recording_does_not_count() {
        let mut capture = Capture::default();
        let command = modifiers(true, false, false, false);

        // Shift goes down while Command, held from before, stays down.
        capture.modifiers_changed(command, modifiers(true, false, false, true));
        capture.key_pressed("!");
        assert_eq!(capture.pending, "Shift+1");

        // Releasing Shift takes it out again.
        capture.modifiers_changed(modifiers(true, false, false, true), command);
        capture.key_pressed("k");
        assert_eq!(capture.pending, "K");
    }

    #[test]
    fn watched_modifiers_are_shown_and_saved_on_release() {
        let mut capture = Capture::default();
        assert_eq!(capture.system_keys_released(), None);

        capture.system_keys_pressed(&[SystemKey::LeftOption, SystemKey::LeftControl]);
        assert_eq!(capture.pending, "Left⌃+Left⌥");
        assert_eq!(
            capture.system_keys_released().as_deref(),
            Some("LeftControl+LeftOption")
        );
    }

    #[test]
    fn one_watched_modifier_is_not_taken_for_a_combination() {
        let mut capture = Capture::default();
        capture.system_keys_pressed(&[SystemKey::RightCommand]);

        assert_eq!(capture.pending, "Right⌘");
        assert_eq!(capture.key_released(), None);
    }

    #[test]
    fn fn_alone_is_refused_for_hold_to_speak_only() {
        assert_eq!(
            HotkeyKind::HoldToSpeak.validate("Fn"),
            Some("FN cannot be used as hold-to-speak hotkey")
        );
        assert_eq!(HotkeyKind::HandsFree.validate("Fn"), None);
        assert_eq!(HotkeyKind::HoldToSpeak.validate("RightCommand"), None);
        assert_eq!(
            HotkeyKind::HandsFree.validate("Command++"),
            Some("Use modifier keys plus one regular key.")
        );
    }

    #[test]
    fn only_one_recorder_records_at_a_time() {
        let (first, second) = (EntityId::from(1), EntityId::from(2));
        let mut active = ActiveRecorder::default();
        assert!(active.is_free_for(first));

        active.0 = Some(first);
        assert!(active.is_free_for(first));
        assert!(!active.is_free_for(second));

        // Only the recorder that is recording can end it.
        active.release(second);
        assert!(!active.is_free_for(second));
        active.release(first);
        assert!(active.is_free_for(second));
    }
}
