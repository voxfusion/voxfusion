//! The CSS transitions of the design, on gpui-kit's value transitions: a
//! color fades to its new value, a switch slides to its new place.
//!
//! The kit moves a value towards a target that is known while rendering.
//! Where the pointer is only shows once an element is placed, so the elements
//! here ask for the transition at that point.

use gpui_kit::base::motion::{
    Easing, Interpolate, MotionStatus, Transition, transition, transition_with_status,
};
use gpui_kit::{
    AnyElement, App, Bounds, Div, Element, ElementId, Fill, FocusHandle, GlobalElementId, Hitbox,
    Hsla, InspectorElementId, InteractiveElement, Interactivity, IntoElement, LayoutId,
    ParentElement, Pixels, Rgba, SharedString, Stateful, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, div,
};
use std::time::Duration;

/// How long a transition takes unless the design says otherwise.
pub const DURATION: Duration = Duration::from_millis(150);

/// The design's default timing function, `cubic-bezier(0.4, 0, 0.2, 1)`.
const EASING: Easing = Easing::CubicBezier {
    x1: 0.4,
    y1: 0.,
    x2: 0.2,
    y2: 1.,
};

/// The default timing function's value at `progress`.
pub fn ease(progress: f32) -> f32 {
    EASING.sample(progress)
}

fn policy(duration: Duration) -> Transition {
    Transition::new(duration).easing(EASING)
}

/// A color as CSS fades it: with its channels multiplied by its alpha, so a
/// color coming out of transparency keeps its hue.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Premultiplied([f32; 4]);

impl From<Hsla> for Premultiplied {
    fn from(color: Hsla) -> Self {
        let Rgba { r, g, b, a } = color.to_rgb();
        Self([r * a, g * a, b * a, a])
    }
}

impl From<Premultiplied> for Hsla {
    fn from(Premultiplied([r, g, b, a]): Premultiplied) -> Self {
        if a <= 0. {
            return Hsla::default();
        }

        Rgba {
            r: r / a,
            g: g / a,
            b: b / a,
            a,
        }
        .into()
    }
}

impl Interpolate for Premultiplied {
    fn interpolate(&self, target: &Self, progress: f32) -> Self {
        Self(std::array::from_fn(|channel| {
            self.0[channel] + (target.0[channel] - self.0[channel]) * progress
        }))
    }
}

/// The properties a [`Faded`] element animates.
#[derive(Debug, Clone, PartialEq, Default)]
struct Look {
    background: Option<Fill>,
    text: Option<Hsla>,
    border: Option<Hsla>,
    opacity: Option<f32>,
}

impl Look {
    fn of(style: &StyleRefinement) -> Self {
        Self {
            background: style.background.clone(),
            text: style.text.color,
            border: style.border_color,
            opacity: style.opacity,
        }
    }

    fn write(&self, style: &mut StyleRefinement) {
        style.background = self.background.clone();
        style.text.color = self.text;
        style.border_color = self.border;
        style.opacity = self.opacity;
    }
}

/// The value of `channel` on its way to `target`, or `None` once it rests
/// there. With `duration` zero it does not move at all.
fn moving<T>(
    channel: &'static str,
    target: T,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Option<T>
where
    T: Interpolate + PartialEq + 'static,
{
    let motion = transition_with_status(channel, target, policy(duration), window, cx);

    matches!(motion.status, MotionStatus::Delayed | MotionStatus::Running).then_some(motion.value)
}

/// What a [`Faded`] element last looked like, for the next frame's layout.
#[derive(Default)]
struct LastLook(Option<Look>);

/// An element whose colors fade to their new values, whether the pointer
/// brought the change or a new render did.
pub struct Faded {
    element: Stateful<Div>,
    /// Holds the hover, group hover and focus styles. It is never drawn, only
    /// asked what the element should look like; the element itself is given
    /// the colors of the moment.
    states: Div,
    /// The element's own colors, before any of the moment were written to it.
    base: Option<Look>,
    colors: Duration,
    opacity: Duration,
}

pub trait Transitions {
    /// `transition-colors`: the background, text and border colors fade.
    /// Call it before `hover`, `group_hover` and `focus`, which then fade too.
    fn transition_colors(self) -> Faded;

    /// `transition-opacity`.
    fn transition_opacity(self) -> Faded;

    /// `transition-all`: the colors and the opacity fade.
    fn transition_all(self) -> Faded;
}

impl Transitions for Stateful<Div> {
    fn transition_colors(self) -> Faded {
        Faded::new(self, DURATION, Duration::ZERO)
    }

    fn transition_opacity(self) -> Faded {
        Faded::new(self, Duration::ZERO, DURATION)
    }

    fn transition_all(self) -> Faded {
        Faded::new(self, DURATION, DURATION)
    }
}

impl Faded {
    fn new(element: Stateful<Div>, colors: Duration, opacity: Duration) -> Self {
        Self {
            element,
            states: div(),
            base: None,
            colors,
            opacity,
        }
    }

    pub fn hover(mut self, style: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        // The empty style makes GPUI draw again when the pointer comes or
        // goes.
        self.element = self.element.hover(|style| style);
        self.states = self.states.hover(style);
        self
    }

    pub fn group_hover(
        mut self,
        group: impl Into<SharedString>,
        style: impl FnOnce(StyleRefinement) -> StyleRefinement,
    ) -> Self {
        let group = group.into();
        self.element = self.element.group_hover(group.clone(), |style| style);
        self.states = self.states.group_hover(group, style);
        self
    }

    pub fn track_focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.element = self.element.track_focus(focus_handle);
        self.states = self.states.track_focus(focus_handle);
        self
    }

    pub fn focus(mut self, style: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.states = self.states.focus(style);
        self
    }

    /// What the element should look like now, going by where the pointer and
    /// the focus are.
    fn target(&mut self, hitbox: Option<&Hitbox>, window: &mut Window, cx: &mut App) -> Look {
        let base = self
            .base
            .get_or_insert_with(|| Look::of(self.element.style()));

        let mut style = self.element.interactivity().base_style.clone();
        base.write(&mut style);
        self.states.interactivity().base_style = style;

        let style = self
            .states
            .interactivity()
            .compute_style(None, hitbox, window, cx);

        Look {
            background: style.background,
            text: style.text.color,
            border: style.border_color,
            opacity: style.opacity,
        }
    }

    /// The look of the moment on the way to `target`. A property at rest is
    /// exactly its target.
    fn shown(&self, target: Look, window: &mut Window, cx: &mut App) -> Look {
        let color = |color: Option<Hsla>| Premultiplied::from(color.unwrap_or_default());
        let solid = target
            .background
            .as_ref()
            .and_then(|fill| fill.color())
            .and_then(|background| background.as_solid());
        // Text without a color of its own has the surrounding one.
        let text = target.text.unwrap_or_else(|| window.text_style().color);

        let mut shown = target.clone();

        if let Some(value) = moving("background-color", color(solid), self.colors, window, cx) {
            shown.background = Some(Hsla::from(value).into());
        }
        if let Some(value) = moving("color", color(Some(text)), self.colors, window, cx) {
            shown.text = Some(value.into());
        }
        if let Some(value) = moving(
            "border-color",
            color(target.border),
            self.colors,
            window,
            cx,
        ) {
            shown.border = Some(value.into());
        }
        if let Some(value) = moving(
            "opacity",
            target.opacity.unwrap_or(1.),
            self.opacity,
            window,
            cx,
        ) {
            shown.opacity = Some(value);
        }

        shown
    }
}

