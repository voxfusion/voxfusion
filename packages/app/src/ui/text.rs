//! The text element every label uses, and the type scale.
//!
//! GPUI's own text has no letter spacing, and it wraps and places baselines a
//! little differently from a browser. This element lays text out the way CSS
//! does: break opportunities follow the Unicode line breaking algorithm,
//! `letter-spacing` is added after every glyph, a truncated line ends in an
//! ellipsis, and the baseline sits where a browser puts it.

use gpui_kit::{
    App, AvailableSpace, Bounds, Element, ElementId, FontId, FontWeight, GlobalElementId, GlyphId,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Style, Styled,
    TextAlign, TextRun, Window, point, px, size,
};
use std::cell::RefCell;
use std::rc::Rc;
use unicode_linebreak::{BreakOpportunity, linebreaks};

use super::theme::{mono_font, mono_font_fallbacks, sans_font};

/// Tailwind's `tracking-wide`, in em.
pub const TRACKING_WIDE: f32 = 0.025;
/// Tailwind's `tracking-wider`, in em.
pub const TRACKING_WIDER: f32 = 0.05;

#[derive(Clone, Copy, PartialEq)]
enum Wrap {
    /// Break at Unicode line break opportunities; long words overflow.
    Normal,
    /// One line, however long.
    Never,
    /// One line, ending in an ellipsis where it would overflow.
    Truncate,
    /// Break between any two glyphs.
    BreakAll,
}

#[derive(Clone)]
struct Span {
    text: SharedString,
    color: Option<Hsla>,
    weight: Option<FontWeight>,
}

/// A run of text. It takes its font, size, line height and color from the
/// enclosing element, like a text node.
pub struct Text {
    spans: Vec<Span>,
    tracking: f32,
    wrap: Wrap,
    keep_newlines: bool,
    align: Option<TextAlign>,
    underline: bool,
}

pub fn text(content: impl Into<SharedString>) -> Text {
    Text {
        spans: vec![Span {
            text: content.into(),
            color: None,
            weight: None,
        }],
        tracking: 0.,
        wrap: Wrap::Normal,
        keep_newlines: false,
        align: None,
        underline: false,
    }
}

impl Text {
    /// Appends text in its own color, as an inline `<span>` would.
    pub fn span(mut self, content: impl Into<SharedString>, color: Hsla) -> Self {
        self.spans.push(Span {
            text: content.into(),
            color: Some(color),
            weight: None,
        });
        self
    }

    /// Appends text in the inherited color.
    pub fn plain(mut self, content: impl Into<SharedString>) -> Self {
        self.spans.push(Span {
            text: content.into(),
            color: None,
            weight: None,
        });
        self
    }

    /// Colors the first span.
    pub fn color(mut self, color: Hsla) -> Self {
        self.spans[0].color = Some(color);
        self
    }

    /// Letter spacing in em.
    pub fn tracking(mut self, em: f32) -> Self {
        self.tracking = em;
        self
    }

    pub fn tracking_wide(self) -> Self {
        self.tracking(TRACKING_WIDE)
    }

    pub fn tracking_wider(self) -> Self {
        self.tracking(TRACKING_WIDER)
    }

    pub fn nowrap(mut self) -> Self {
        self.wrap = Wrap::Never;
        self
    }

    /// CSS `truncate`: one line, with an ellipsis where it overflows.
    pub fn truncate(mut self) -> Self {
        self.wrap = Wrap::Truncate;
        self
    }

    /// CSS `break-all`.
    pub fn break_all(mut self) -> Self {
        self.wrap = Wrap::BreakAll;
        self
    }

    /// CSS `whitespace-pre-wrap`: newlines in the text break lines.
    pub fn pre_wrap(mut self) -> Self {
        self.keep_newlines = true;
        self
    }

    pub fn center(mut self) -> Self {
        self.align = Some(TextAlign::Center);
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.underline = underline;
        self
    }
}

#[derive(Clone, Copy)]
struct Glyph {
    font_id: FontId,
    id: GlyphId,
    is_emoji: bool,
    /// Advance including letter spacing.
    advance: Pixels,
    /// Byte offset of the glyph's first character in the shaped text.
    index: usize,
    span: usize,
    is_space: bool,
}

