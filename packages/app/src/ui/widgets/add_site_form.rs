//! The form that adds a site, by its domain, to a per-site list.

use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Window, div, prelude::*, px,
};

use crate::ui::t;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::app_icon::site_icon;
use crate::ui::widgets::text_field::{TextField, TextFieldEvent, field_box};
use crate::ui::widgets::word_list::{Variant, add_button};

/// The domain a user means by `input`, which may be a whole URL: lowercase,
/// without scheme, credentials, port, path or a leading `www.`.
fn normalize_domain(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_scheme = trimmed.split("://").nth(1).unwrap_or(trimmed);
    let authority = without_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host_with_port = authority.rsplit('@').next().unwrap_or("");
    let host = host_with_port.split(':').next().unwrap_or("").trim();

    let lowered = host.to_lowercase();
    let lowered = lowered.trim_matches('.');
    let stripped = lowered.strip_prefix("www.").unwrap_or(lowered);

    (!stripped.is_empty()).then(|| stripped.to_string())
}

/// The domain to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAdded(pub String);

pub struct AddSiteForm {
    field: Entity<TextField>,
    invalid: bool,
    _subscription: Subscription,
}

impl EventEmitter<SiteAdded> for AddSiteForm {}

impl AddSiteForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(window, cx).placeholder(|cx| t(cx, "dictionary.sitesDomainPlaceholder"))
        });

        let subscription =
            cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
                TextFieldEvent::Change => {
                    this.invalid = false;
                    cx.notify();
                }
                TextFieldEvent::Enter => this.add(window, cx),
                _ => {}
            });

        Self {
            field,
            invalid: false,
            _subscription: subscription,
        }
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(domain) = normalize_domain(&self.field.read(cx).value(cx)) else {
            self.invalid = true;
            cx.notify();
            return;
        };

        self.invalid = false;
        self.field
            .update(cx, |field, cx| field.set_value("", window, cx));
        cx.emit(SiteAdded(domain));
        cx.notify();
    }
}

impl Render for AddSiteForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let input = self.field.read(cx).value(cx);
        let domain = normalize_domain(&input);

        div()
            .mb_6()
            .p_4()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(site_icon(domain.as_deref(), px(20.), cx))
                    .child(
                        field_box(&self.field, cx)
                            .flex_1()
                            .text_color(p.txt_primary),
                    )
                    .child(add_button(
                        Variant::Nested,
                        t(cx, "dictionary.sitesAddSite"),
                        input.trim().is_empty(),
                        cx.listener(|this, _, window, cx| this.add(window, cx)),
                        cx,
                    )),
            )
            .when(self.invalid, |form| {
                form.child(
                    div().flex().justify_end().child(
                        div()
                            .type_xs()
                            .text_color(p.ac)
                            .child(text(t(cx, "dictionary.sitesInvalidDomain"))),
                    ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_domain;

    #[track_caller]
    fn assert_normalized(input: &str, expected: Option<&str>) {
        assert_eq!(normalize_domain(input).as_deref(), expected, "{input:?}");
    }

    #[test]
    fn a_plain_domain_is_lowercased_and_trimmed() {
        assert_normalized("  GitHub.com ", Some("github.com"));
        assert_normalized("linear.app", Some("linear.app"));
        assert_normalized("localhost", Some("localhost"));
    }

    #[test]
    fn a_url_is_reduced_to_its_host() {
        assert_normalized(
            "https://www.GitHub.com/voxfusion/app?tab=readme#top",
            Some("github.com"),
        );
        assert_normalized("http://example.com:8080/path", Some("example.com"));
        assert_normalized(
            "https://user:secret@mail.google.com/",
            Some("mail.google.com"),
        );
        assert_normalized("example.com?query=1", Some("example.com"));
        assert_normalized("example.com#section", Some("example.com"));
    }

    #[test]
    fn only_a_leading_www_is_removed() {
        assert_normalized("www.example.com", Some("example.com"));
        assert_normalized("www.www.example.com", Some("www.example.com"));
        assert_normalized("awww.example.com", Some("awww.example.com"));
        assert_normalized("www.", Some("www"));
    }

    #[test]
    fn dots_around_the_host_are_removed() {
        assert_normalized(".example.com.", Some("example.com"));
        assert_normalized("..example.com..", Some("example.com"));
    }

    #[test]
    fn input_without_a_host_is_refused() {
        assert_normalized("", None);
        assert_normalized("   ", None);
        assert_normalized("https://", None);
        assert_normalized("/path/only", None);
        assert_normalized(":8080", None);
        assert_normalized("...", None);
        assert_normalized("user@", None);
    }

    #[test]
    fn only_what_follows_the_first_scheme_separator_counts() {
        assert_normalized("a://b://c", Some("b"));
    }
}
