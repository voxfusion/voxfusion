//! The default dictionary: the words used everywhere.

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString, Styled as _, Subscription,
    Task, Window, div,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::analytics;
use crate::backend::{Backend, CommandResult, DictionaryWord};
use crate::ui::pages::section::{empty_state, list_header, load_failed};
use crate::ui::t;
use crate::ui::theme::palette;
use crate::ui::widgets::text_field::{TextField, TextFieldEvent};
use crate::ui::widgets::word_list::{
    Notice, UNDO_DURATION, UndoAction, UndoBanner, UndoFocus, Variant, WordAction, add_word_form,
    new_word_field, too_long, undo_row, word_count_label, word_row,
};

use super::ops::{
    self, DeleteTicket, QueueScope, RestoreEvent, RestoreOutcome, RestoreState, RestoreStore,
    RestoreTarget,
};

struct Editing {
    word_id: String,
    field: Entity<TextField>,
    _subscription: Subscription,
}

/// An unacted deletion waiting for Undo, Dismiss, or [`UNDO_DURATION`].
/// Once the user clicks Undo, ownership moves into [`RestoreStore`] so the
/// restore and any retryable error survive tab switches.
struct LocalUndo {
    token: u64,
    word: DictionaryWord,
    index: usize,
    ticket: DeleteTicket,
    restore_requested: Rc<Cell<bool>>,
    ready: bool,
    _timer: Option<Task<()>>,
}

struct RenderedUndo {
    token: u64,
    index: usize,
    banner: UndoBanner,
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
    /// Per-word mutation generation so a failed edit whose word was already
    /// deleted, re-edited, or restored does not attach a stale error notice.
    word_mutations: HashMap<String, u64>,
    mutation_seq: u64,
    /// Word ids whose delete request is still in flight, mapped to their Undo
    /// token, so a concurrent fetch does not put them back prematurely.
    deleting: HashMap<String, u64>,
    /// Incremented on local list mutations so a fetch started earlier reloads
    /// instead of overwriting newer state.
    revision: u64,
    /// Identifies the newest fetch so older responses are ignored.
    fetch_seq: u64,
    local_undo: Option<LocalUndo>,
    /// Per-token `(undo, dismiss)` focus handles kept stable across phase
    /// transitions (local Undo -> restoring -> failed retry).
    undo_focus: HashMap<u64, (FocusHandle, FocusHandle)>,
    restore_store: Entity<RestoreStore>,
    editing: Option<Editing>,
    /// Set for the duration of an Escape keypress that cancelled an active
    /// inline edit so the bubbled keydown does not also dismiss an Undo row.
    escape_cancelled_edit: bool,
    focus_handle: FocusHandle,
    _subscriptions: [Subscription; 3],
}

impl DefaultDictionary {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/dictionary".into())]);

