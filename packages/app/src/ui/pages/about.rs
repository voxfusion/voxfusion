use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div, px};

use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::{t, t_upper};

pub struct AboutPage;

impl Render for AboutPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        div()
            .min_h(window.viewport_size().height)
            .flex()
            .items_center()
            .justify_center()
            .child(
                // The header's own bottom margin collapses into the 32px gap.
                div()
                    .w_full()
                    .max_w(px(672.))
                    .px_4()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .font_mono()
                    .child(
                        div()
                            .type_sm()
                            .text_color(p.txt_secondary)
                            .child(text("[ABOUT] > VOXFUSION").tracking_wider().center()),
                    )
                    .child(
                        div()
                            .type_5xl()
                            .font_weight(gpui_kit::FontWeight::BOLD)
                            .text_color(p.ac)
                            .child(text(t_upper(cx, "about.title")).tracking_wider().center()),
                    )
                    .child(
                        div()
                            .bg(p.surface)
                            .border_1()
                            .border_color(p.border)
                            .p_8()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .type_lg()
                                    .text_color(p.txt_primary)
                                    .child(text(t(cx, "about.welcomeDescription")).center()),
                            )
                            .child(
                                div()
                                    .type_base()
                                    .text_color(p.txt_secondary)
                                    .child(text(t(cx, "about.navigationDescription")).center()),
                            ),
                    ),
            )
    }
}