/// Shaped text, ready to be broken into lines for any width.
struct Shaped {
    glyphs: Vec<Glyph>,
    /// For each glyph, whether a line may end right before it.
    break_before: Vec<Option<BreakOpportunity>>,
    ellipsis: Option<Glyph>,
    ascent: Pixels,
    descent: Pixels,
    font_size: Pixels,
    line_height: Pixels,
    /// Each span's own color; spans without one use the inherited color.
    colors: Vec<Option<Hsla>>,
}

#[derive(Clone, Debug, PartialEq)]
struct Line {
    /// Glyph range, without the trailing spaces a wrapped line drops.
    start: usize,
    end: usize,
    width: Pixels,
    ellipsis: bool,
}

/// A browser accepts a line that overflows by less than its layout unit.
const FIT_TOLERANCE: f32 = 1. / 64.;

impl Shaped {
    fn width_of(&self, start: usize, end: usize) -> Pixels {
        self.glyphs[start..end]
            .iter()
            .fold(px(0.), |width, glyph| width + glyph.advance)
    }

    /// The end of `start..end` with trailing spaces removed.
    fn trim_end(&self, start: usize, mut end: usize) -> usize {
        while end > start && self.glyphs[end - 1].is_space {
            end -= 1;
        }
        end
    }

    fn lines(&self, wrap: Wrap, available: Option<Pixels>) -> Vec<Line> {
        let count = self.glyphs.len();
        let whole = |start: usize, end: usize| Line {
            start,
            end,
            width: self.width_of(start, end),
            ellipsis: false,
        };

        // Segments are separated by mandatory breaks (newlines).
        let mut segments = Vec::new();
        let mut segment_start = 0;
        for index in 1..count {
            if self.break_before[index] == Some(BreakOpportunity::Mandatory) {
                segments.push((segment_start, index));
                segment_start = index;
            }
        }
        segments.push((segment_start, count));

        let mut lines = Vec::new();

        for (start, end) in segments {
            let Some(available) = available else {
                lines.push(whole(start, self.trim_end(start, end)));
                continue;
            };
            let limit = available + px(FIT_TOLERANCE);

            match wrap {
                Wrap::Never => lines.push(whole(start, end)),
                Wrap::Truncate => lines.push(self.truncated(start, end, limit)),
                Wrap::Normal | Wrap::BreakAll => {
                    let mut line_start = start;
                    let mut last_break = None;
                    let mut width = px(0.);
                    let mut index = start;

                    while index < end {
                        let glyph = &self.glyphs[index];
                        let can_break = index > line_start
                            && (wrap == Wrap::BreakAll || self.break_before[index].is_some());
                        if can_break {
                            last_break = Some(index);
                        }

                        // Spaces at the end of a line hang past its edge.
                        if !glyph.is_space
                            && width + glyph.advance > limit
                            && let Some(at) = last_break
                        {
                            lines.push(whole(line_start, self.trim_end(line_start, at)));
                            line_start = at;
                            last_break = None;
                            width = px(0.);
                            index = at;
                            continue;
                        }

                        width += glyph.advance;
                        index += 1;
                    }

                    lines.push(whole(line_start, self.trim_end(line_start, end)));
                }
            }
        }

        lines
    }

    fn truncated(&self, start: usize, end: usize, limit: Pixels) -> Line {
        let full = self.width_of(start, end);
        if full <= limit {
            return Line {
                start,
                end,
                width: full,
                ellipsis: false,
            };
        }

        let ellipsis_width = self.ellipsis.map_or(px(0.), |glyph| glyph.advance);
        let mut width = px(0.);
        let mut cut = start;

        while cut < end && width + self.glyphs[cut].advance + ellipsis_width <= limit {
            width += self.glyphs[cut].advance;
            cut += 1;
        }

        Line {
            start,
            end: cut,
            width: width + ellipsis_width,
            ellipsis: self.ellipsis.is_some(),
        }
    }

    /// The distance from the top of a line to its baseline, placed as a
    /// browser places it: whole-pixel ascent and descent, with the leading
    /// above the text rounded down.
    fn baseline(&self) -> Pixels {
        let ascent = self.ascent.round();
        let descent = self.descent.round();
        let leading_above = ((self.line_height - ascent - descent) / 2.).floor();

        leading_above + ascent
    }
}

/// The lines of the last measurement, with the width they were broken for.
type MeasuredLines = Option<(Option<Pixels>, Vec<Line>)>;

pub struct TextLayout {
    shaped: Rc<Shaped>,
    lines: Rc<RefCell<MeasuredLines>>,
}

