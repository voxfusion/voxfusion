//! The faint 40px grid over the window.
//!
//! In the original stylesheet the grid is an absolutely positioned layer
//! that comes first in the document, so it paints above plain content (the
//! sidebar, cards) and below anything positioned after it. The overlay here
//! paints last and skips the areas that positioned, opaque elements cover.

use gpui_kit::{
    App, Bounds, Global, Hsla, IntoElement, ParentElement as _, Pixels, Styled as _, canvas, div,
    fill, point, px, size,
};
use std::cell::RefCell;
use std::rc::Rc;

const CELL: f32 = 40.;

#[derive(Clone, Default)]
pub struct GridLayer {
    covered: Rc<RefCell<Vec<Bounds<Pixels>>>>,
}

impl Global for GridLayer {}

impl GridLayer {
    fn get(cx: &mut App) -> GridLayer {
        cx.default_global::<GridLayer>().clone()
    }
}

/// Forgets the covered areas of the previous frame. Place it before the
/// content the grid lies over.
pub fn grid_reset(cx: &mut App) -> impl IntoElement + use<> {
    let layer = GridLayer::get(cx);

    canvas(
        move |_, _, _| layer.covered.borrow_mut().clear(),
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// Marks its parent's box as painted above the grid. The parent must be
/// positioned (`relative` or `absolute`).
pub fn above_grid(cx: &mut App) -> impl IntoElement + use<> {
    let layer = GridLayer::get(cx);

    canvas(
        move |bounds, window, _| {
            let visible = bounds.intersect(&window.content_mask().bounds);
            if visible.size.width > px(0.) && visible.size.height > px(0.) {
                layer.covered.borrow_mut().push(visible);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

/// Splits `start..end` around the `covered` intervals.
fn uncovered(start: f32, end: f32, mut covered: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    covered.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut segments = Vec::new();
    let mut cursor = start;

    for (from, to) in covered {
        if to <= cursor {
            continue;
        }
        if from >= end {
            break;
        }
        if from > cursor {
            segments.push((cursor, from));
        }
        cursor = cursor.max(to);
    }

    if cursor < end {
        segments.push((cursor, end));
    }

    segments
}

/// The grid itself. Place it after the content it lies over.
pub fn grid_overlay(color: Hsla, cx: &mut App) -> impl IntoElement + use<> {
    let layer = GridLayer::get(cx);

    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let covered = layer.covered.borrow();
                let left = f32::from(bounds.origin.x);
                let top = f32::from(bounds.origin.y);
                let right = left + f32::from(bounds.size.width);
                let bottom = top + f32::from(bounds.size.height);

                // Vertical lines first: the stylesheet lists the horizontal
                // gradient first, which puts it on top.
                let mut x = left;
                while x < right {
                    let blocked = covered
                        .iter()
                        .filter(|area| f32::from(area.left()) <= x && x < f32::from(area.right()))
                        .map(|area| (f32::from(area.top()), f32::from(area.bottom())))
                        .collect();

                    for (from, to) in uncovered(top, bottom, blocked) {
                        window.paint_quad(fill(
                            Bounds::new(point(px(x), px(from)), size(px(1.), px(to - from))),
                            color,
                        ));
                    }
                    x += CELL;
                }

                let mut y = top;
                while y < bottom {
                    let blocked = covered
                        .iter()
                        .filter(|area| f32::from(area.top()) <= y && y < f32::from(area.bottom()))
                        .map(|area| (f32::from(area.left()), f32::from(area.right())))
                        .collect();

                    for (from, to) in uncovered(left, right, blocked) {
                        window.paint_quad(fill(
                            Bounds::new(point(px(from), px(y)), size(px(to - from), px(1.))),
                            color,
                        ));
                    }
                    y += CELL;
                }
            },
        )
        .size_full(),
    )
}

#[cfg(test)]
mod tests {
    use super::uncovered;

    #[test]
    fn a_line_is_split_around_covered_spans() {
        assert_eq!(
            uncovered(0., 100., vec![(60., 80.), (10., 20.)]),
            [(0., 10.), (20., 60.), (80., 100.)]
        );
    }

    #[test]
    fn overlapping_and_outside_spans_are_handled() {
        assert_eq!(
            uncovered(
                0.,
                100.,
                vec![(-10., 15.), (10., 30.), (90., 140.), (200., 300.)]
            ),
            [(30., 90.)]
        );
        assert_eq!(uncovered(0., 100., vec![]), [(0., 100.)]);
        assert!(uncovered(0., 100., vec![(0., 100.)]).is_empty());
    }
}