        let new_word = new_word_field(window, cx);
        let restore_store = ops::store(cx);
        let subscriptions = [
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
            ),
            cx.observe_in(&restore_store, window, |this, _, window, cx| {
                this.sync_undo_focus(UndoFocus::Keep, window, cx);
                cx.notify();
            }),
            cx.subscribe_in(&restore_store, window, |this, _, event, window, cx| {
                this.handle_restore_event(event, window, cx);
            }),
        ];

        let mut page = Self {
            words: Vec::new(),
            loading: true,
            load_error: None,
            new_word,
            adding: false,
            add_error: None,
            word_errors: HashMap::new(),
            word_mutations: HashMap::new(),
            mutation_seq: 0,
            deleting: HashMap::new(),
            revision: 0,
            fetch_seq: 0,
            local_undo: None,
            undo_focus: HashMap::new(),
            restore_store,
            editing: None,
            escape_cancelled_edit: false,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        page.sync_undo_focus(UndoFocus::Keep, window, cx);
        page.fetch_words(cx).detach();
        page
    }

    fn bump_word_mutation(&mut self, word_id: &str) -> u64 {
        self.mutation_seq += 1;
        let seq = self.mutation_seq;
        self.word_mutations.insert(word_id.to_string(), seq);
        seq
    }

    fn clear_deleting(&mut self, word_id: &str, token: u64) {
        if self.deleting.get(word_id) == Some(&token) {
            self.deleting.remove(word_id);
        }
    }

    fn handle_restore_event(
        &mut self,
        event: &RestoreEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RestoreEvent::Settled {
                scope: QueueScope::Default,
                token,
                word_id,
                outcome,
                ..
            } => {
                // Clear the in-flight delete marker before fetching so a failed
                // delete followed by Undo never filters out the saved word, and
                // report the delete failure if the preceding delete failed.
                self.clear_deleting(word_id, *token);
                self.bump_word_mutation(word_id);
                match outcome {
                    RestoreOutcome::DeleteFailed(error) => {
                        self.word_errors.insert(
                            word_id.clone(),
                            Notice::new("dictionary.deleteFailed", error.clone(), cx),
                        );
                    }
                    RestoreOutcome::Restored
                    | RestoreOutcome::NotFound
                    | RestoreOutcome::Cancelled => {
                        self.word_errors.remove(word_id);
                    }
                }
                self.sync_undo_focus(UndoFocus::Keep, window, cx);
                self.fetch_words(cx).detach();
            }
            RestoreEvent::Failed {
                scope: QueueScope::Default,
                token,
                word_id,
            } => {
                self.clear_deleting(word_id, *token);
                self.sync_undo_focus(UndoFocus::Keep, window, cx);
                cx.notify();
            }
            _ => {}
        }
    }

    fn active_tokens(&self, cx: &App) -> Vec<u64> {
        let mut tokens: Vec<u64> = self
            .restore_store
            .read(cx)
            .entries_for(QueueScope::Default)
            .into_iter()
            .map(|entry| entry.token)
            .collect();
        if let Some(local) = &self.local_undo {
            tokens.push(local.token);
        }
        tokens
    }

    fn sync_undo_focus(&mut self, focus: UndoFocus, window: &mut Window, cx: &mut Context<Self>) {
        let tokens = self.active_tokens(cx);
        let mut removed_had_focus = false;
        self.undo_focus
            .retain(|token, (undo_focus, dismiss_focus)| {
                let keep = tokens.contains(token);
                if !keep && (undo_focus.is_focused(window) || dismiss_focus.is_focused(window)) {
                    removed_had_focus = true;
                }
                keep
            });
        for token in tokens {
            self.undo_focus.entry(token).or_insert_with(|| {
                (
                    cx.focus_handle().tab_stop(true),
                    cx.focus_handle().tab_stop(true),
                )
            });
        }

        match focus {
            UndoFocus::Button(token) => {
                if let Some((undo_focus, _)) = self.undo_focus.get(&token) {
                    window.focus(undo_focus, cx);
                }
            }
            UndoFocus::Container => {
                if self.editing.is_none() {
                    window.focus(&self.focus_handle, cx);
                }
            }
            UndoFocus::Keep => {
                if removed_had_focus {
                    window.focus(&self.focus_handle, cx);
                }
            }
        }
    }

    fn collect_undos(&self, cx: &App) -> Vec<RenderedUndo> {
        let mut undos: Vec<RenderedUndo> = self
            .restore_store
            .read(cx)
            .entries_for(QueueScope::Default)
            .into_iter()
            .map(|entry| {
                let (restoring, error) = match entry.state {
                    RestoreState::Restoring => (true, None),
                    RestoreState::Failed(error) => {
                        (false, Some(Notice::new("dictionary.undoFailed", error, cx)))
                    }
                };
                RenderedUndo {
                    token: entry.token,
                    index: entry.index,
                    banner: UndoBanner {
                        word: entry.word.word.into(),
                        restoring,
                        error,
                    },
                }
            })
            .collect();

        if let Some(local) = &self.local_undo {
            undos.push(RenderedUndo {
                token: local.token,
                index: local.index,
                banner: UndoBanner {
                    word: local.word.word.clone().into(),
                    restoring: false,
                    error: None,
                },
            });
        }
        undos
    }

    fn fetch_words(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.fetch_seq += 1;
        let seq = self.fetch_seq;
        let rev = self.revision;
        let request = ops::run(QueueScope::Default, cx, |backend| {
            backend.list_dictionary_words()
        });

        cx.spawn(async move |this, cx| {
            let result = request.await;

            let refetch = this.update(cx, |this, cx| {
                if seq != this.fetch_seq {
                    return None;
                }
                if rev != this.revision {
                    return Some(this.fetch_words(cx));
                }

                this.loading = false;
                match result {
                    Ok(mut words) => {
                        words.retain(|word| !this.deleting.contains_key(&word.id));
                        this.words = words;
                        this.load_error = None;
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "list_failed scope=Default error={error}");
                        this.load_error = Some(error.into());
                    }
                }
                cx.notify();
                None
            });
            if let Ok(Some(task)) = refetch {
                task.await;
            }
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

        let request = ops::run(QueueScope::Default, cx, move |backend| {
            backend.add_dictionary_word(&word)
        });

        cx.spawn_in(window, async move |this, cx| {
            match request.await {
                Ok(_) => {
                    let fetch = this.update(cx, |this, cx| {
                        analytics::capture(cx, "dictionary_word_added", &[]);
                        if let Some(local) = &mut this.local_undo {
                            local.index += 1;
                        }
                        ops::shift_indices_on_add(QueueScope::Default, None, cx);
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
        mutation: u64,
        failure: &'static str,
        change: impl FnOnce(&dyn Backend) -> CommandResult<T> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let request = ops::run(QueueScope::Default, cx, change);

        cx.spawn(async move |this, cx| {
            let Err(error) = request.await else {
                return;
            };
            log::error!(target: "dictionary", "change_failed scope=Default error={error}");

            let fetch = this.update(cx, |this, cx| {
                if this.word_mutations.get(&id) == Some(&mutation) {
                    this.word_errors.insert(id, Notice::new(failure, error, cx));
                }
                this.fetch_words(cx)
            });
            if let Ok(fetch) = fetch {
                fetch.await;
            }
        })
        .detach();
    }

    fn spawn_undo_timer(
        &self,
        token: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(UNDO_DURATION).await;
            this.update_in(cx, |this, window, cx| {
                if this
                    .local_undo
                    .as_ref()
                    .is_some_and(|local| local.token == token)
                {
                    this.local_undo = None;
                    this.sync_undo_focus(UndoFocus::Keep, window, cx);
                    cx.notify();
                }
            })
            .ok();
        })
    }

    fn delete(
        &mut self,
        id: &str,
        from_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.words.iter().position(|word| word.id == id) else {
            return;
        };
        let deleted = self.words.remove(index);
        if self
            .editing
            .as_ref()
            .is_some_and(|editing| editing.word_id == deleted.id)
        {
            self.editing = None;
        }

        ops::shift_indices_on_delete(QueueScope::Default, None, index, cx);

        analytics::capture(cx, "dictionary_word_deleted", &[]);
        self.word_errors.remove(&deleted.id);
        self.bump_word_mutation(&deleted.id);
        self.revision += 1;
        let token = ops::next_token(cx);
        let ticket = DeleteTicket::new();
        let restore_requested = Rc::new(Cell::new(false));
        self.deleting.insert(deleted.id.clone(), token);

        self.local_undo = Some(LocalUndo {
            token,
            word: deleted.clone(),
            index,
            ticket: ticket.clone(),
            restore_requested: restore_requested.clone(),
            ready: false,
            _timer: None,
        });
        self.sync_undo_focus(
            if from_keyboard {
                UndoFocus::Button(token)
            } else {
                UndoFocus::Container
            },
            window,
            cx,
        );
        cx.notify();

        let word_id = deleted.id.clone();
        let request = ops::run(QueueScope::Default, cx, {
            let word_id = word_id.clone();
            let ticket = ticket.clone();
            move |backend| {
                let result = backend.delete_dictionary_word(&word_id);
                match &result {
                    Ok(Some(saved)) => ticket.record_deleted(saved.clone()),
                    Err(error) => ticket.record_failed(error.clone()),
                    Ok(None) => {}
                }
                result
            }
        });

        cx.spawn_in(window, async move |this, cx| match request.await {
            Ok(Some(saved)) => {
                this.update_in(cx, |this, window, cx| {
                    this.clear_deleting(&word_id, token);
                    ops::sync_deleted_word(token, saved.clone(), cx);
                    if this
                        .local_undo
                        .as_ref()
                        .is_some_and(|local| local.token == token)
                    {
                        let needs_timer =
                            this.local_undo.as_ref().is_some_and(|local| !local.ready);
                        let timer = needs_timer.then(|| this.spawn_undo_timer(token, window, cx));
                        if let Some(local) = this.local_undo.as_mut() {
                            local.word = saved;
                            if let Some(timer) = timer {
                                local.ready = true;
                                local._timer = Some(timer);
                            }
                        }
                        cx.notify();
                    }
                })
                .ok();
            }
            Ok(None) => {
                let fetch = this.update_in(cx, |this, window, cx| {
                    this.clear_deleting(&word_id, token);
                    let requested = restore_requested.get();
                    if this
                        .local_undo
                        .as_ref()
                        .is_some_and(|local| local.token == token)
                    {
                        this.local_undo = None;
                        this.sync_undo_focus(UndoFocus::Keep, window, cx);
                        cx.notify();
                    }
                    (!requested).then(|| this.fetch_words(cx))
                });
                if let Ok(Some(fetch)) = fetch {
                    fetch.await;
                }
            }
            Err(error) => {
                log::error!(target: "dictionary", "change_failed scope=Default error={error}");
                let requested = restore_requested.get();
                let fetch = this.update_in(cx, |this, window, cx| {
                    this.clear_deleting(&word_id, token);
                    if !requested {
                        if this
                            .local_undo
                            .as_ref()
                            .is_some_and(|local| local.token == token)
                        {
                            this.local_undo = None;
                            this.sync_undo_focus(UndoFocus::Keep, window, cx);
                        }
                        this.word_errors
                            .insert(word_id, Notice::new("dictionary.deleteFailed", error, cx));
                        Some(this.fetch_words(cx))
                    } else {
                        None
                    }
                });
                if let Ok(Some(fetch)) = fetch {
                    fetch.await;
                }
            }
        })
        .detach();
    }

    fn undo_delete(&mut self, token: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .local_undo
            .as_ref()
            .is_some_and(|local| local.token == token)
        {
            let local = self.local_undo.take().expect("checked above");
            local.restore_requested.set(true);
            ops::start_restore(
                local.token,
                QueueScope::Default,
                RestoreTarget::Default,
                local.word,
                local.index,
                local.ticket,
                cx,
            );
            self.sync_undo_focus(UndoFocus::Keep, window, cx);
            cx.notify();
            return;
        }

        ops::retry_restore(token, cx);
    }

    fn dismiss_undo(&mut self, token: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .local_undo
            .as_ref()
            .is_some_and(|local| local.token == token)
        {
            self.local_undo = None;
            self.sync_undo_focus(UndoFocus::Keep, window, cx);
            cx.notify();
            return;
        }

        ops::dismiss_restore(token, cx);
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key != "escape" {
            return;
        }
        if self.escape_cancelled_edit {
            self.escape_cancelled_edit = false;
            return;
        }
        if self.editing.is_some() {
            return;
        }
        if let Some(token) = self.local_undo.as_ref().map(|local| local.token) {
            self.dismiss_undo(token, window, cx);
            cx.stop_propagation();
            return;
        }
        let failed_token = self
            .restore_store
            .read(cx)
            .entries_for(QueueScope::Default)
            .into_iter()
            .rev()
            .find(|entry| matches!(entry.state, RestoreState::Failed(_)))
            .map(|entry| entry.token);
        if let Some(token) = failed_token {
            self.dismiss_undo(token, window, cx);
            cx.stop_propagation();
        }
    }

    fn handle_undo_action(
        &mut self,
        token: u64,
        action: UndoAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            UndoAction::Undo => self.undo_delete(token, window, cx),
            UndoAction::Dismiss => self.dismiss_undo(token, window, cx),
        }
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
                TextFieldEvent::Escape => {
                    this.escape_cancelled_edit = true;
                    let entity = cx.entity().downgrade();
                    window.defer(cx, move |_, cx| {
                        entity
                            .update(cx, |this, _| this.escape_cancelled_edit = false)
                            .ok();
                    });
                    this.cancel_edit(window, cx);
                }
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
        self.revision += 1;
        let mutation = self.bump_word_mutation(id);
        if let Some(word) = self.words.iter_mut().find(|word| word.id == id) {
            word.word = text.clone();
        }
        self.cancel_edit(window, cx);

        let id = id.to_string();
        self.persist(
            id.clone(),
            mutation,
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
            WordAction::Delete { from_keyboard } => {
                self.delete(&word.id, from_keyboard, window, cx)
            }
        }
    }

    fn render_undo(
        &self,
        undo: &RenderedUndo,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let token = undo.token;
        let Some((undo_focus, dismiss_focus)) = self.undo_focus.get(&token) else {
            return div().into_any_element();
        };
        let on_action = cx.listener(move |this, action: &UndoAction, window, cx| {
            this.handle_undo_action(token, *action, window, cx);
        });
        undo_row(
            Variant::Page,
            SharedString::from(format!("undo-{token}")),
            &undo.banner,
            undo_focus,
            dismiss_focus,
            on_action,
            window,
            cx,
        )
        .into_any_element()
    }
}

impl Render for DefaultDictionary {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let loaded = !self.loading && self.load_error.is_none();
        let count =
            (loaded && !self.words.is_empty()).then(|| word_count_label(self.words.len(), cx));
        let undos = self.collect_undos(cx);

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
            let banners: Vec<AnyElement> = undos
                .iter()
                .map(|undo| self.render_undo(undo, window, cx))
                .collect();
            let failed = load_failed(
                t(cx, "dictionary.loadFailed"),
                error,
                cx.listener(|this, _, _, cx| this.reload(cx)),
                cx,
            );
            Some(if banners.is_empty() {
                failed.into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(div().flex().flex_col().gap_1().children(banners))
                    .child(failed)
                    .into_any_element()
            })
        } else if self.words.is_empty() {
            let banners: Vec<AnyElement> = undos
                .iter()
                .map(|undo| self.render_undo(undo, window, cx))
                .collect();
            let empty = empty_state(
                "NO_TERMS_FOUND",
                t(cx, "dictionary.emptyState"),
                t(cx, "dictionary.emptyStateDescription"),
                cx,
            );
            Some(if banners.is_empty() {
                empty.into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(div().flex().flex_col().gap_1().children(banners))
                    .child(empty)
                    .into_any_element()
            })
        } else {
            let mut rows: Vec<AnyElement> = Vec::with_capacity(self.words.len() + undos.len());

            for pos in 0..=self.words.len() {
                for undo in &undos {
                    if undo.index.min(self.words.len()) == pos {
                        rows.push(self.render_undo(undo, window, cx));
                    }
                }
                if let Some(word) = self.words.get(pos) {
                    let editing = self
                        .editing
                        .as_ref()
                        .filter(|editing| editing.word_id == word.id)
                        .map(|editing| &editing.field);
                    let on_action = cx.listener({
                        let word = word.clone();
                        move |this, action: &WordAction, window, cx| {
                            this.handle_word_action(&word, *action, window, cx);
                        }
                    });

                    rows.push(
                        word_row(
                            Variant::Page,
                            word,
                            editing,
                            self.word_errors.get(&word.id),
                            on_action,
                            window,
                            cx,
                        )
                        .into_any_element(),
                    );
                }
            }

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
            .on_key_down(cx.listener(Self::handle_key_down))
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
