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
use crate::ui::widgets::bulk_add::{BulkAddEvent, BulkAddPanel, bulk_add_trigger};
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

struct BulkAdd {
    panel: Entity<BulkAddPanel>,
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
    bulk_add: Option<BulkAdd>,
    bulk_trigger_focus: FocusHandle,
    adding: bool,
    /// Chains dictionary writes in user-action order so a bulk save cannot
    /// overtake an earlier delete or edit on another thread.
    write_tail: Option<async_channel::Receiver<()>>,
    /// Incremented on every local write so an earlier `fetch_words` snapshot
    /// cannot overwrite newer additions, edits, or deletions.
    fetch_generation: u64,
    /// Monotonic id of the latest `fetch_words` started, so an older slow
    /// refresh never overwrites a newer refresh.
    latest_fetch_id: u64,
    /// Number of backend writes still in flight.
    pending_writes: usize,
    /// Number of optimistic edits or deletions awaiting backend confirmation.
    pending_optimistic_writes: usize,
    /// Whether a failed optimistic write is waiting for a refresh to restore
    /// confirmed backend state before bulk submit can proceed.
    reconciling_after_failure: bool,
    /// Whether a fetch needs to run once `pending_writes` reaches zero.
    refetch_after_writes: bool,
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

        let mut page = Self {
            words: Vec::new(),
            loading: true,
            load_error: None,
            new_word,
            bulk_add: None,
            bulk_trigger_focus: cx.focus_handle().tab_stop(true),
            adding: false,
            write_tail: None,
            fetch_generation: 0,
            latest_fetch_id: 0,
            pending_writes: 0,
            pending_optimistic_writes: 0,
            reconciling_after_failure: false,
            refetch_after_writes: false,
            add_error: None,
            word_errors: HashMap::new(),
            editing: None,
            focus_handle: cx.focus_handle(),
            _subscription: subscription,
        };
        page.fetch_words(cx).detach();
        page
    }

    fn enqueue_write(
        &mut self,
    ) -> (
        Option<async_channel::Receiver<()>>,
        async_channel::Sender<()>,
    ) {
        let prev = self.write_tail.take();
        let (done_tx, done_rx) = async_channel::bounded(1);
        self.write_tail = Some(done_rx);
        (prev, done_tx)
    }

    fn has_unconfirmed_changes(&self) -> bool {
        self.pending_optimistic_writes > 0 || self.reconciling_after_failure
    }

    fn sync_bulk_submit_blocked(&self, cx: &mut Context<Self>) {
        if let Some(bulk) = &self.bulk_add {
            let blocked = self.has_unconfirmed_changes();
            bulk.panel
                .update(cx, |panel, cx| panel.set_submit_blocked(blocked, cx));
        }
    }

    fn sync_bulk_existing_words(&self, cx: &mut Context<Self>) {
        if let Some(bulk) = &self.bulk_add {
            let existing: Vec<String> = self.words.iter().map(|w| w.word.clone()).collect();
            bulk.panel
                .update(cx, |panel, cx| panel.set_existing_words(existing, cx));
        }
    }

    fn open_bulk_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.load_error.is_some() || self.adding || self.bulk_add.is_some() {
            return;
        }

        let existing: Vec<String> = self.words.iter().map(|w| w.word.clone()).collect();
        let submit_blocked = self.has_unconfirmed_changes();
        let panel = cx.new(|cx| {
            let mut panel = BulkAddPanel::new(Variant::Page, existing, window, cx);
            panel.set_submit_blocked(submit_blocked, cx);
            panel
        });
        let subscription =
            cx.subscribe_in(&panel, window, |this, _, event, window, cx| match event {
                BulkAddEvent::Submit(words) => this.bulk_add_words(words.clone(), window, cx),
                BulkAddEvent::Cancel => this.close_bulk_add(window, cx),
            });

        self.add_error = None;
        self.bulk_add = Some(BulkAdd {
            panel,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn close_bulk_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.adding {
            return;
        }
        let restore_focus = self
            .bulk_add
            .take()
            .is_some_and(|bulk| bulk.panel.read(cx).contains_focus(window, cx));
        if restore_focus {
            self.new_word
                .update(cx, |field, cx| field.focus(window, cx));
        }
        cx.notify();
    }

    fn bulk_add_words(&mut self, words: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        if words.is_empty() || self.adding || self.has_unconfirmed_changes() {
            return;
        }

        self.adding = true;
        if let Some(bulk) = &self.bulk_add {
            bulk.panel.update(cx, |panel, cx| {
                panel.set_error(None, cx);
                panel.set_saving(true, cx);
            });
        }
        cx.notify();

        self.fetch_generation += 1;
        self.pending_writes += 1;
        let (prev, done_tx) = self.enqueue_write();

        cx.spawn_in(window, async move |this, cx| {
            if let Some(prev) = prev {
                let _ = prev.recv().await;
            }
            let request = cx.update(|_, cx| {
                backend::call(cx, move |backend| backend.add_dictionary_words(&words))
            });
            let result = match request {
                Ok(request) => request.await,
                Err(_) => {
                    drop(done_tx);
                    return;
                }
            };

            let fetch = this.update_in(cx, |this, window, cx| {
                this.pending_writes = this.pending_writes.saturating_sub(1);
                this.adding = false;

                let fetch = match result {
                    Ok(added) => {
                        if !added.is_empty() {
                            analytics::capture(
                                cx,
                                "dictionary_words_bulk_added",
                                &[("count", (added.len() as i64).into())],
                            );
                            this.words.splice(0..0, added);
                        }
                        let restore_focus = this
                            .bulk_add
                            .take()
                            .is_some_and(|bulk| bulk.panel.read(cx).contains_focus(window, cx));
                        if restore_focus {
                            this.new_word
                                .update(cx, |field, cx| field.focus(window, cx));
                        }
                        if this.pending_writes == 0 {
                            this.refetch_after_writes = false;
                            Some(this.fetch_words(cx))
                        } else {
                            this.refetch_after_writes = true;
                            None
                        }
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "bulk_add_failed scope=Default error={error}");
                        let notice = Notice::new("dictionary.bulkAddFailed", error, cx);
                        if let Some(bulk) = &this.bulk_add {
                            bulk.panel.update(cx, |panel, cx| {
                                panel.set_saving(false, cx);
                                panel.set_error(Some(notice), cx);
                            });
                        } else {
                            this.add_error = Some(notice);
                        }
                        if this.pending_writes == 0
                            && std::mem::take(&mut this.refetch_after_writes)
                        {
                            Some(this.fetch_words(cx))
                        } else {
                            None
                        }
                    }
                };
                cx.notify();
                fetch
            });
            drop(done_tx);

            if let Ok(Some(fetch)) = fetch {
                fetch.await;
            }
        })
        .detach();
    }

    fn fetch_words(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.latest_fetch_id += 1;
        let fetch_id = self.latest_fetch_id;
        let generation = self.fetch_generation;
        let request = backend::call(cx, |backend| backend.list_dictionary_words());

        cx.spawn(async move |this, cx| {
            let result = request.await;

            this.update(cx, |this, cx| {
                if fetch_id != this.latest_fetch_id {
                    return;
                }
                if generation != this.fetch_generation || this.pending_writes > 0 {
                    if this.pending_writes > 0 {
                        this.refetch_after_writes = true;
                    } else {
                        this.fetch_words(cx).detach();
                    }
                    return;
                }

                this.loading = false;
                match result {
                    Ok(words) => {
                        this.words = words;
                        this.load_error = None;
                        this.reconciling_after_failure = false;
                        this.sync_bulk_existing_words(cx);
                        this.sync_bulk_submit_blocked(cx);
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

        self.fetch_generation += 1;
        self.pending_writes += 1;
        let (prev, done_tx) = self.enqueue_write();

        cx.spawn_in(window, async move |this, cx| {
            if let Some(prev) = prev {
                let _ = prev.recv().await;
            }
            let request = cx.update(|_, cx| {
                backend::call(cx, move |backend| backend.add_dictionary_word(&word))
            });
            let result = match request {
                Ok(request) => request.await,
                Err(_) => {
                    drop(done_tx);
                    return;
                }
            };

            let fetch = this.update_in(cx, |this, window, cx| {
                this.pending_writes = this.pending_writes.saturating_sub(1);
                this.adding = false;

                let fetch = match result {
                    Ok(added) => {
                        analytics::capture(cx, "dictionary_word_added", &[]);
                        this.words.insert(0, added);
                        this.sync_bulk_existing_words(cx);
                        this.new_word
                            .update(cx, |field, cx| field.set_value("", window, cx));
                        if this.pending_writes == 0 {
                            this.refetch_after_writes = false;
                            Some(this.fetch_words(cx))
                        } else {
                            this.refetch_after_writes = true;
                            None
                        }
                    }
                    // What was typed stays in the field, to be corrected or sent
                    // again.
                    Err(error) => {
                        log::error!(target: "dictionary", "add_failed scope=Default error={error}");
                        this.add_error = Some(Notice::new("dictionary.addFailed", error, cx));
                        if this.pending_writes == 0
                            && std::mem::take(&mut this.refetch_after_writes)
                        {
                            Some(this.fetch_words(cx))
                        } else {
                            None
                        }
                    }
                };
                cx.notify();
                fetch
            });
            drop(done_tx);

            if let Ok(Some(fetch)) = fetch {
                fetch.await;
            }
        })
        .detach();
    }

    /// Stores a change to the word `id` that the list already shows. If it
    /// is refused, the stored words are fetched again, and the word says why
    /// with the notice titled by `failure`.
    fn persist<T: Send + 'static>(
        &mut self,
        id: String,
        failure: &'static str,
        change: impl FnOnce(&dyn Backend) -> CommandResult<T> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.fetch_generation += 1;
        self.pending_writes += 1;
        self.pending_optimistic_writes += 1;
        self.sync_bulk_submit_blocked(cx);
        let (prev, done_tx) = self.enqueue_write();

        cx.spawn(async move |this, cx| {
            if let Some(prev) = prev {
                let _ = prev.recv().await;
            }
            let request = cx.update(|cx| backend::call(cx, change));
            let result = request.await;

            let fetch = this.update(cx, |this, cx| {
                this.pending_writes = this.pending_writes.saturating_sub(1);
                this.pending_optimistic_writes = this.pending_optimistic_writes.saturating_sub(1);
                let fetch = match result {
                    Ok(_) => {
                        if this.pending_writes == 0
                            && std::mem::take(&mut this.refetch_after_writes)
                        {
                            Some(this.fetch_words(cx))
                        } else {
                            None
                        }
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "change_failed scope=Default error={error}");
                        this.reconciling_after_failure = true;
                        this.word_errors.insert(id, Notice::new(failure, error, cx));
                        if this.pending_writes == 0 {
                            this.refetch_after_writes = false;
                            Some(this.fetch_words(cx))
                        } else {
                            this.refetch_after_writes = true;
                            None
                        }
                    }
                };
                this.sync_bulk_submit_blocked(cx);
                fetch
            });
            drop(done_tx);

            if let Ok(Some(fetch)) = fetch {
                fetch.await;
            }
        })
        .detach();
    }

    fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        analytics::capture(cx, "dictionary_word_deleted", &[]);
        self.words.retain(|word| word.id != id);
        self.word_errors.remove(id);
        self.sync_bulk_existing_words(cx);
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
        self.sync_bulk_existing_words(cx);
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

        let entry_card = match &self.bulk_add {
            Some(bulk) => bulk.panel.clone().into_any_element(),
            None => {
                let form = add_word_form(
                    Variant::Page,
                    &self.new_word,
                    self.adding,
                    self.add_error.as_ref(),
                    cx.listener(|this, _, window, cx| this.add(window, cx)),
                    window,
                    cx,
                );
                let trigger = bulk_add_trigger(
                    Variant::Page,
                    &self.bulk_trigger_focus,
                    !loaded || self.adding,
                    cx.listener(|this, _, window, cx| this.open_bulk_add(window, cx)),
                    window,
                    cx,
                );

                div().child(form).child(trigger).into_any_element()
            }
        };

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
                    .child(entry_card),
            )
            .children(list)
    }
}
