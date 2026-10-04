//! The overlay: a small pill along the bottom of the screen that shows the
//! dictation in progress. It never takes focus from the app dictated into.

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, AppContext as _, Bounds, Context, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, Result,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TextRun, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, prelude::*,
    px, size,
};
use std::time::{Duration, Instant};

use crate::paths::APP_IDENTIFIER;
use crate::platform::overlay_window::{self, OverlayWindow, ScreenRect};
use crate::settings::SettingsStore;
use crate::ui::t_upper;
use crate::ui::text::{TRACKING_WIDER, TypeScale as _, text};
use crate::ui::theme::{self, mono_font, mono_font_fallbacks, palette};
use crate::ui::widgets::{animations_frozen, icon};

use super::bars::{HEIGHT_TRANSITION, NUM_BARS, bars};
use super::controller::{VoiceController, WINDOW_WIDTH_COMPACT, WindowRequest};
use super::error_pill::{self, ErrorPillLayout, error_pill_layout};
use super::position::{WINDOW_HEIGHT, display_for_cursor, frame_correction, overlay_frame};
use super::spinner::dot_matrix_spinner;
use super::transition::{Transition, ease_out};
use crate::ui::motion::{Faded, Transitions as _};

/// How often the window's place on screen is checked: the cursor may have
/// moved to another display, or the displays may have changed.
const PLACEMENT_CHECK: Duration = Duration::from_secs(1);

const ERROR_FONT_SIZE: f32 = 10.;

/// The room the spinner takes when it is showing: its 16px and the gap
/// before it.
const SPINNER_SLOT_WIDTH: f32 = 24.;
const SPINNER_SLOT_TRANSITION: Duration = Duration::from_millis(300);

/// The window as it was last arranged, and the means to arrange it.
struct Placement {
    /// Missing where the platform gives no control over the window.
    native: Option<OverlayWindow>,
    applied: WindowRequest,
}

