//! The main window: the loading screen, onboarding, and the app itself with
//! its sidebar, pages and settings.

use gpui_kit::{
    Animation, AnimationExt as _, App, AppContext as _, Bounds, Context, DismissEvent, Entity,
    FocusHandle, Focusable, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, ease_in_out, img, prelude::*, px,
    size,
};
use std::time::Duration;

use crate::actions::OpenSettings;
use crate::backend::{self, AppEvent};
use crate::events;
use crate::settings::{self, MODEL_DOWNLOAD_STEP, SettingsStore};
use crate::ui::apps_cache;
use crate::ui::grid::{grid_overlay, grid_reset};
use crate::ui::motion::Transitions as _;
use crate::ui::onboarding::OnboardingWizard;
use crate::ui::pages::{AboutPage, DictionaryPage, HomePage, StylePage};
use crate::ui::settings_modal::SettingsModal;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::{self, palette};
use crate::ui::update_notification::UpdateNotification;
use crate::ui::widgets::{animations_frozen, icon};
use crate::ui::{t_upper, upper};

pub const MAIN_WINDOW_SIZE: (f32, f32) = (1360., 850.);
pub const MAIN_WINDOW_MIN_SIZE: (f32, f32) = (1024., 720.);

/// The tabs of the Dictionary and Style pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionTab {
    Default,
    PerApp,
    Sites,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Home,
    About,
    Dictionary(SectionTab),
    Style(SectionTab),
}

impl Route {
    pub fn from_path(path: &str) -> Route {
        let tab = |rest: &str| match rest.trim_matches('/') {
            "per-app" => SectionTab::PerApp,
            "sites" => SectionTab::Sites,
            _ => SectionTab::Default,
        };

        if let Some(rest) = path.strip_prefix("/dictionary") {
            Route::Dictionary(tab(rest))
        } else if let Some(rest) = path.strip_prefix("/style") {
            Route::Style(tab(rest))
        } else if path.starts_with("/about") {
            Route::About
        } else {
            Route::Home
        }
    }
}

enum Page {
    Home(Entity<HomePage>),
    About(Entity<AboutPage>),
    Dictionary(Entity<DictionaryPage>),
    Style(Entity<StylePage>),
}

