//! The global shortcuts dictation listens to: the hands-free and the
//! hold-to-speak hotkey, and Escape while there is something to cancel.
//!
//! A hotkey made of modifier keys alone is matched here against the key
//! watcher's events; any other is registered as a global shortcut.

use gpui_kit::{AsyncApp, Context, EventEmitter, Result, Subscription, Task, WeakEntity};
use std::time::Duration;

use global_hotkey::hotkey::HotKey;

use crate::backend::{self, AppEvent, ShortcutState, SystemKey};
use crate::events;
use crate::settings::{DEFAULT_HOLD_TO_SPEAK_HOTKEY, DEFAULT_HOTKEY, Settings};
use crate::ui::hotkeys::{is_valid_hotkey, system_keys_from_hotkey};

/// How long an exact match of a modifier-only hotkey must stay held, unchanged,
/// before it fires. Long enough to outlast the modifier transients of key
/// remappers (Karabiner posts a hyperkey's modifiers within about a
/// millisecond of each other), too short to notice on a deliberate press.
const MATCH_STABILITY: Duration = Duration::from_millis(60);

const ESCAPE: &str = "Escape";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyRole {
    /// Each press starts or ends a hands-free recording.
    Toggle,
    /// Records for as long as it is held.
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed(HotkeyRole),
    Released(HotkeyRole),
    Escape,
}

/// What a key event means for one modifier-only hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    Ignored,
    /// The held keys are exactly the hotkey: fire once they have stayed so
    /// for the stability window, which starts over.
    AwaitStability,
    /// The held keys are not the hotkey (any more): stop waiting, and report
    /// a release if the hotkey had fired.
    Cancelled {
        released: bool,
    },
}

/// One modifier-only hotkey, followed through the key watcher's events.
#[derive(Debug)]
pub struct SystemHotkey {
    keys: Vec<SystemKey>,
    held: bool,
    /// The settings are recording a new hotkey, which must not trigger this one.
    recorder_active: bool,
    awaiting_stability: bool,
}

impl SystemHotkey {
    pub fn new(keys: Vec<SystemKey>) -> Self {
        Self {
            keys,
            held: false,
            recorder_active: false,
            awaiting_stability: false,
        }
    }

    fn matches(&self, pressed_keys: &[SystemKey]) -> bool {
        self.keys.iter().all(|key| pressed_keys.contains(key))
            && pressed_keys.iter().all(|key| self.keys.contains(key))
    }

    fn cancel(&mut self) -> Reaction {
        self.awaiting_stability = false;
        Reaction::Cancelled {
            released: std::mem::take(&mut self.held),
        }
    }

    pub fn key_pressed(&mut self, pressed_keys: &[SystemKey]) -> Reaction {
        if !self.matches(pressed_keys) {
            // Another key joined the chord: a pending match was a transient
            // state (a remapper posting a hyperkey's modifiers one by one),
            // not a deliberate press of this hotkey.
            return self.cancel();
        }
        if self.held || self.recorder_active {
            return Reaction::Ignored;
        }

        self.awaiting_stability = true;
        Reaction::AwaitStability
    }

    pub fn key_released(&mut self, pressed_keys: &[SystemKey]) -> Reaction {
        if self.matches(pressed_keys) {
            return Reaction::Ignored;
        }

        self.cancel()
    }

    pub fn all_keys_released(&mut self) -> Reaction {
        self.cancel()
    }

    pub fn set_recorder_active(&mut self, active: bool) -> Reaction {
        self.recorder_active = active;
        if !active {
            return Reaction::Ignored;
        }

        self.awaiting_stability = false;
        Reaction::Cancelled { released: false }
    }

    /// The match outlasted the stability window. Returns whether the hotkey
    /// fires.
    pub fn stabilized(&mut self) -> bool {
        if !std::mem::take(&mut self.awaiting_stability) {
            return false;
        }

        self.held = true;
        true
    }
}

/// The hotkeys in `settings`, with the default in place of one that neither
/// backend can register. Hold-to-speak is optional and stays unset when cleared.
pub fn dictation_hotkeys(settings: &Settings) -> (String, Option<String>) {
    let toggle = if is_valid_hotkey(&settings.hotkey) {
        settings.hotkey.clone()
    } else {
        DEFAULT_HOTKEY.to_string()
    };

    let hold = settings
        .hold_to_speak_hotkey
        .as_deref()
        .filter(|hotkey| !hotkey.trim().is_empty())
        .map(|hotkey| {
            if is_valid_hotkey(hotkey) {
                hotkey.to_string()
            } else {
                DEFAULT_HOLD_TO_SPEAK_HOTKEY.to_string()
            }
        });

    (toggle, hold)
}

