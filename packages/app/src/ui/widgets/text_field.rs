//! The app's text input: one line of editable text.
//!
//! The field is only the text. It takes its font, size, line height and
//! color from the box it is placed in, and that box supplies the border,
//! padding and background, the way a styled `<input>` does:
//!
//! ```ignore
//! field_box(&field, cx)
//!     .px_4()
//!     .py_2()
//!     .border_1()
//!     .border_color(p.border_strong)
//!     .text_color(p.txt_primary)
//!     .font_mono()
//!     .type_base()
//! ```

use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render, SharedString,
    Styled, Subscription, Window, div,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFieldEvent {
    /// The user edited the text.
    Change,
    Enter,
    Escape,
    Focus,
    Blur,
}

type Placeholder = Box<dyn Fn(&App) -> SharedString>;

pub struct TextField {
    input: Entity<InputState>,
    placeholder: Option<Placeholder>,
    _subscription: Subscription,
}

impl TextField {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));

        let subscription = cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
            cx.emit(match event {
                InputEvent::Change => TextFieldEvent::Change,
                InputEvent::PressEnter { .. } => TextFieldEvent::Enter,
                InputEvent::Focus => TextFieldEvent::Focus,
                InputEvent::Blur => TextFieldEvent::Blur,
            });
        });

        Self {
            input,
            placeholder: None,
            _subscription: subscription,
        }
    }

    /// The text shown while the field is empty. It is asked for on every
    /// render, so it follows the interface language.
    pub fn placeholder(mut self, placeholder: impl Fn(&App) -> SharedString + 'static) -> Self {
        self.placeholder = Some(Box::new(placeholder));
        self
    }

    pub fn value(&self, cx: &App) -> SharedString {
        self.input.read(cx).value()
    }

    /// Replaces the text without raising [`TextFieldEvent::Change`], leaving
    /// the caret at its end.
    pub fn set_value(
        &mut self,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value: SharedString = value.into();
        self.input
            .update(cx, |input, cx| input.set_value(value, window, cx));
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }
}

impl EventEmitter<TextFieldEvent> for TextField {}

/// A box holding `field`, for the caller to style. As with the padding of an
/// `<input>`, a click anywhere in the box focuses the field.
pub fn field_box(field: &Entity<TextField>, cx: &App) -> Div {
    let focus_handle = field.focus_handle(cx);

    div()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&focus_handle, cx);
            // Keeps a focusable element around the box from taking the focus.
            window.prevent_default();
        })
        .child(field.clone())
}

impl Focusable for TextField {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(placeholder) = &self.placeholder {
            let placeholder = placeholder(cx);

            if self.input.read(cx).presentation().placeholder() != &placeholder {
                self.input.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx);
                });
            }
        }

        // The kit's input has a size, padding and line height of its own.
        // The field is exactly one line of the surrounding text style.
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let input = Input::new(&self.input)
            .appearance(false)
            .p_0()
            .gap_0()
            .text_size(font_size)
            .line_height(line_height);

        div()
            .w_full()
            // The input lets Escape through without reporting it.
            .on_action(cx.listener(|_, _: &Escape, _, cx| {
                cx.emit(TextFieldEvent::Escape);
                cx.propagate();
            }))
            // `Input::h` is the height of a multi-line input, not the style.
            .child(Styled::h(input, line_height))
    }
}