pub struct MainView {
    ready: bool,
    route: Route,
    page: Option<Page>,
    settings_modal: Option<Entity<SettingsModal>>,
    open_settings_when_ready: bool,
    onboarding: Option<Entity<OnboardingWizard>>,
    update_notification: Option<Entity<UpdateNotification>>,
    scroll: ScrollHandle,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// Development builds show onboarding regardless of the setting when
/// `VOXFUSION_FORCE_ONBOARDING=true` (`bun run dev:onboarding`).
fn force_onboarding() -> bool {
    cfg!(debug_assertions)
        && std::env::var("VOXFUSION_FORCE_ONBOARDING").is_ok_and(|value| value == "true")
}

/// Asks the main window to show the page at `path`.
pub fn navigate(cx: &App, path: &str) {
    events::emit(cx, AppEvent::Navigate(path.to_string()));
}

impl MainView {
    pub fn new(route: Route, window: &mut Window, cx: &mut Context<Self>) -> Self {
        log::info!(target: "app", "mount_started");

        let subscriptions = vec![
            cx.subscribe_in(&events::hub(cx), window, Self::handle_event),
            cx.observe_in(&SettingsStore::entity(cx), window, |this, _, window, cx| {
                this.sync_onboarding(window, cx);
                cx.notify();
            }),
        ];

        let view = Self {
            ready: false,
            route,
            page: None,
            settings_modal: None,
            open_settings_when_ready: false,
            onboarding: None,
            update_notification: None,
            scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };

        // A finished onboarding without a model cannot transcribe: send the
        // user back to the download step.
        let model_status = backend::call(cx, |backend| backend.check_model_status());
        cx.spawn_in(window, async move |this, cx| {
            let model_ready = model_status.await;

            this.update_in(cx, |this, window, cx| {
                match model_ready {
                    Ok(false) if SettingsStore::get(cx).onboarding_complete => {
                        log::warn!(target: "app", "model_missing_resume_onboarding");
                        settings::resume_onboarding_at(cx, MODEL_DOWNLOAD_STEP);
                    }
                    Err(error) => {
                        log::error!(target: "app", "model_status_failed error={error}");
                    }
                    _ => {}
                }

                this.ready = true;
                log::info!(target: "app", "ready");
                this.sync_onboarding(window, cx);
                this.show_route(this.route, window, cx);
                this.update_notification = Some(cx.new(UpdateNotification::new));
                apps_cache::warm(cx);

                if std::mem::take(&mut this.open_settings_when_ready) {
                    this.open_settings(window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();

        view
    }

    fn handle_event(
        &mut self,
        _: &Entity<events::EventHub>,
        event: &AppEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let AppEvent::Navigate(path) = event {
            log::debug!(target: "app", "navigate_event path={path}");
            self.show_route(Route::from_path(path), window, cx);
        }
    }

    fn show_onboarding(&self, cx: &App) -> bool {
        force_onboarding() || !SettingsStore::get(cx).onboarding_complete
    }

    /// Creates the wizard when onboarding starts and drops it when it ends.
    fn sync_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready {
            return;
        }

        if self.show_onboarding(cx) {
            if self.onboarding.is_none() {
                let step = SettingsStore::get(cx).onboarding_step;
                self.onboarding = Some(cx.new(|cx| OnboardingWizard::new(step, window, cx)));
            }
        } else {
            self.onboarding = None;
        }
    }

    fn show_route(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>) {
        let same_section = std::mem::discriminant(&route) == std::mem::discriminant(&self.route);
        self.route = route;

        match (&self.page, route) {
            (Some(Page::Dictionary(page)), Route::Dictionary(tab)) if same_section => {
                page.update(cx, |page, cx| page.set_tab(tab, window, cx));
            }
            (Some(Page::Style(page)), Route::Style(tab)) if same_section => {
                page.update(cx, |page, cx| page.set_tab(tab, window, cx));
            }
            (Some(_), _) if same_section => {}
            _ => {
                self.page = Some(match route {
                    Route::Home => Page::Home(cx.new(HomePage::new)),
                    Route::About => Page::About(cx.new(|_| AboutPage)),
                    Route::Dictionary(tab) => {
                        Page::Dictionary(cx.new(|cx| DictionaryPage::new(tab, window, cx)))
                    }
                    Route::Style(tab) => Page::Style(cx.new(|cx| StylePage::new(tab, window, cx))),
                });
                self.scroll.set_offset(gpui_kit::point(px(0.), px(0.)));
            }
        }

        cx.notify();
    }

    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready {
            // A window that was just opened shows the settings once loaded.
            self.open_settings_when_ready = true;
            return;
        }
        if self.settings_modal.is_some() || self.show_onboarding(cx) {
            return;
        }

        let modal = cx.new(|cx| SettingsModal::new(window, cx));
        cx.subscribe_in(&modal, window, |this, _, _: &DismissEvent, window, cx| {
            this.settings_modal = None;
            window.focus(&this.focus_handle, cx);
            cx.notify();
        })
        .detach();

        self.settings_modal = Some(modal);
        cx.notify();
    }

    fn render_splash(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let frozen = animations_frozen();

        // The bar slides from fully left of the track to four of its widths
        // right, as `translateX(-100%)` to `translateX(400%)` does. Without
        // the animation it rests at the start of the track.
        let bar = div()
            .w(px(48.))
            .h_full()
            .bg(p.ac)
            .relative()
            .with_animation(
                "splash-bar",
                Animation::new(Duration::from_millis(1500))
                    .repeat()
                    .with_easing(ease_in_out),
                move |bar, delta| {
                    if frozen {
                        bar
                    } else {
                        bar.left(px(-48. + delta * 240.))
                    }
                },
            );

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(img("images/app-icon.svg").size_16().mb_8())
            .child(div().w_48().h_1().bg(p.border).overflow_hidden().child(bar))
    }

    fn render_nav_item(
        &self,
        number: &str,
        icon_name: &'static str,
        label: SharedString,
        path: &'static str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = palette(cx);

        div()
            .id(path)
            .transition_colors()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .type_xs()
            .border_l_2()
            .map(|item| {
                if active {
                    item.text_color(p.ac).border_color(p.ac).bg(p.surface)
                } else {
                    item.text_color(p.txt_secondary)
                        .border_color(gpui_kit::transparent_black())
                        .hover(|item| item.text_color(p.txt_primary).bg(p.surface))
                }
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.show_route(Route::from_path(path), window, cx);
            }))
            .child(icon(icon_name).size_4())
            .child(text(format!("{number} {label}")).tracking_wider())
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let route = self.route;

        div()
            .w(px(224.))
            .flex_shrink_0()
            .h_full()
            .bg(p.base)
            .border_r_1()
            .border_color(p.border)
            .flex()
            .flex_col()
            .font_mono()
            .child(
                div()
                    .flex_1()
                    .p_3()
                    .pt_9()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.render_nav_item(
                        "01",
                        "home",
                        t_upper(cx, "sidebar.home"),
                        "/",
                        route == Route::Home,
                        cx,
                    ))
                    .child(self.render_nav_item(
                        "02",
                        "book-open",
                        t_upper(cx, "sidebar.dictionary"),
                        "/dictionary",
                        matches!(route, Route::Dictionary(_)),
                        cx,
                    ))
                    .child(self.render_nav_item(
                        "03",
                        "wand-2",
                        t_upper(cx, "sidebar.style"),
                        "/style",
                        matches!(route, Route::Style(_)),
                        cx,
                    )),
            )
            .children(self.update_notification.clone())
            .child(
                div().p_3().border_t_1().border_color(p.border).child(
                    div()
                        .id("open-settings")
                        .transition_colors()
                        .flex()
                        .items_center()
                        .gap_3()
                        .w_full()
                        .px_3()
                        .py_2()
                        .type_xs()
                        .text_color(p.txt_secondary)
                        .hover(|button| button.text_color(p.txt_primary).bg(p.surface))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings(window, cx);
                        }))
                        .child(icon("settings").size_4().text_color(p.txt_secondary))
                        .child(text(upper(&crate::ui::t(cx, "sidebar.settings"))).tracking_wider()),
                ),
            )
    }

