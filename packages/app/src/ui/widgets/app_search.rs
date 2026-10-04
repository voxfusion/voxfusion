//! The search box that picks an installed app, and the placeholder rows
//! shown while the apps are being listed.

use gpui_kit::base::animation::cubic_bezier;
use gpui_kit::{
    Animation, AnimationExt as _, App, AppContext as _, Context, Div, ElementId, Entity,
    EventEmitter, InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent,
    ParentElement as _, Pixels, Render, ScrollHandle, Size, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, deferred, div, point, prelude::*, px, relative,
};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use crate::backend::InstalledApp;
use crate::ui::grid::above_grid;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::app_icon::{AppIcons, app_icon};
use crate::ui::widgets::text_field::{TextField, TextFieldEvent};
use crate::ui::widgets::{animations_frozen, icon};
use crate::ui::{t, t_upper};

/// How many apps the list offers at most.
const MAX_RESULTS: usize = 50;
/// How many placeholder rows stand in for a list that is loading.
pub const SKELETON_ROWS: usize = 4;

/// The apps a query finds, as indices into `apps`: those not excluded whose
/// name or bundle id contains the query, whatever its case.
fn filter_apps(apps: &[InstalledApp], excluded: &HashSet<String>, query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();

    apps.iter()
        .enumerate()
        .filter(|(_, app)| !excluded.contains(&app.bundle_id))
        .filter(|(_, app)| {
            query.is_empty()
                || app.name.to_lowercase().contains(&query)
                || app.bundle_id.to_lowercase().contains(&query)
        })
        .map(|(index, _)| index)
        .take(MAX_RESULTS)
        .collect()
}

/// The app the user picked. The search stays as it is until [`AppSearch::reset`].
#[derive(Debug, Clone, PartialEq)]
pub struct AppSelected(pub InstalledApp);

pub struct AppSearch {
    field: Entity<TextField>,
    apps: Vec<InstalledApp>,
    icons: Rc<AppIcons>,
    excluded: HashSet<String>,
    /// The apps on offer, as indices into `apps`.
    filtered: Vec<usize>,
    loading: bool,
    open: bool,
    highlighted: usize,
    scroll: ScrollHandle,
    _subscription: Subscription,
}

impl EventEmitter<AppSelected> for AppSearch {}

impl AppSearch {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(window, cx).placeholder(|cx| t(cx, "appInstructions.searchPlaceholder"))
        });

        let subscription = cx.subscribe(&field, |this, _, event, cx| match event {
            TextFieldEvent::Change => {
                this.refilter(cx);
                this.open(cx);
            }
            TextFieldEvent::Focus => this.open(cx),
            _ => {}
        });

        Self {
            field,
            apps: Vec::new(),
            icons: Rc::default(),
            excluded: HashSet::new(),
            filtered: Vec::new(),
            loading: false,
            open: false,
            highlighted: 0,
            scroll: ScrollHandle::new(),
            _subscription: subscription,
        }
    }

    pub fn set_apps(
        &mut self,
        apps: Vec<InstalledApp>,
        icons: Rc<AppIcons>,
        cx: &mut Context<Self>,
    ) {
        self.apps = apps;
        self.icons = icons;
        self.refilter(cx);
    }

    /// The apps that are not offered, by bundle id.
    pub fn set_excluded(&mut self, excluded: HashSet<String>, cx: &mut Context<Self>) {
        self.excluded = excluded;
        self.refilter(cx);
    }

    /// While the apps are being listed, the list shows placeholder rows.
    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    /// Empties the search and closes the list, once a pick has been taken.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.field
            .update(cx, |field, cx| field.set_value("", window, cx));
        self.refilter(cx);
        self.close(cx);
    }

    /// Any change to what is on offer moves the highlight back to the top.
    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.field.read(cx).value(cx);

        self.filtered = filter_apps(&self.apps, &self.excluded, &query);
        self.highlight(0, cx);
    }

    fn open(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            self.open = true;
            self.scroll.set_offset(point(px(0.), px(0.)));
            self.scroll.scroll_to_item(self.highlighted);
        }
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    /// Highlights an offered app and, as the list is open, brings it into view.
    fn highlight(&mut self, position: usize, cx: &mut Context<Self>) {
        self.highlighted = position;

        if self.open {
            self.scroll.scroll_to_item(position);
        }
        cx.notify();
    }

    fn select(&mut self, position: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.filtered.get(position) {
            cx.emit(AppSelected(self.apps[*index].clone()));
        }
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let count = self.filtered.len();

        match event.keystroke.key.as_str() {
            "down" => {
                self.open(cx);
                if count > 0 {
                    self.highlight((self.highlighted + 1).min(count - 1), cx);
                }
            }
            "up" if count > 0 => self.highlight(self.highlighted.saturating_sub(1), cx),
            "enter" if self.open && count > 0 => self.select(self.highlighted.min(count - 1), cx),
            "escape" if self.open => self.close(cx),
            _ => {}
        }
    }

    fn render_options(&self, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        let p = palette(cx);

        if self.loading {
            return (0..SKELETON_ROWS)
                .map(|index| app_row_skeleton(index, None, cx).into_any_element())
                .collect();
        }

        if self.filtered.is_empty() {
            let message = if self.apps.is_empty() {
                "appInstructions.noAppsDetected"
            } else {
                "appInstructions.noMatches"
            };

            return vec![
                div()
                    .px_4()
                    .py_3()
                    .type_xs()
                    .text_color(p.txt_muted)
                    .child(text(t_upper(cx, message)).tracking_wide())
                    .into_any_element(),
            ];
        }

        self.filtered
            .iter()
            .enumerate()
            .map(|(position, index)| {
                let app = &self.apps[*index];

                div()
                    .id(position)
                    .w_full()
                    .px_4()
                    .py_2p5()
                    .flex()
                    .items_center()
                    .gap_3()
                    .type_sm()
                    .when(position == self.highlighted, |option| option.bg(p.hover))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.highlight(position, cx);
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| this.select(position, cx)))
                    .child(app_icon(self.icons.get(&app.bundle_id), &app.name, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_color(p.txt_primary)
                                    .child(text(app.name.clone()).truncate()),
                            )
                            .child(
                                div()
                                    .type_xs()
                                    .text_color(p.txt_muted)
                                    .child(text(app.bundle_id.clone()).truncate()),
                            ),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for AppSearch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        div()
            .relative()
            .mb_6()
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                // The list hangs below the box, outside its bounds.
                if this.open && !this.scroll.bounds().contains(&event.position) {
                    this.close(cx);
                }
            }))
            .child(
                div()
                    .p_4()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(icon("search").size_4().text_color(p.txt_muted))
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(p.txt_primary)
                                    .child(self.field.clone()),
                            ),
                    ),
            )
            .when(self.open, |container| {
                // The border sits outside the scrolling area, as in CSS, so
                // an option scrolled into view is not hidden beneath it.
                container.child(deferred(
                    div()
                        .absolute()
                        .top(relative(1.))
                        .left_0()
                        .w_full()
                        .mt_1()
                        .occlude()
                        .bg(p.surface)
                        .border_1()
                        .border_color(p.border_strong)
                        .child(
                            div()
                                .id("options")
                                .max_h(px(286.))
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .children(self.render_options(cx)),
                        ),
                ))
            })
    }
}