impl Text {
    fn shape(&self, window: &mut Window, cx: &mut App) -> Shaped {
        let style = window.text_style();
        let rem_size = window.rem_size();
        let font_size = style.font_size.to_pixels(rem_size);
        // GPUI rounds line heights to whole pixels; a browser keeps the
        // fraction (`leading-relaxed` at 14px is 22.75px).
        let line_height = style.line_height.to_pixels(font_size.into(), rem_size);
        let tracking = font_size * self.tracking;

        // The baseline comes from the element's own font, as in CSS, whatever
        // fallback fonts individual glyphs end up in.
        let text_system = cx.text_system().clone();
        let primary_font = text_system.resolve_font(&style.font());

        // Shape everything as one line so breaking can happen anywhere.
        let mut content = String::new();
        let mut runs = Vec::new();
        let mut span_ends = Vec::new();
        let mut colors = Vec::new();

        for span in &self.spans {
            let piece = if self.keep_newlines {
                span.text.to_string()
            } else {
                span.text.replace('\n', " ")
            };

            let mut font = style.font();
            if let Some(weight) = span.weight {
                font.weight = weight;
            }

            runs.push(TextRun {
                len: piece.len(),
                font,
                color: style.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            content.push_str(&piece);
            span_ends.push(content.len());
            colors.push(span.color);
        }

        // The shaper drops newlines; breaking still needs to know where they were.
        let shaped_content = content.replace('\n', " ");
        let layout = window
            .text_system()
            .layout_line(&shaped_content, font_size, &runs, None);

        let mut glyphs = Vec::new();
        for run in &layout.runs {
            for glyph in &run.glyphs {
                let span = span_ends
                    .iter()
                    .position(|end| glyph.index < *end)
                    .unwrap_or(span_ends.len() - 1);
                let character = content[glyph.index..].chars().next().unwrap_or(' ');

                glyphs.push((
                    glyph.position.x,
                    Glyph {
                        font_id: run.font_id,
                        id: glyph.id,
                        is_emoji: glyph.is_emoji,
                        advance: px(0.),
                        index: glyph.index,
                        span,
                        is_space: character == ' ' || character == '\n',
                    },
                ));
            }
        }

        let mut break_at = vec![None; content.len() + 1];
        for (index, opportunity) in linebreaks(&content) {
            break_at[index] = Some(opportunity);
        }

        let mut shaped_glyphs = Vec::with_capacity(glyphs.len());
        let mut break_before = Vec::with_capacity(glyphs.len());
        for (position, (x, mut glyph)) in glyphs.iter().copied().enumerate() {
            let next_x = glyphs
                .get(position + 1)
                .map_or(layout.width, |(next_x, _)| *next_x);

            glyph.advance = next_x - x + tracking;
            break_before.push(if position == 0 {
                None
            } else {
                break_at[glyph.index]
            });
            shaped_glyphs.push(glyph);
        }

        let ellipsis = (self.wrap == Wrap::Truncate)
            .then(|| {
                let layout = window.text_system().layout_line(
                    "…",
                    font_size,
                    &[TextRun {
                        len: "…".len(),
                        font: style.font(),
                        color: style.color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let run = layout.runs.first()?;
                let glyph = run.glyphs.first()?;

                Some(Glyph {
                    font_id: run.font_id,
                    id: glyph.id,
                    is_emoji: false,
                    advance: layout.width + tracking,
                    index: 0,
                    span: 0,
                    is_space: false,
                })
            })
            .flatten();

        Shaped {
            glyphs: shaped_glyphs,
            break_before,
            ellipsis,
            ascent: text_system.ascent(primary_font, font_size),
            // The sign of a font's descent differs between GPUI's platforms.
            descent: text_system.descent(primary_font, font_size).abs(),
            font_size,
            line_height,
            colors,
        }
    }
}

impl IntoElement for Text {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Text {
    type RequestLayoutState = TextLayout;
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
    ) -> (LayoutId, Self::RequestLayoutState) {
        let shaped = Rc::new(self.shape(window, cx));
        let lines = Rc::new(RefCell::new(None));
        let wrap = self.wrap;

        let layout_id = window.request_measured_layout(Style::default(), {
            let shaped = shaped.clone();
            let lines = lines.clone();

            move |known, available, _window, _cx| {
                let wrap_width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    AvailableSpace::MinContent => Some(px(0.)),
                    AvailableSpace::MaxContent => None,
                });

                let broken = shaped.lines(wrap, wrap_width);
                let widest = broken
                    .iter()
                    .fold(px(0.), |widest, line| widest.max(line.width));
                let height = shaped.line_height * broken.len().max(1) as f32;

                *lines.borrow_mut() = Some((wrap_width, broken));

                size(
                    known.width.unwrap_or(widest),
                    known.height.unwrap_or(height),
                )
            }
        });

        (layout_id, TextLayout { shaped, lines })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let shaped = &layout.shaped;

        // The last measurement may have been for another width.
        let lines = match layout.lines.borrow().as_ref() {
            Some((Some(width), lines)) if (*width - bounds.size.width).abs() < px(0.01) => {
                lines.clone()
            }
            _ => shaped.lines(self.wrap, Some(bounds.size.width)),
        };

        let align = self.align.unwrap_or_else(|| window.text_style().text_align);
        let baseline = shaped.baseline();
        // Read while painting: a hovered ancestor's color is not known earlier.
        let inherited_color = window.text_style().color;

        for (row, line) in lines.iter().enumerate() {
            let slack = bounds.size.width - line.width;
            let offset = match align {
                TextAlign::Center => slack / 2.,
                TextAlign::Right => slack,
                _ => px(0.),
            };

            let top = bounds.origin.y + shaped.line_height * row as f32;
            let mut x = bounds.origin.x + offset;
            let line_left = x;

            let glyphs = shaped.glyphs[line.start..line.end]
                .iter()
                .chain(line.ellipsis.then_some(()).and(shaped.ellipsis.as_ref()));

            let mut last_color = inherited_color;
            for glyph in glyphs {
                let color = shaped.colors[glyph.span.min(shaped.colors.len() - 1)]
                    .unwrap_or(inherited_color);
                last_color = color;
                let origin = point(x, top + baseline);

                if !glyph.is_space {
                    let _ = if glyph.is_emoji {
                        window.paint_emoji(origin, glyph.font_id, glyph.id, shaped.font_size)
                    } else {
                        window.paint_glyph(origin, glyph.font_id, glyph.id, shaped.font_size, color)
                    };
                }

                x += glyph.advance;
            }

            if self.underline {
                let thickness = (shaped.font_size / 14.).max(px(1.)).round();
                window.paint_quad(gpui_kit::fill(
                    Bounds::new(
                        point(line_left, top + baseline + px(1.)),
                        size(x - line_left, thickness),
                    ),
                    last_color,
                ));
            }
        }
    }
}

/// Tailwind's type scale: each size sets its own line height.
pub trait TypeScale: Styled + Sized {
    /// `text-xs`: 12px on a 16px line.
    fn type_xs(self) -> Self {
        self.text_size(px(12.)).line_height(px(16.))
    }

    /// `text-sm`: 14px on a 20px line.
    fn type_sm(self) -> Self {
        self.text_size(px(14.)).line_height(px(20.))
    }

    /// `text-base`: 16px on a 24px line.
    fn type_base(self) -> Self {
        self.text_size(px(16.)).line_height(px(24.))
    }

    /// `text-lg`: 18px on a 28px line.
    fn type_lg(self) -> Self {
        self.text_size(px(18.)).line_height(px(28.))
    }

    /// `text-xl`: 20px on a 28px line.
    fn type_xl(self) -> Self {
        self.text_size(px(20.)).line_height(px(28.))
    }

    /// `text-2xl`: 24px on a 32px line.
    fn type_2xl(self) -> Self {
        self.text_size(px(24.)).line_height(px(32.))
    }

    /// `text-5xl`: 48px on a 48px line.
    fn type_5xl(self) -> Self {
        self.text_size(px(48.)).line_height(px(48.))
    }

    /// An arbitrary size such as `text-[10px]`, which keeps the page's 1.5
    /// line height.
    fn type_px(self, font_size: f32) -> Self {
        self.text_size(px(font_size))
            .line_height(px(font_size * 1.5))
    }

    /// `leading-relaxed` for text of `font_size`.
    fn leading_relaxed(self, font_size: f32) -> Self {
        self.line_height(px(font_size * 1.625))
    }

    /// `font-mono`.
    fn font_mono(mut self) -> Self {
        self.text_style().font_fallbacks = mono_font_fallbacks();
        self.font_family(mono_font())
    }

    /// The page's default family.
    fn font_sans(self) -> Self {
        self.font_family(sans_font())
    }
}

impl<T: Styled> TypeScale for T {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped text where every character is one glyph of `advance` pixels.
    fn mono(text: &str, advance: f32) -> Shaped {
        let mut break_at = vec![None; text.len() + 1];
        for (index, opportunity) in linebreaks(text) {
            break_at[index] = Some(opportunity);
        }

        let glyphs: Vec<Glyph> = text
            .char_indices()
            .map(|(index, character)| Glyph {
                font_id: FontId(0),
                id: GlyphId(0),
                is_emoji: false,
                advance: px(advance),
                index,
                span: 0,
                is_space: character == ' ' || character == '\n',
            })
            .collect();

        let break_before = glyphs
            .iter()
            .enumerate()
            .map(|(position, glyph)| {
                if position == 0 {
                    None
                } else {
                    break_at[glyph.index]
                }
            })
            .collect();

        Shaped {
            glyphs,
            break_before,
            ellipsis: Some(Glyph {
                font_id: FontId(0),
                id: GlyphId(0),
                is_emoji: false,
                advance: px(advance),
                index: 0,
                span: 0,
                is_space: false,
            }),
            ascent: px(10.),
            descent: px(3.),
            font_size: px(12.),
            line_height: px(16.),
            colors: vec![None],
        }
    }

    fn rendered(shaped: &Shaped, text: &str, lines: &[Line]) -> Vec<String> {
        let _ = shaped;
        lines
            .iter()
            .map(|line| {
                let mut row: String = text
                    .chars()
                    .skip(line.start)
                    .take(line.end - line.start)
                    .collect();
                if line.ellipsis {
                    row.push('…');
                }
                row
            })
            .collect()
    }

    #[test]
    fn wraps_at_spaces_and_drops_the_space_at_the_break() {
        let text = "one two three";
        let shaped = mono(text, 10.);
        let lines = shaped.lines(Wrap::Normal, Some(px(75.)));

        assert_eq!(rendered(&shaped, text, &lines), ["one two", "three"]);
        assert_eq!(lines[0].width, px(70.));
    }

    #[test]
    fn a_line_that_fits_exactly_is_not_wrapped() {
        let text = "one two";
        let shaped = mono(text, 10.);

        assert_eq!(shaped.lines(Wrap::Normal, Some(px(70.))).len(), 1);
        assert_eq!(shaped.lines(Wrap::Normal, Some(px(69.))).len(), 2);
    }

    #[test]
    fn a_long_word_overflows_instead_of_breaking() {
        let text = "a verylongword b";
        let shaped = mono(text, 10.);
        let lines = shaped.lines(Wrap::Normal, Some(px(50.)));

        assert_eq!(rendered(&shaped, text, &lines), ["a", "verylongword", "b"]);
    }

    #[test]
    fn break_all_breaks_inside_words() {
        let text = "abcdefgh";
        let shaped = mono(text, 10.);
        let lines = shaped.lines(Wrap::BreakAll, Some(px(30.)));

        assert_eq!(rendered(&shaped, text, &lines), ["abc", "def", "gh"]);
    }

    #[test]
    fn truncation_leaves_room_for_the_ellipsis() {
        let text = "abcdefgh";
        let shaped = mono(text, 10.);
        let lines = shaped.lines(Wrap::Truncate, Some(px(50.)));

        assert_eq!(rendered(&shaped, text, &lines), ["abcd…"]);
        assert_eq!(lines[0].width, px(50.));
    }

    #[test]
    fn text_that_fits_is_not_truncated() {
        let text = "abc";
        let shaped = mono(text, 10.);
        let lines = shaped.lines(Wrap::Truncate, Some(px(30.)));

        assert_eq!(rendered(&shaped, text, &lines), ["abc"]);
    }

    #[test]
    fn newlines_break_lines_when_kept() {
        let text = "one\ntwo";
        let shaped = mono(text, 10.);
        let lines = shaped.lines(Wrap::Normal, Some(px(500.)));

        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].start, lines[0].end), (0, 3));
        assert_eq!((lines[1].start, lines[1].end), (4, 7));
    }

    #[test]
    fn the_baseline_rounds_the_way_a_browser_does() {
        let mut shaped = mono("a", 10.);
        shaped.ascent = px(10.86);
        shaped.descent = px(2.72);
        shaped.line_height = px(16.);

        // ascent 11, descent 3, leading above floor((16 - 14) / 2) = 1.
        assert_eq!(shaped.baseline(), px(12.));
    }
}
