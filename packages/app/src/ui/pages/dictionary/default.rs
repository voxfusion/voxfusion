//! The default dictionary: the words used everywhere.

use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Task, Window, div,
};

use crate::analytics;
use crate::backend::{self, Backend, CommandResult, DictionaryWord};
use crate::ui::pages::section::{empty_state, list_header};
use crate::ui::t;
use crate::ui::theme::palette;
use crate::ui::widgets::text_field::{TextField, TextFieldEvent};
use crate::ui::widgets::word_list::{
    Variant, WordAction, add_word_form, new_word_field, word_count_label, word_row,
};

struct Editing {
    word_id: String,
    field: Entity<TextField>,
    _subscription: Subscription,
}

pub struct DefaultDictionary {
    words: Vec<DictionaryWord>,
    new_word: Entity<TextField>,
    adding: bool,
    editing: Option<Editing>,
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl DefaultDictionary {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/dictionary".into())]);

        let new_word = new_word_field(window, cx);
        let subscription =
            cx.subscribe_in(
                &new_word,
                window,
                |this, _, event, window, cx| match event {
                    TextFieldEvent::Change => cx.notify(),
                    TextFieldEvent::Enter => this.add(window, cx),
                    _ => {}
                },
            );

        let page = Self {
            words: Vec::new(),
            new_word,
            adding: false,
            editing: None,
            focus_handle: cx.focus_handle(),
            _subscription: subscription,
        };
        page.fetch_words(cx).detach();
        page
    }

    fn fetch_words(&self, cx: &mut Context<Self>) -> Task<()> {
        let request = backend::call(cx, |backend| backend.list_dictionary_words());

        cx.spawn(async move |this, cx| {
            if let Ok(words) = request.await {
                this.update(cx, |this, cx| {
                    this.words = words;
                    cx.notify();
                })
                .ok();
            }
        })
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.new_word.read(cx).value(cx).trim().to_string();
        if word.is_empty() || self.adding {
            return;
        }

        self.adding = true;
        cx.notify();

        let request = backend::call(cx, move |backend| backend.add_dictionary_word(&word));

        cx.spawn_in(window, async move |this, cx| {
            if request.await.is_ok() {
                let fetch = this.update(cx, |this, cx| {
                    analytics::capture(cx, "dictionary_word_added", &[]);
                    this.fetch_words(cx)
                });
                if let Ok(fetch) = fetch {
                    fetch.await;
                }

                this.update_in(cx, |this, window, cx| {
                    this.new_word
                        .update(cx, |field, cx| field.set_value("", window, cx));
                })
                .ok();
            }

            this.update(cx, |this, cx| {
                this.adding = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Stores a change the list already shows. If it is refused, the stored
    /// words are fetched again.
    fn persist<T: Send + 'static>(
        &self,
        change: impl FnOnce(&dyn Backend) -> CommandResult<T> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let request = backend::call(cx, change);

        cx.spawn(async move |this, cx| {
            if request.await.is_err()
                && let Ok(fetch) = this.update(cx, |this, cx| this.fetch_words(cx))
            {
                fetch.await;
            }
        })
        .detach();
    }

    fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        analytics::capture(cx, "dictionary_word_deleted", &[]);
        self.words.retain(|word| word.id != id);
        cx.notify();

        let id = id.to_string();
        self.persist(move |backend| backend.delete_dictionary_word(&id), cx);
    }

    fn start_edit(&mut self, word: &DictionaryWord, window: &mut Window, cx: &mut Context<Self>) {
        let field = cx.new(|cx| {
            let mut field = TextField::new(window, cx);
            field.set_value(word.word.clone(), window, cx);
            field.focus(window, cx);
            field
        });

        let subscription = cx.subscribe_in(&field, window, {
            let word_id = word.id.clone();
            move |this, _, event, window, cx| match event {
                TextFieldEvent::Enter => this.save_edit(&word_id, window, cx),
                TextFieldEvent::Escape => this.cancel_edit(window, cx),
                _ => {}
            }
        });

        self.editing = Some(Editing {
            word_id: word.id.clone(),
            field,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        // The field that held the keyboard focus is gone.
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn save_edit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = &self.editing else {
            return;
        };
        let text = editing.field.read(cx).value(cx).trim().to_string();
        if text.is_empty() {
            return;
        }

        analytics::capture(cx, "dictionary_word_edited", &[]);
        if let Some(word) = self.words.iter_mut().find(|word| word.id == id) {
            word.word = text.clone();
        }
        self.cancel_edit(window, cx);

        let id = id.to_string();
        self.persist(
            move |backend| backend.update_dictionary_word(&id, &text),
            cx,
        );
    }

    fn handle_word_action(
        &mut self,
        word: &DictionaryWord,
        action: WordAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            WordAction::StartEdit => self.start_edit(word, window, cx),
            WordAction::SaveEdit => self.save_edit(&word.id, window, cx),
            WordAction::CancelEdit => self.cancel_edit(window, cx),
            WordAction::Delete => self.delete(&word.id, cx),
        }
    }
}

impl Render for DefaultDictionary {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let count = (!self.words.is_empty()).then(|| word_count_label(self.words.len(), cx));

        let form = add_word_form(
            Variant::Page,
            &self.new_word,
            self.adding,
            cx.listener(|this, _, window, cx| this.add(window, cx)),
            window,
            cx,
        );

        let list = if self.words.is_empty() {
            empty_state(
                "NO_TERMS_FOUND",
                t(cx, "dictionary.emptyState"),
                t(cx, "dictionary.emptyStateDescription"),
                cx,
            )
            .into_any_element()
        } else {
            let rows = self.words.iter().map(|word| {
                let editing = self
                    .editing
                    .as_ref()
                    .filter(|editing| editing.word_id == word.id)
                    .map(|editing| &editing.field);

                word_row(
                    Variant::Page,
                    word,
                    editing,
                    cx.listener({
                        let word = word.clone();
                        move |this, action: &WordAction, window, cx| {
                            this.handle_word_action(&word, *action, window, cx);
                        }
                    }),
                    window,
                    cx,
                )
            });

            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(rows)
                .into_any_element()
        };

        div()
            .track_focus(&self.focus_handle)
            .child(list_header(None, count, cx))
            .child(
                div()
                    .mb_6()
                    .p_4()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .child(form),
            )
            .child(list)
    }
}
