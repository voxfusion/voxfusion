//! The settings: a modal over the main window, with the list of sections on
//! the left and the chosen section on the right.

mod appearance;
mod audio;
mod hotkey;
mod language;
mod model;
mod privacy;
mod sidebar;

#[cfg(feature = "fixture")]
pub use sidebar::set_app_version;

use gpui_kit::{
    App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Global,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, deferred, div, px,
};

use crate::settings::SettingsStore;
use crate::ui::hotkey_recorder::{HotkeyKind, HotkeyRecorder, route_keys};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;

use crate::ui::motion::Transitions as _;
use audio::AudioState;
use model::ModelSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Section {
    #[default]
    Audio,
    Model,
    Hotkey,
    Appearance,
    Language,
    Privacy,
}

impl Section {
    const ALL: [Section; 6] = [
        Section::Audio,
        Section::Model,
        Section::Hotkey,
        Section::Appearance,
        Section::Language,
        Section::Privacy,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::Audio => "// AUDIO_CONFIG",
            Section::Model => "// MODEL_CONFIG",
            Section::Hotkey => "// HOTKEY_CONFIG",
            Section::Appearance => "// APPEARANCE_CONFIG",
            Section::Language => "// LANGUAGE_CONFIG",
            Section::Privacy => "// PRIVACY_CONFIG",
        }
    }
}

/// The section shown when the settings were last open. They open on it again.
struct LastSection(Section);

impl Global for LastSection {}

pub struct SettingsModal {
    section: Section,
    audio: AudioState,
    /// Exists while the Models section is shown, so the section starts
    /// afresh every time.
    model: Option<Entity<ModelSettings>>,
    hands_free_recorder: Entity<HotkeyRecorder>,
    hold_to_speak_recorder: Entity<HotkeyRecorder>,
    version_copied: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for SettingsModal {}

impl SettingsModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hands_free_recorder = cx.new(|cx| HotkeyRecorder::new(HotkeyKind::HandsFree, cx));
        let hold_to_speak_recorder = cx.new(|cx| HotkeyRecorder::new(HotkeyKind::HoldToSpeak, cx));

        let subscriptions = vec![
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
            cx.observe(&hands_free_recorder, |_, _, cx| cx.notify()),
            cx.observe(&hold_to_speak_recorder, |_, _, cx| cx.notify()),
        ];

        // Keys go to the modal for as long as it is open.
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        let section = cx
            .try_global::<LastSection>()
            .map_or(Section::default(), |last| last.0);

        let mut modal = Self {
            section,
            audio: AudioState::default(),
            model: (section == Section::Model).then(|| cx.new(ModelSettings::new)),
            hands_free_recorder,
            hold_to_speak_recorder,
            version_copied: false,
            focus_handle,
            _subscriptions: subscriptions,
        };
        modal.fetch_audio_devices(cx);
        modal
    }

    fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if section == self.section {
            return;
        }

        self.section = section;
        cx.set_global(LastSection(section));

        self.audio.stop_refresh_animation();
        self.model = (section == Section::Model).then(|| cx.new(ModelSettings::new));
        cx.notify();
    }

    fn is_recording_hotkey(&self, cx: &App) -> bool {
        self.hands_free_recorder.read(cx).is_recording()
            || self.hold_to_speak_recorder.read(cx).is_recording()
    }

    fn render_section(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.section {
            Section::Audio => self.render_audio(window, cx).into_any_element(),
            Section::Model => div().children(self.model.clone()).into_any_element(),
            Section::Hotkey => self.render_hotkeys(cx).into_any_element(),
            Section::Appearance => appearance::render(cx).into_any_element(),
            Section::Language => language::render(window, cx).into_any_element(),
            Section::Privacy => privacy::render(cx).into_any_element(),
        }
    }
}

impl Render for SettingsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px_6()
            .py_4()
            .border_b_1()
            .border_color(p.border)
            .type_sm()
            .child(
                div()
                    .text_color(p.txt_primary)
                    .child(text(self.section.title()).tracking_wider()),
            )
            .child(
                div()
                    .id("close")
                    .transition_colors()
                    .text_color(p.txt_muted)
                    .hover(|button| button.text_color(p.ac))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
                    .child(text("[X]")),
            );

        let footer = div()
            .px_6()
            .py_3()
            .border_t_1()
            .border_color(p.border)
            .type_px(10.)
            .text_color(p.txt_faint)
            .child(text("ESC TO CLOSE | CHANGES SAVED AUTOMATICALLY"));

        let main = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("section")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_6()
                    .child(self.render_section(window, cx)),
            )
            .child(footer);

        // Clicks inside the box must not count as clicks on the overlay.
        let modal = div()
            .occlude()
            .w(px(800.))
            .h(px(600.))
            .bg(p.base)
            .border_1()
            .border_color(p.border)
            .flex()
            .overflow_hidden()
            .font_mono()
            .child(self.render_sidebar(cx))
            .child(main);

        let recorders = [
            self.hands_free_recorder.clone(),
            self.hold_to_speak_recorder.clone(),
        ];
        let overlay = route_keys(div().id("settings-modal"), &recorders)
            .track_focus(&self.focus_handle)
            // A recorder takes Escape like any other key.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && !this.is_recording_hotkey(cx) {
                    cx.emit(DismissEvent);
                }
            }))
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .absolute()
            .inset_0()
            .occlude()
            .bg(p.overlay)
            .flex()
            .items_center()
            .justify_center()
            .child(modal);

        // Above everything in the window, including a list a page has open.
        deferred(overlay).with_priority(1)
    }
}