impl Styled for Faded {
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl InteractiveElement for Faded {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.element.interactivity()
    }
}

impl StatefulInteractiveElement for Faded {}

impl ParentElement for Faded {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.element.extend(elements);
    }
}

impl IntoElement for Faded {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Faded {
    type RequestLayoutState = <Stateful<Div> as Element>::RequestLayoutState;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Element::id(&self.element)
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        Element::source_location(&self.element)
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.base = Some(Look::of(self.element.style()));

        // Until the element is placed it looks as it last did, for children
        // that take their color while they are laid out.
        if let Some(id) = id {
            let last = window.with_element_state(id, |last: Option<LastLook>, _| {
                let last = last.unwrap_or_default();
                (last.0.clone(), last)
            });

            if let Some(last) = last {
                last.write(self.element.style());
            }
        }

        self.element.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        self.element
            .prepaint(id, inspector_id, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(id) = id {
            let target = self.target(hitbox.as_ref(), window, cx);
            let shown = self.shown(target, window, cx);

            shown.write(self.element.style());
            window.with_element_state(id, |_: Option<LastLook>, _| ((), LastLook(Some(shown))));
        }

        self.element
            .paint(id, inspector_id, bounds, layout, hitbox, window, cx);
    }
}

/// An element drawn from a number that glides to each new `target`, like a
/// CSS transition of a transform. `build` gets the number of the moment.
pub fn gliding<E: IntoElement>(
    id: impl Into<ElementId>,
    target: f32,
    build: impl FnOnce(f32) -> E + 'static,
) -> Glided {
    Glided {
        id: id.into(),
        target,
        build: Some(Box::new(move |value| build(value).into_any_element())),
    }
}

pub struct Glided {
    id: ElementId,
    target: f32,
    build: Option<Box<dyn FnOnce(f32) -> AnyElement>>,
}

impl IntoElement for Glided {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Glided {
    type RequestLayoutState = Option<AnyElement>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Option<AnyElement>) {
        let value = transition("value", self.target, policy(DURATION), window, cx);

        let mut element = self.build.take().map(|build| build(value));
        let layout = match element.as_mut() {
            Some(element) => element.request_layout(window, cx),
            None => window.request_layout(Default::default(), [], cx),
        };

        (layout, element)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        element: &mut Option<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = element {
            element.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        element: &mut Option<AnyElement>,
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = element {
            element.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::rgb;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn the_timing_function_starts_fast_and_settles_slowly() {
        assert!(ease(0.).abs() < 1e-4);
        assert!((ease(1.) - 1.).abs() < 1e-4);

        // Values of cubic-bezier(0.4, 0, 0.2, 1) as browsers compute it.
        assert!(close(ease(0.25), 0.2366));
        assert!(close(ease(0.5), 0.7756));
        assert!(close(ease(0.75), 0.9594));
    }

    #[test]
    fn a_color_fades_out_of_transparency_without_darkening() {
        let color: Hsla = rgb(0xff4400).into();
        let transparent = Premultiplied::from(Hsla::default());

        let halfway: Hsla = transparent.interpolate(&color.into(), 0.5).into();
        let (halfway, full) = (halfway.to_rgb(), color.to_rgb());

        assert!(close(halfway.r, full.r));
        assert!(close(halfway.g, full.g));
        assert!(close(halfway.b, full.b));
        assert!(close(halfway.a, 0.5));
    }

    #[test]
    fn a_color_survives_the_round_trip() {
        let color: Hsla = rgb(0x2a6f97).into();
        let back = Hsla::from(Premultiplied::from(color)).to_rgb();
        let color = color.to_rgb();

        assert!(close(back.r, color.r));
        assert!(close(back.g, color.g));
        assert!(close(back.b, color.b));
        assert!(close(back.a, color.a));
    }
}