/// What to do about the dictation hotkeys after something they depend on
/// changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyPlan {
    Register {
        toggle: String,
        hold: Option<String>,
    },
    Unregister,
    Keep,
}

/// Whether two hotkey strings name the same OS shortcut (for example
/// `Command+Shift+K` and `Shift+Command+K`, or `Command+K` and `Cmd+K`).
fn same_shortcut(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if let (Some(a), Some(b)) = (system_keys_from_hotkey(a), system_keys_from_hotkey(b)) {
        return a.iter().all(|key| b.contains(key)) && b.iter().all(|key| a.contains(key));
    }
    if let (Ok(a), Ok(b)) = (a.parse::<HotKey>(), b.parse::<HotKey>()) {
        return a.id() == b.id();
    }
    false
}

/// Identifies a pair of dictation hotkeys.
fn registration_key(toggle: &str, hold: Option<&str>) -> String {
    format!("{toggle}|{}", hold.unwrap_or_default())
}

/// `live` says whether dictation may be started at all; `registered` is
/// [`Hotkeys::registered`]; `force` registers again even if nothing changed.
pub fn hotkey_plan(
    live: bool,
    toggle: &str,
    hold: Option<&str>,
    registered: Option<&str>,
    force: bool,
) -> HotkeyPlan {
    if !live {
        return HotkeyPlan::Unregister;
    }

    if force || registered != Some(registration_key(toggle, hold).as_str()) {
        HotkeyPlan::Register {
            toggle: toggle.to_string(),
            hold: hold.map(str::to_string),
        }
    } else {
        HotkeyPlan::Keep
    }
}

struct DictationHotkey {
    hotkey: String,
    role: HotkeyRole,
    /// The matcher of a modifier-only hotkey; the others are global shortcuts.
    system: Option<SystemHotkey>,
    stability_timer: Option<Task<()>>,
}

/// Registers the shortcuts and turns their key events into [`HotkeyEvent`]s.
pub struct Hotkeys {
    dictation: Vec<DictationHotkey>,
    /// The pair of hotkeys last asked for (and registered once pending work
    /// finishes), or `None` when unregistered or a registration failed.
    registered: Option<String>,
    /// Incremented on every dictation change so stale async work skips itself
    /// and never overwrites a newer request's `registered` key.
    generation: u64,
    escape: bool,
    /// The registration changes still to be made, in the order asked for.
    pending: Task<()>,
    _subscription: Subscription,
}

impl EventEmitter<HotkeyEvent> for Hotkeys {}

