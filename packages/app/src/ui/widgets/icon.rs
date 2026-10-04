//! Lucide icons.

use gpui_kit::{
    Animation, AnimationExt as _, App, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Refineable as _, SharedString, Style,
    StyleRefinement, Styled, TransformationMatrix, Window, percentage,
};
use std::time::Duration;

use super::animations_frozen;
use crate::ui::motion::{Glided, gliding};

/// A Lucide icon. Like an inline SVG with `stroke="currentColor"`, it takes
/// the surrounding text color unless it is given its own, which GPUI's `svg`
/// element does not do.
pub struct Icon {
    path: SharedString,
    style: StyleRefinement,
    /// Clockwise rotation, in turns.
    rotation: f32,
}

pub fn icon(name: &str) -> Icon {
    Icon {
        path: format!("icons/{name}.svg").into(),
        style: StyleRefinement::default(),
        rotation: 0.,
    }
    .flex_shrink_0()
}

impl Icon {
    pub fn rotate(mut self, turns: f32) -> Self {
        self.rotation = turns;
        self
    }
}

/// A chevron that points up while its menu is open: `rotate-180`, reached by
/// turning over as `transition-transform` does.
pub fn turning(open: bool, icon: Icon) -> Glided {
    gliding("chevron", if open { 0.5 } else { 0. }, move |turns| {
        icon.rotate(turns)
    })
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for Icon {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Icon {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
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
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.refine(&self.style);

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _layout: &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let color: Hsla = self
            .style
            .text
            .color
            .unwrap_or_else(|| window.text_style().color);

        // Rotate about the icon's center, in device pixels.
        let scale_factor = window.scale_factor();
        let center = bounds.center();
        let transformation = TransformationMatrix::unit()
            .translate(center.scale(scale_factor))
            .rotate(percentage(self.rotation).into())
            .translate(center.scale(-scale_factor));

        let Ok(Some(bytes)) = cx.asset_source().load(&self.path) else {
            return;
        };

        let _ = window.paint_svg(
            bounds,
            self.path.clone(),
            Some(&bytes),
            transformation,
            color,
            cx,
        );
    }
}

/// `animate-spin`: one turn a second.
pub fn spinning(id: impl Into<ElementId>, icon: Icon) -> impl IntoElement {
    let frozen = animations_frozen();

    icon.with_animation(
        id,
        Animation::new(Duration::from_secs(1)).repeat(),
        move |icon, delta| {
            if frozen { icon } else { icon.rotate(delta) }
        },
    )
}
