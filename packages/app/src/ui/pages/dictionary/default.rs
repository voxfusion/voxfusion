//! The default dictionary: the words used everywhere.

use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Task, Window, div,
};
use std::collections::HashMap;

use crate::analytics;
use crate::backend::{self, Backend, CommandResult, DictionaryWord};
use crate::ui::pages::section::{empty_state, list_header, load_failed};
use crate::ui::t;
use crate::ui::theme::palette;
use crate::ui::widgets::text_field::{TextField, TextFieldEvent};
use crate::ui::widgets::word_list::{
    Notice, Variant, WordAction, add_word_form, new_word_field, too_long, word_count_label,
    word_row,
};

struct Editing {
    word_id: String,
    field: Entity<TextField>,
    _subscription: Subscription,
}

pub struct DefaultDictionary {
    words: Vec<DictionaryWord>,
    /// Whether the words are on their way.
    loading: bool,
    /// Why the words could not be listed. The list is not shown then: it
    /// would look empty, or out of date.
    load_error: Option<SharedString>,
    new_word: Entity<TextField>,
    adding: bool,
    /// Why the last word could not be added.
    add_error: Option<Notice>,
    /// Why a change to a word was refused, by the word's id.
    word_errors: HashMap<String, Notice>,
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
                    TextFieldEvent::Change => {
                        this.add_error = None;
                        cx.notify();
                    }
                    TextFieldEvent::Enter => this.add(window, cx),
                    _ => {}
                },
            );

        let page = Self {
            words: Vec::new(),
            loading: true,
            load_error: None,
            new_word,
            adding: false,
            add_error: None,
            word_errors: HashMap::new(),
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
            let result = request.await;

            this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(words) => {
                        this.words = words;
                        this.load_error = None;
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "list_failed scope=Default error={error}");
                        this.load_error = Some(error.into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
    }

    /// Lists the words again, after they could not be listed.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.load_error = None;
        cx.notify();
        self.fetch_words(cx).detach();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.new_word.read(cx).value(cx).trim().to_string();
        // A word that is too long has its warning under the field already.
        if word.is_empty() || too_long(&word) || self.adding {
            return;
        }

        self.adding = true;
        self.add_error = None;
        cx.notify();

        let request = backend::call(cx, move |backend| backend.add_dictionary_word(&word));

        cx.spawn_in(window, async move |this, cx| {
            match request.await {
                Ok(_) => {
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
                // What was typed stays in the field, to be corrected or sent
                // again.
                Err(error) => {
                    log::error!(target: "dictionary", "add_failed scope=Default error={error}");
                    this.update(cx, |this, cx| {
                        this.add_error = Some(Notice::new("dictionary.addFailed", error, cx));
                    })
                    .ok();
                }
            }

            this.update(cx, |this, cx| {
                this.adding = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Stores a change to the word `id` that the list already shows. If it
    /// is refused, the stored words are fetched again, and the word says why
    /// with the notice titled by `failure`.
    fn persist<T: Send + 'static>(
        &self,
        id: String,
        failure: &'static str,
        change: impl FnOnce(&dyn Backend) -> CommandResult<T> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let request = backend::call(cx, change);

        cx.spawn(async move |this, cx| {
            let Err(error) = request.await else {
                return;
            };
            log::error!(target: "dictionary", "change_failed scope=Default error={error}");

            let fetch = this.update(cx, |this, cx| {
                this.word_errors.insert(id, Notice::new(failure, error, cx));
                this.fetch_words(cx)
            });
            if let Ok(fetch) = fetch {
                fetch.await;
            }
        })
        .detach();
    }

    fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        analytics::capture(cx, "dictionary_word_deleted", &[]);
        self.words.retain(|word| word.id != id);
        self.word_errors.remove(id);
        cx.notify();

        let id = id.to_string();
        self.persist(
            id.clone(),
            "dictionary.deleteFailed",
            move |backend| backend.delete_dictionary_word(&id),
            cx,
        );
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
                TextFieldEvent::Change => cx.notify(),
                TextFieldEvent::Enter => this.save_edit(&word_id, window, cx),
                TextFieldEvent::Escape => this.cancel_edit(window, cx),
                _ => {}
            }
        });

        self.word_errors.remove(&word.id);
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
        if text.is_empty() || too_long(&text) {
            return;
        }

        analytics::capture(cx, "dictionary_word_edited", &[]);
        if let Some(word) = self.words.iter_mut().find(|word| word.id == id) {
            word.word = text.clone();
        }
        self.cancel_edit(window, cx);

        let id = id.to_string();
        self.persist(
            id.clone(),
            "dictionary.saveFailed",
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
        let loaded = !self.loading && self.load_error.is_none();
        let count =
            (loaded && !self.words.is_empty()).then(|| word_count_label(self.words.len(), cx));

        let form = add_word_form(
            Variant::Page,
            &self.new_word,
            self.adding,
            self.add_error.as_ref(),
            cx.listener(|this, _, window, cx| this.add(window, cx)),
            window,
            cx,
        );

        let list = if self.loading {
            // Nothing yet, rather than an empty list the words then replace.
            None
        } else if let Some(error) = self.load_error.clone() {
            Some(
                load_failed(
                    t(cx, "dictionary.loadFailed"),
                    error,
                    cx.listener(|this, _, _, cx| this.reload(cx)),
                    cx,
                )
                .into_any_element(),
            )
        } else if self.words.is_empty() {
            Some(
                empty_state(
                    "NO_TERMS_FOUND",
                    t(cx, "dictionary.emptyState"),
                    t(cx, "dictionary.emptyStateDescription"),
                    cx,
                )
                .into_any_element(),
            )
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
                    self.word_errors.get(&word.id),
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

            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows)
                    .into_any_element(),
            )
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
            .children(list)
    }
}