impl Hotkeys {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            dictation: Vec::new(),
            registered: None,
            generation: 0,
            escape: false,
            pending: Task::ready(()),
            _subscription: cx.subscribe(&events::hub(cx), |this, _, event, cx| {
                this.handle_event(event, cx);
            }),
        }
    }

    pub fn registered(&self) -> Option<&str> {
        self.registered.as_deref()
    }

    /// Replaces the dictation hotkeys. One hotkey given for both is the
    /// hands-free hotkey. When `force` is false, any shortcut that is unchanged
    /// keeps its existing registration without interruption.
    pub fn register_dictation(
        &mut self,
        toggle: String,
        hold: Option<String>,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let key = registration_key(&toggle, hold.as_deref());
        self.generation += 1;
        let generation = self.generation;
        self.registered = Some(key.clone());

        self.enqueue(cx, async move |this, cx| {
            Self::replace_dictation(&this, cx, toggle, hold, key, force, generation)
                .await
                .ok();
        });
    }

    pub fn unregister_dictation(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        self.registered = None;

        self.enqueue(cx, async move |this, cx| {
            let current = this
                .update(cx, |this, _| this.generation == generation)
                .unwrap_or(false);
            if current {
                let _ = Self::release_dictation(&this, cx).await;
            }
        });
    }

    /// Whether Escape is taken from the other apps and reported.
    pub fn set_escape(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.escape == enabled {
            return;
        }
        self.escape = enabled;

        self.enqueue(cx, async move |_, cx| {
            let change = cx.update(|cx| {
                backend::call(cx, move |backend| {
                    if enabled {
                        backend.register_shortcut(ESCAPE)
                    } else {
                        backend.unregister_shortcut(ESCAPE)
                    }
                })
            });

            if let Err(error) = change.await {
                log::warn!(target: "voice", "escape_shortcut_failed enabled={enabled} error={error}");
            }
        });
    }

    /// Runs `change` after every change asked for before it, so that the
    /// backend sees registrations in order however long each takes.
    fn enqueue(
        &mut self,
        cx: &mut Context<Self>,
        change: impl AsyncFnOnce(WeakEntity<Self>, &mut AsyncApp) + 'static,
    ) {
        let earlier = std::mem::replace(&mut self.pending, Task::ready(()));

        self.pending = cx.spawn(async move |this, cx| {
            earlier.await;
            change(this, cx).await;
        });
    }

    fn current_registration_key(dictation: &[DictationHotkey]) -> Option<String> {
        let toggle = dictation
            .iter()
            .find(|hotkey| hotkey.role == HotkeyRole::Toggle)?;
        let hold = dictation
            .iter()
            .find(|hotkey| hotkey.role == HotkeyRole::Hold)
            .map(|hotkey| hotkey.hotkey.as_str());
        Some(registration_key(&toggle.hotkey, hold))
    }

    async fn replace_dictation(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        toggle: String,
        hold: Option<String>,
        key: String,
        force: bool,
        generation: u64,
    ) -> Result<()> {
        let mut wanted = vec![(toggle, HotkeyRole::Toggle)];
        if let Some(hold) = hold
            && !same_shortcut(&hold, &wanted[0].0)
        {
            wanted.push((hold, HotkeyRole::Hold));
        }

        let Some((removed, to_register)) = this.update(cx, |this, _| {
            if this.generation != generation {
                return None;
            }

            if force {
                let removed = std::mem::take(&mut this.dictation);
                return Some((removed, wanted));
            }

            let mut kept = Vec::new();
            let mut removed = Vec::new();
            for existing in std::mem::take(&mut this.dictation) {
                if wanted
                    .iter()
                    .any(|(hotkey, role)| existing.hotkey == *hotkey && existing.role == *role)
                {
                    kept.push(existing);
                } else {
                    removed.push(existing);
                }
            }

            // Unregistering a removed global shortcut drops any kept entry that
            // shares the same OS shortcut identity (such as different modifier
            // spelling or order), so re-register any kept entry that collides.
            let mut safe_kept = Vec::new();
            for existing in kept {
                let collides = existing.system.is_none()
                    && removed.iter().any(|other| {
                        other.system.is_none() && same_shortcut(&existing.hotkey, &other.hotkey)
                    });
                if collides {
                    removed.push(existing);
                } else {
                    safe_kept.push(existing);
                }
            }

            let to_register: Vec<_> = wanted
                .into_iter()
                .filter(|(hotkey, role)| {
                    !safe_kept
                        .iter()
                        .any(|existing| existing.hotkey == *hotkey && existing.role == *role)
                })
                .collect();

            this.dictation = safe_kept;
            Some((removed, to_register))
        })?
        else {
            return Ok(());
        };

        Self::unregister_shortcuts(cx, removed).await;

        let mut added = Vec::new();
        for (hotkey, role) in to_register {
            let system_keys = system_keys_from_hotkey(&hotkey);
            let modifier_only = system_keys.is_some();

            let registration = cx.update(|cx| {
                let hotkey = hotkey.clone();
                backend::call(cx, move |backend| {
                    if modifier_only {
                        backend.start_system_key_watcher()
                    } else {
                        // Whatever still holds the combination is replaced.
                        let _ = backend.unregister_shortcut(&hotkey);
                        backend.register_shortcut(&hotkey)
                    }
                })
            });

            if let Err(error) = registration.await {
                log::error!(target: "voice", "hotkey_registration_failed hotkey={hotkey} error={error}");
                Self::unregister_shortcuts(cx, added).await;
                this.update(cx, |this, _| {
                    if this.generation == generation {
                        this.registered = Self::current_registration_key(&this.dictation);
                    }
                })
                .ok();
                return Ok(());
            }

            added.push(DictationHotkey {
                hotkey,
                role,
                system: system_keys.map(SystemHotkey::new),
                stability_timer: None,
            });
        }

        let stale = this.update(cx, |this, _| {
            if this.generation != generation {
                return Some(added);
            }
            this.dictation.extend(added);
            this.registered = Some(key);
            None
        })?;
        if let Some(stale) = stale {
            Self::unregister_shortcuts(cx, stale).await;
        }
        Ok(())
    }

    /// Stops listening to the dictation hotkeys.
    async fn release_dictation(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        let dictation = this.update(cx, |this, _| std::mem::take(&mut this.dictation))?;
        Self::unregister_shortcuts(cx, dictation).await;

        Ok(())
    }

    async fn unregister_shortcuts(cx: &mut AsyncApp, dictation: Vec<DictationHotkey>) {
        for hotkey in dictation {
            if hotkey.system.is_some() {
                continue;
            }

            let unregistration = cx.update(|cx| {
                backend::call(cx, move |backend| {
                    backend.unregister_shortcut(&hotkey.hotkey)
                })
            });
            let _ = unregistration.await;
        }
    }

    fn handle_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::GlobalShortcut { shortcut, state } => {
                if shortcut == ESCAPE {
                    if self.escape && *state == ShortcutState::Pressed {
                        cx.emit(HotkeyEvent::Escape);
                    }
                    return;
                }

                let roles = self
                    .dictation
                    .iter()
                    .filter(|hotkey| hotkey.system.is_none() && hotkey.hotkey == *shortcut)
                    .map(|hotkey| hotkey.role);
                for role in roles {
                    cx.emit(match state {
                        ShortcutState::Pressed => HotkeyEvent::Pressed(role),
                        ShortcutState::Released => HotkeyEvent::Released(role),
                    });
                }
            }
            AppEvent::SystemKeyPressed { pressed_keys, .. } => {
                self.react(|hotkey| hotkey.key_pressed(pressed_keys), cx);
            }
            AppEvent::SystemKeyReleased { pressed_keys, .. } => {
                self.react(|hotkey| hotkey.key_released(pressed_keys), cx);
            }
            AppEvent::SystemKeysReleased => self.react(SystemHotkey::all_keys_released, cx),
            AppEvent::HotkeyRecorderActive { active } => {
                self.react(|hotkey| hotkey.set_recorder_active(*active), cx);
            }
            _ => {}
        }
    }

    /// Passes a key event to every modifier-only hotkey and acts on what it
    /// means for each.
    fn react(
        &mut self,
        mut event: impl FnMut(&mut SystemHotkey) -> Reaction,
        cx: &mut Context<Self>,
    ) {
        for index in 0..self.dictation.len() {
            let hotkey = &mut self.dictation[index];
            let role = hotkey.role;
            let Some(system) = hotkey.system.as_mut() else {
                continue;
            };

            match event(system) {
                Reaction::Ignored => {}
                Reaction::AwaitStability => {
                    hotkey.stability_timer = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(MATCH_STABILITY).await;
                        this.update(cx, |this, cx| this.stabilized(role, cx)).ok();
                    }));
                }
                Reaction::Cancelled { released } => {
                    hotkey.stability_timer = None;
                    if released {
                        cx.emit(HotkeyEvent::Released(role));
                    }
                }
            }
        }
    }

    fn stabilized(&mut self, role: HotkeyRole, cx: &mut Context<Self>) {
        let Some(hotkey) = self
            .dictation
            .iter_mut()
            .find(|hotkey| hotkey.role == role)
        else {
            return;
        };

        if hotkey.system.as_mut().is_some_and(SystemHotkey::stabilized) {
            cx.emit(HotkeyEvent::Pressed(role));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SystemKey::{LeftControl, LeftOption, LeftShift, RightCommand};

    fn released(released: bool) -> Reaction {
        Reaction::Cancelled { released }
    }

    #[test]
    fn a_match_fires_only_after_the_stability_window() {
        let mut hotkey = SystemHotkey::new(vec![RightCommand]);

        assert_eq!(
            hotkey.key_pressed(&[RightCommand]),
            Reaction::AwaitStability
        );
        assert!(!hotkey.held);
        assert!(hotkey.stabilized());
        assert!(hotkey.held);
    }

    #[test]
    fn a_chord_matches_whatever_the_order_of_its_keys() {
        let mut hotkey = SystemHotkey::new(vec![LeftControl, LeftOption]);

        assert_eq!(hotkey.key_pressed(&[LeftControl]), released(false));
        assert_eq!(
            hotkey.key_pressed(&[LeftOption, LeftControl]),
            Reaction::AwaitStability
        );
        assert!(hotkey.stabilized());
    }

    #[test]
    fn a_match_passed_through_on_the_way_to_a_larger_chord_never_fires() {
        let mut hotkey = SystemHotkey::new(vec![LeftControl, LeftOption]);

        assert_eq!(
            hotkey.key_pressed(&[LeftControl, LeftOption]),
            Reaction::AwaitStability
        );
        assert_eq!(
            hotkey.key_pressed(&[LeftControl, LeftOption, LeftShift]),
            released(false)
        );
        assert!(!hotkey.stabilized());
        assert!(!hotkey.held);
    }

    #[test]
    fn releasing_before_the_window_ends_cancels_the_press() {
        let mut hotkey = SystemHotkey::new(vec![RightCommand]);
        hotkey.key_pressed(&[RightCommand]);

        assert_eq!(hotkey.key_released(&[]), released(false));
        assert!(!hotkey.stabilized());
    }

    #[test]
    fn a_repeated_match_restarts_the_window_until_it_fires() {
        let mut hotkey = SystemHotkey::new(vec![RightCommand]);

        assert_eq!(
            hotkey.key_pressed(&[RightCommand]),
            Reaction::AwaitStability
        );
        assert_eq!(
            hotkey.key_pressed(&[RightCommand]),
            Reaction::AwaitStability
        );
        assert!(hotkey.stabilized());
        assert_eq!(hotkey.key_pressed(&[RightCommand]), Reaction::Ignored);
        assert!(!hotkey.stabilized());
    }

    #[test]
    fn a_held_hotkey_is_released_when_its_keys_change() {
        let mut hotkey = SystemHotkey::new(vec![RightCommand]);
        hotkey.key_pressed(&[RightCommand]);
        hotkey.stabilized();

        assert_eq!(hotkey.key_released(&[]), released(true));
        assert_eq!(hotkey.key_released(&[]), released(false));

        hotkey.key_pressed(&[RightCommand]);
        hotkey.stabilized();
        assert_eq!(
            hotkey.key_pressed(&[RightCommand, LeftShift]),
            released(true)
        );

        hotkey.key_released(&[]);
        hotkey.key_pressed(&[RightCommand]);
        hotkey.stabilized();
        assert_eq!(hotkey.all_keys_released(), released(true));
    }

    #[test]
    fn a_release_that_leaves_the_hotkey_held_changes_nothing() {
        let mut hotkey = SystemHotkey::new(vec![RightCommand]);
        hotkey.key_pressed(&[RightCommand, LeftShift]);
        hotkey.key_pressed(&[RightCommand]);
        hotkey.stabilized();

        assert_eq!(hotkey.key_released(&[RightCommand]), Reaction::Ignored);
        assert!(hotkey.held);
    }

    #[test]
    fn the_hotkey_recorder_suppresses_presses() {
        let mut hotkey = SystemHotkey::new(vec![RightCommand]);
        hotkey.key_pressed(&[RightCommand]);

        assert_eq!(hotkey.set_recorder_active(true), released(false));
        assert!(!hotkey.stabilized());
        assert_eq!(hotkey.key_pressed(&[RightCommand]), Reaction::Ignored);
        assert!(!hotkey.stabilized());

        assert_eq!(hotkey.set_recorder_active(false), Reaction::Ignored);
        hotkey.key_released(&[]);
        assert_eq!(
            hotkey.key_pressed(&[RightCommand]),
            Reaction::AwaitStability
        );
        assert!(hotkey.stabilized());
    }

    #[test]
    fn the_recorder_does_not_release_a_hotkey_already_held() {
        let mut hotkey = SystemHotkey::new(vec![RightCommand]);
        hotkey.key_pressed(&[RightCommand]);
        hotkey.stabilized();

        assert_eq!(hotkey.set_recorder_active(true), released(false));
        assert!(hotkey.held);
        assert_eq!(hotkey.key_released(&[]), released(true));
    }

    #[test]
    fn hotkeys_that_cannot_be_registered_fall_back_to_the_defaults() {
        let settings = Settings {
            hotkey: "K".into(),
            hold_to_speak_hotkey: Some("Alt+Space".into()),
            ..Settings::default()
        };

        assert_eq!(
            dictation_hotkeys(&settings),
            (DEFAULT_HOTKEY.to_string(), Some("Alt+Space".to_string()))
        );
    }

    #[test]
    fn hotkeys_are_registered_when_they_change_and_dictation_is_live() {
        let register = HotkeyPlan::Register {
            toggle: "A".into(),
            hold: Some("B".into()),
        };

        assert_eq!(hotkey_plan(true, "A", Some("B"), None, false), register);
        assert_eq!(
            hotkey_plan(true, "A", Some("B"), Some("A|C"), false),
            register
        );
        assert_eq!(
            hotkey_plan(true, "A", Some("B"), Some("A|B"), false),
            HotkeyPlan::Keep
        );
        assert_eq!(
            hotkey_plan(true, "A", Some("B"), Some("A|B"), true),
            register
        );
        assert_eq!(
            hotkey_plan(false, "A", Some("B"), Some("A|B"), true),
            HotkeyPlan::Unregister
        );
        assert_eq!(
            hotkey_plan(false, "A", Some("B"), None, false),
            HotkeyPlan::Unregister
        );
    }
}