pub struct Overlay {
    controller: Entity<VoiceController>,
    bars: Transition<NUM_BARS>,
    bars_shown: bool,
    spinner_slot: Transition<1>,
    /// Missing when the window is to be left as it was opened.
    placement: Option<Placement>,
    _placement_check: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Overlay {
    fn new(
        controller: Entity<VoiceController>,
        placement: Option<Placement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe_in(&controller, window, |this, _, window, cx| {
                this.arrange_window(window, cx);
                cx.notify();
            }),
            // The language of the retry button may have changed.
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
        ];

        let placement_check = placement.is_some().then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(PLACEMENT_CHECK).await;

                    if this.update(cx, |this, _| this.reposition()).is_err() {
                        break;
                    }
                }
            })
        });

        let overlay = Self {
            bars: Transition::at(controller.read(cx).bar_heights(), HEIGHT_TRANSITION),
            bars_shown: false,
            spinner_slot: Transition::at([0.], SPINNER_SLOT_TRANSITION),
            controller,
            placement,
            _placement_check: placement_check,
            _subscriptions: subscriptions,
        };
        overlay.reposition();
        overlay
    }

    /// Gives the window the size and visibility the controller asks for.
    fn arrange_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let request = self.controller.read(cx).window_request();
        let Some(placement) = self.placement.as_mut() else {
            return;
        };
        if request == placement.applied {
            return;
        }

        let resized = request.width != placement.applied.width;
        let shown = request.visible && !placement.applied.visible;
        let hidden = !request.visible && placement.applied.visible;
        placement.applied = request;

        if resized && placement.native.is_none() {
            window.resize(size(px(request.width), px(WINDOW_HEIGHT as f32)));
        }
        if resized || shown {
            self.reposition();
        }

        let Some(native) = self.placement.as_ref().and_then(|it| it.native.as_ref()) else {
            return;
        };
        if shown {
            native.show();
        }
        if hidden {
            // A hidden window draws nothing, and shows its last frame again
            // when it comes back. It is hidden only once the frame without
            // the pill is on screen: the next frame draws it, the one after
            // follows it.
            let this = cx.weak_entity();
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |_, cx| {
                    this.update(cx, |this, _| this.hide_unless_wanted()).ok();
                });
            });
        }
    }

    fn hide_unless_wanted(&self) {
        let Some(placement) = &self.placement else {
            return;
        };

        if let Some(native) = placement
            .native
            .as_ref()
            .filter(|_| !placement.applied.visible)
        {
            native.hide();
        }
    }

    /// Centers the window along the bottom of the display the cursor is on,
    /// at its current width.
    fn reposition(&self) {
        let Some(placement) = &self.placement else {
            return;
        };
        let Some(native) = &placement.native else {
            return;
        };
        let display = display_for_cursor(
            overlay_window::cursor_position(),
            &overlay_window::displays(),
        );
        let (Some(display), Some(current)) = (display, native.frame()) else {
            return;
        };

        let width = placement.applied.width as f64;
        if let Some(frame) = frame_correction(display, width, current, native.scale_factor()) {
            native.set_frame(frame);
        }
    }

    fn round_button(
        id: &'static str,
        icon_name: &'static str,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id(id)
            .size(px(error_pill::BUTTON_SIZE))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .child(icon(icon_name).size(px(10.)))
    }

    /// A button in the neutral colors: it cancels or dismisses.
    fn quiet_button(id: &'static str, cx: &App) -> Faded {
        let p = palette(cx);

        Self::round_button(id, "x")
            .transition_colors()
            .bg(p.elevated)
            .text_color(p.txt_secondary)
            .hover(|button| button.bg(p.hover).text_color(p.txt_primary))
    }

    fn render_bars(&self, color: Hsla) -> AnyElement {
        if animations_frozen() {
            return bars(self.bars.target(), color).into_any_element();
        }

        let transition = self.bars.clone();
        div()
            .flex()
            .with_animation(
                ("voice-bars", transition.run()),
                Animation::new(transition.duration()).with_easing(ease_out),
                move |row, eased| row.child(bars(transition.values(eased), color)),
            )
            .into_any_element()
    }

    /// The width of `content` in the error pill's type, each glyph followed
    /// by `tracking` em of space.
    fn text_width(content: &str, tracking: f32, window: &mut Window) -> f32 {
        let mut style = window.text_style();
        style.font_family = mono_font();
        style.font_fallbacks = mono_font_fallbacks();

        let run = TextRun {
            len: content.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .layout_line(content, px(ERROR_FONT_SIZE), &[run], None);
        let glyphs: usize = line.runs.iter().map(|run| run.glyphs.len()).sum();

        line.width.as_f32() + glyphs as f32 * tracking * ERROR_FONT_SIZE
    }

    /// The error's message, its retry button if it has one, and the button
    /// that dismisses it, with the pill's distance from the left edge.
    fn render_error(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(impl IntoElement + use<>, f32)> {
        let p = palette(cx);
        let error = self.controller.read(cx).error()?;
        let message = error.message.clone();
        let retry_label = error
            .retry
            .is_some()
            .then(|| t_upper(cx, "voiceControl.retry"));

        let ErrorPillLayout {
            left,
            message_width,
            retry_width,
        } = error_pill_layout(
            window.viewport_size().width.as_f32(),
            Self::text_width(&message, 0., window),
            retry_label
                .as_ref()
                .map(|label| Self::text_width(label, TRACKING_WIDER, window)),
            window.scale_factor(),
        );

        let retry = retry_label.zip(retry_width).map(|(label, width)| {
            div()
                .id("retry")
                .transition_colors()
                .flex_none()
                .w(px(width))
                .px(px(error_pill::RETRY_PADDING))
                .border_1()
                .border_color(p.ac)
                .hover(|button| button.bg(p.ac).text_color(p.ac_on))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.controller
                        .update(cx, |controller, cx| controller.retry_transcription(cx));
                }))
                .child(text(label).tracking_wider().nowrap())
        });

        let dismiss = Self::quiet_button("dismiss", cx).on_click(cx.listener(|this, _, _, cx| {
            this.controller
                .update(cx, |controller, cx| controller.dismiss_error(cx));
        }));

        let row = div()
            .self_center()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(error_pill::GAP))
            .font_mono()
            .type_px(ERROR_FONT_SIZE)
            .text_color(p.ac)
            .child(icon("alert-circle").size(px(error_pill::ICON_SIZE)))
            .child(
                div()
                    .flex_none()
                    .w(px(message_width))
                    .child(text(message).nowrap()),
            )
            .children(retry)
            .child(dismiss);

        Some((row, left))
    }

    /// The spinner in a slot that opens and closes around it.
    fn render_spinner_slot(&self, color: Hsla) -> AnyElement {
        let slot = div()
            .flex()
            .flex_none()
            .items_center()
            .overflow_hidden()
            .child(div().ml_2().child(dot_matrix_spinner(16., 3., color)));

        if animations_frozen() {
            return slot
                .max_w(px(self.spinner_slot.target()[0]))
                .into_any_element();
        }

        let transition = self.spinner_slot.clone();
        slot.with_animation(
            ("voice-spinner-slot", transition.run()),
            Animation::new(transition.duration()).with_easing(ease_out),
            move |slot, eased| slot.max_w(px(transition.values(eased)[0])),
        )
        .into_any_element()
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let controller = self.controller.read(cx);
        let pill = controller.pill();
        let heights = controller.bar_heights();
        let now = Instant::now();

        // Bars appear at their heights and move from there.
        if pill.bars && !self.bars_shown {
            self.bars.jump_to(heights);
        } else {
            self.bars.retarget(heights, now);
        }
        self.bars_shown = pill.bars;

        let slot_width = if pill.spinner { SPINNER_SLOT_WIDTH } else { 0. };
        self.spinner_slot.retarget([slot_width], now);
        let slot_open = pill.spinner || !self.spinner_slot.at_rest(now);

        let root = div().size_full().flex();
        if !pill.visible {
            return root;
        }

        let cancel = pill.buttons.then(|| {
            Self::quiet_button("cancel", cx)
                .self_center()
                .mr_1()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.controller
                        .update(cx, |controller, cx| controller.cancel_recording(cx));
                }))
        });

        let confirm = pill.buttons.then(|| {
            Self::round_button("confirm", "check")
                .transition_colors()
                .self_center()
                .ml_1()
                .bg(p.ac)
                .text_color(p.ac_on)
                .hover(|button| button.bg(p.ac_hover))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.controller
                        .update(cx, |controller, cx| controller.stop_recording(cx));
                }))
        });

        let (error, error_left) = if pill.error {
            self.render_error(window, cx).unzip()
        } else {
            (None, None)
        };

        root.child(
            div()
                // As wide as its content and centered in the window.
                .map(|pill_box| match error_left {
                    Some(left) => pill_box.ml(px(left)),
                    None => pill_box.mx_auto(),
                })
                .flex_none()
                .flex()
                .justify_center()
                .py_1()
                .px(px(if pill.narrow {
                    error_pill::PADDING
                } else {
                    2. * error_pill::PADDING
                }))
                .bg(p.base)
                .border_1()
                .border_color(if pill.error { p.ac } else { p.border_strong })
                .rounded(px(16.))
                .children(cancel)
                .children(pill.bars.then(|| self.render_bars(p.ac)))
                .children(error)
                .children(confirm)
                .children(slot_open.then(|| self.render_spinner_slot(p.ac))),
        )
    }
}