    fn render_app(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page.as_ref().map(|page| match page {
            Page::Home(page) => page.clone().into_any_element(),
            Page::About(page) => page.clone().into_any_element(),
            Page::Dictionary(page) => page.clone().into_any_element(),
            Page::Style(page) => page.clone().into_any_element(),
        });

        div()
            .flex()
            .size_full()
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .id("main")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .pt_6()
                    .children(page),
            )
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        let content = if !self.ready {
            self.render_splash(cx).into_any_element()
        } else if let Some(onboarding) = self.onboarding.clone() {
            onboarding.into_any_element()
        } else {
            self.render_app(cx).into_any_element()
        };

        div()
            .id("main-window")
            // The window changes theme over 150ms, as the original's root does.
            .transition_colors()
            .key_context("MainWindow")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                log::debug!(target: "app", "settings_shortcut_pressed");
                this.open_settings(window, cx);
            }))
            .relative()
            .size_full()
            .bg(p.base)
            .text_color(p.txt_primary)
            .font_sans()
            .type_base()
            .child(grid_reset(cx))
            .child(content)
            .child(grid_overlay(p.grid_line, cx))
            // The strip the window is dragged by, over the top of the content.
            .child(
                div()
                    .id("drag-region")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h_6()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |event, window, _| {
                        if event.click_count == 2 {
                            window.titlebar_double_click();
                        } else {
                            window.start_window_move();
                        }
                    }),
            )
            .children(self.settings_modal.clone())
    }
}

pub fn main_window_options(
    size_override: Option<(f32, f32)>,
    origin: Option<(f32, f32)>,
    cx: &App,
) -> WindowOptions {
    let (width, height) = size_override.unwrap_or(MAIN_WINDOW_SIZE);
    let window_size = size(px(width), px(height));
    let bounds = match origin {
        Some((x, y)) => Bounds::new(gpui_kit::point(px(x), px(y)), window_size),
        None => Bounds::centered(None, window_size, cx),
    };

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("VoxFusion".into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_min_size: Some(size(px(MAIN_WINDOW_MIN_SIZE.0), px(MAIN_WINDOW_MIN_SIZE.1))),
        app_id: Some(crate::paths::APP_IDENTIFIER.into()),
        window_decorations: Some(gpui_kit::WindowDecorations::Server),
        ..Default::default()
    }
}

/// Opens the main window showing `route`.
pub fn open_main_window(
    route: Route,
    options: WindowOptions,
    cx: &mut App,
) -> gpui_kit::Result<(gpui_kit::AnyWindowHandle, Entity<MainView>)> {
    gpui_kit::open_window(options, cx, move |window, cx| {
        window.set_rem_size(px(16.));
        theme::follow(window, cx);
        crate::platform::drawables::keep_two(&*window);

        let view = cx.new(|cx| MainView::new(route, window, cx));
        let focus_handle = view.focus_handle(cx);
        window.focus(&focus_handle, cx);
        view
    })
}