/// `animate-pulse`: fades to half and back every two seconds.
fn pulsing(id: impl Into<ElementId>, element: Div) -> impl IntoElement {
    let frozen = animations_frozen();
    let ease = cubic_bezier(0.4, 0., 0.6, 1.);

    element.with_animation(
        id,
        Animation::new(Duration::from_secs(2)).repeat(),
        move |element, delta| {
            if frozen {
                return element;
            }

            // Each half of the cycle is eased on its own.
            let faded = if delta < 0.5 {
                ease(delta * 2.)
            } else {
                1. - ease(delta * 2. - 1.)
            };

            element.opacity(1. - faded / 2.)
        },
    )
}

/// A placeholder for a row that shows an app. In a list on the page, the
/// row is a card that ends in a block of the `trailing` size; in the search
/// list it is neither.
pub fn app_row_skeleton(
    index: usize,
    trailing: Option<Size<Pixels>>,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let p = palette(cx);

    let row = div()
        .px_4()
        .py_2p5()
        .flex()
        .items_center()
        .gap_3()
        .when(trailing.is_some(), |row| {
            row.bg(p.surface).border_1().border_color(p.border)
        })
        // What is being faded lies above the grid.
        .when(trailing.is_some() && !animations_frozen(), |row| {
            row.relative().child(above_grid(cx))
        })
        .child(div().size_8().flex_shrink_0().bg(p.input))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .min_w_0()
                .flex_1()
                .child(div().h_3().w_32().bg(p.input))
                .child(
                    div()
                        .relative()
                        .h_2p5()
                        .w_48()
                        .bg(p.input)
                        .opacity(0.6)
                        .child(above_grid(cx)),
                ),
        )
        .children(trailing.map(|trailing| {
            div()
                .w(trailing.width)
                .h(trailing.height)
                .flex_shrink_0()
                .bg(p.input)
        }));

    pulsing(("app-row-skeleton", index), row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, bundle_id: &str) -> InstalledApp {
        InstalledApp {
            name: name.into(),
            bundle_id: bundle_id.into(),
            path: format!("/Applications/{name}.app"),
            icon_data_url: None,
        }
    }

    fn apps() -> Vec<InstalledApp> {
        vec![
            app("Safari", "com.apple.Safari"),
            app("Slack", "com.tinyspeck.slackmacgap"),
            app("Zed", "dev.zed.Zed"),
        ]
    }

    #[test]
    fn an_empty_query_offers_every_app_that_is_not_excluded() {
        let excluded = HashSet::from(["com.tinyspeck.slackmacgap".to_string()]);

        assert_eq!(filter_apps(&apps(), &HashSet::new(), "  "), [0, 1, 2]);
        assert_eq!(filter_apps(&apps(), &excluded, ""), [0, 2]);
    }

    #[test]
    fn the_query_matches_names_and_bundle_ids_in_any_case() {
        let none = HashSet::new();

        assert_eq!(filter_apps(&apps(), &none, "SA"), [0]);
        assert_eq!(filter_apps(&apps(), &none, " apple "), [0]);
        assert_eq!(filter_apps(&apps(), &none, "s"), [0, 1]);
        assert_eq!(filter_apps(&apps(), &none, "zed.zed"), [2]);
        assert!(filter_apps(&apps(), &none, "xcode").is_empty());
    }

    #[test]
    fn at_most_fifty_apps_are_offered() {
        let many: Vec<InstalledApp> = (0..80)
            .map(|index| app(&format!("App {index}"), &format!("com.example.app{index}")))
            .collect();

        assert_eq!(
            filter_apps(&many, &HashSet::new(), "app").len(),
            MAX_RESULTS
        );
    }
}