fn window_options(bounds: Bounds<Pixels>, show: bool) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: false,
        show,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        // The whole window lies where a title bar would be. Clicks there
        // must reach the buttons at once, not wait to become a window drag.
        app_owns_titlebar_drag: true,
        // The overlay is never the active window, and its bars still move.
        inactive_frame_interval: None,
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some(APP_IDENTIFIER.into()),
        ..Default::default()
    }
}

/// Opens the overlay for `controller`.
///
/// Normally it starts hidden at the bottom of the primary display and follows
/// the controller from there. Given a `fixed_size`, it is instead shown at
/// the screen's origin at that size and left there, for screenshots.
pub fn open(
    controller: Entity<VoiceController>,
    fixed_size: Option<(f32, f32)>,
    cx: &mut App,
) -> Result<()> {
    // Only macOS can show the window later, so elsewhere it stays up and
    // draws nothing between dictations.
    let show = fixed_size.is_some() || !cfg!(target_os = "macos");

    let bounds = match fixed_size {
        Some((width, height)) => Bounds::new(point(px(0.), px(0.)), size(px(width), px(height))),
        None => {
            let display = cx
                .primary_display()
                .map(|display| display.bounds())
                .unwrap_or_default();
            let frame = overlay_frame(
                ScreenRect {
                    x: display.origin.x.as_f32() as f64,
                    y: display.origin.y.as_f32() as f64,
                    width: display.size.width.as_f32() as f64,
                    height: display.size.height.as_f32() as f64,
                },
                WINDOW_WIDTH_COMPACT as f64,
            );

            Bounds::new(
                point(px(frame.x as f32), px(frame.y as f32)),
                size(px(frame.width as f32), px(frame.height as f32)),
            )
        }
    };

    cx.open_window(window_options(bounds, show), |window, cx| {
        window.set_rem_size(px(16.));
        theme::follow(window, cx);

        let placement = fixed_size.is_none().then(|| Placement {
            native: OverlayWindow::new(&*window),
            applied: WindowRequest {
                visible: show,
                width: WINDOW_WIDTH_COMPACT,
            },
        });

        cx.new(|cx| Overlay::new(controller, placement, window, cx))
    })?;

    Ok(())
}
