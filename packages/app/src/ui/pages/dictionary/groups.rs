//! The dictionaries of the apps or of the sites: the stored ones, the ones
//! just added that have no words yet, and the editor that shows them all.

use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, Render, SharedString, Subscription, Task,
    Window,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::analytics;
use crate::backend::{AppDictionary, Backend, CommandResult, DictionaryWord, SiteDictionary};
use crate::ui::widgets::app_icon::AppIcons;
use crate::ui::widgets::word_list::{
    GroupIcon, GroupUndo, Notice, UNDO_DURATION, UndoBanner, UndoFocus, WordGroup, WordListEditor,
    WordListEvent,
};

use super::ops::{
    self, DeleteTicket, QueueScope, RestoreEvent, RestoreOutcome, RestoreState, RestoreStore,
    RestoreTarget,
};

/// Whose dictionaries these are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Apps,
    Sites,
}

/// The dictionary of one app or one site.
#[derive(Debug, Clone, PartialEq)]
struct Group {
    /// The app's bundle id or the site's domain.
    key: String,
    /// An app's name. Sites go by their domain.
    name: Option<String>,
    words: Vec<DictionaryWord>,
}

impl From<AppDictionary> for Group {
    fn from(dictionary: AppDictionary) -> Self {
        Self {
            key: dictionary.bundle_id,
            name: Some(dictionary.app_name),
            words: dictionary.words,
        }
    }
}

impl From<SiteDictionary> for Group {
    fn from(dictionary: SiteDictionary) -> Self {
        Self {
            key: dictionary.domain,
            name: None,
            words: dictionary.words,
        }
    }
}

impl Scope {
    fn queue_scope(self) -> QueueScope {
        match self {
            Scope::Apps => QueueScope::Apps,
            Scope::Sites => QueueScope::Sites,
        }
    }

    fn restore_target(self, group: &Group) -> RestoreTarget {
        match self {
            Scope::Apps => RestoreTarget::App {
                bundle_id: group.key.clone(),
                app_name: group.name.clone().unwrap_or_default(),
            },
            Scope::Sites => RestoreTarget::Site {
                domain: group.key.clone(),
            },
        }
    }

    fn list(self, backend: &dyn Backend) -> CommandResult<Vec<Group>> {
        fn groups<T: Into<Group>>(dictionaries: Vec<T>) -> Vec<Group> {
            dictionaries.into_iter().map(Into::into).collect()
        }

        match self {
            Scope::Apps => backend.list_app_dictionaries().map(groups),
            Scope::Sites => backend.list_site_dictionaries().map(groups),
        }
    }

    fn add_word(
        self,
        backend: &dyn Backend,
        group: &Group,
        word: &str,
    ) -> CommandResult<DictionaryWord> {
        match self {
            Scope::Apps => backend.add_app_dictionary_word(
                &group.key,
                group.name.as_deref().unwrap_or_default(),
                word,
            ),
            Scope::Sites => backend.add_site_dictionary_word(&group.key, word),
        }
    }

    fn update_word(
        self,
        backend: &dyn Backend,
        id: &str,
        word: &str,
    ) -> CommandResult<DictionaryWord> {
        match self {
            Scope::Apps => backend.update_app_dictionary_word(id, word),
            Scope::Sites => backend.update_site_dictionary_word(id, word),
        }
    }

    fn delete_word(self, backend: &dyn Backend, id: &str) -> CommandResult<Option<DictionaryWord>> {
        match self {
            Scope::Apps => backend.delete_app_dictionary_word(id),
            Scope::Sites => backend.delete_site_dictionary_word(id),
        }
    }

    fn delete_group(self, backend: &dyn Backend, key: &str) -> CommandResult<()> {
        match self {
            Scope::Apps => backend.delete_app_dictionary(key),
            Scope::Sites => backend.delete_site_dictionary(key),
        }
    }

    fn group_added_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_dictionary_app_added",
            Scope::Sites => "site_dictionary_site_added",
        }
    }

    fn group_removed_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_dictionary_app_removed",
            Scope::Sites => "site_dictionary_site_removed",
        }
    }

    fn word_added_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_dictionary_word_added",
            Scope::Sites => "site_dictionary_word_added",
        }
    }

    fn word_edited_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_dictionary_word_edited",
            Scope::Sites => "site_dictionary_word_edited",
        }
    }

    fn word_deleted_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_dictionary_word_deleted",
            Scope::Sites => "site_dictionary_word_deleted",
        }
    }
}

/// What a change was made to, to say there why it was refused.
enum Changed {
    Word { id: String, mutation: u64 },
    Group(SharedString),
}

/// An unacted deletion waiting for Undo, Dismiss, or [`UNDO_DURATION`].
/// Once the user clicks Undo, ownership moves into [`RestoreStore`] so the
/// restore and any retryable error survive tab switches.
struct LocalGroupUndo {
    token: u64,
    group: Group,
    word: DictionaryWord,
    index: usize,
    ticket: DeleteTicket,
    restore_requested: Rc<Cell<bool>>,
    ready: bool,
    _timer: Option<Task<()>>,
}

pub struct DictionaryGroups {
    scope: Scope,
    stored: Vec<Group>,
    /// Why the stored dictionaries could not be listed, the last time.
    load_error: Option<SharedString>,
    /// Apps or sites added here that have no words yet. A dictionary is
    /// stored from its first word on.
    pending: Vec<Group>,
    /// Word ids whose delete request is still in flight, mapped to their Undo
    /// token, so a concurrent fetch does not put them back prematurely.
    deleting: HashMap<String, u64>,
    /// Per-word mutation generation so a failed edit whose word was already
    /// deleted, re-edited, or restored does not attach a stale error notice.
    word_mutations: HashMap<String, u64>,
    mutation_seq: u64,
    /// Counts removals of each group key so completion callbacks from an
    /// earlier word delete or restore do not re-expand a removed group.
    group_removals: HashMap<String, u64>,
    revision: u64,
    fetch_seq: u64,
    local_undo: Option<LocalGroupUndo>,
    restore_store: Entity<RestoreStore>,
    icons: Rc<AppIcons>,
    editor: Entity<WordListEditor>,
    _subscriptions: [Subscription; 3],
}

impl DictionaryGroups {
    pub fn new(scope: Scope, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(WordListEditor::new);
        let restore_store = ops::store(cx);
        let subscriptions = [
            cx.subscribe_in(&editor, window, Self::handle_editor_event),
            cx.observe_in(&restore_store, window, |this, _, window, cx| {
                this.ensure_restore_groups(cx);
                this.sync_undos(UndoFocus::Keep, window, cx);
                this.changed(cx);
            }),
            cx.subscribe_in(&restore_store, window, |this, _, event, window, cx| {
                this.handle_restore_event(event, window, cx);
            }),
        ];

        let mut groups = Self {
            scope,
            stored: Vec::new(),
            load_error: None,
            pending: Vec::new(),
            deleting: HashMap::new(),
            word_mutations: HashMap::new(),
            mutation_seq: 0,
            group_removals: HashMap::new(),
            revision: 0,
            fetch_seq: 0,
            local_undo: None,
            restore_store,
            icons: Rc::default(),
            editor,
            _subscriptions: subscriptions,
        };
        groups.ensure_restore_groups(cx);
        groups.sync_undos(UndoFocus::Keep, window, cx);
        groups.changed(cx);
        groups
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

    /// Keeps a pending placeholder group on screen for any surviving in-flight
    /// or failed restore whose group currently has no stored words.
    fn ensure_restore_groups(&mut self, cx: &mut Context<Self>) {
        let entries = self
            .restore_store
            .read(cx)
            .entries_for(self.scope.queue_scope());
        for entry in entries {
            let Some(key) = entry.target.group_key() else {
                continue;
            };
            if !self.keys().any(|existing| existing == key) {
                self.pending.push(Group {
                    key: key.to_string(),
                    name: entry.target.app_name().map(str::to_string),
                    words: Vec::new(),
                });
                let open = SharedString::from(key.to_string());
                self.editor.update(cx, |editor, cx| editor.expand(open, cx));
            }
        }
    }

    fn handle_restore_event(
        &mut self,
        event: &RestoreEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let qscope = self.scope.queue_scope();
        match event {
            RestoreEvent::Settled {
                scope,
                token,
                word_id,
                group_key,
                outcome,
            } if *scope == qscope => {
                // Clear the in-flight delete marker before fetching so a failed
                // delete followed by Undo never filters out the saved word, and
                // report the delete failure if the preceding delete failed.
                self.clear_deleting(word_id, *token);
                self.bump_word_mutation(word_id);
                match outcome {
                    RestoreOutcome::DeleteFailed(error) => {
                        let group_present = group_key
                            .as_deref()
                            .is_some_and(|key| self.keys().any(|existing| existing == key));
                        if group_present {
                            let notice = Notice::new("dictionary.deleteFailed", error.clone(), cx);
                            self.editor.update(cx, |editor, cx| {
                                editor.fail_word(word_id.clone(), notice, cx);
                            });
                        }
                    }
                    RestoreOutcome::Restored
                    | RestoreOutcome::NotFound
                    | RestoreOutcome::Cancelled => {
                        self.editor
                            .update(cx, |editor, cx| editor.clear_word_error(word_id, cx));
                    }
                }
                self.sync_undos(UndoFocus::Keep, window, cx);
                if matches!(
                    outcome,
                    RestoreOutcome::Restored
                        | RestoreOutcome::NotFound
                        | RestoreOutcome::DeleteFailed(_)
                ) {
                    if let Some(key) = group_key
                        && self.keys().any(|existing| existing == key.as_str())
                    {
                        self.editor
                            .update(cx, |editor, cx| editor.expand(key.clone().into(), cx));
                    }
                    self.fetch(cx).detach();
                }
            }
            RestoreEvent::Failed {
                scope,
                token,
                word_id,
            } if *scope == qscope => {
                self.clear_deleting(word_id, *token);
                self.ensure_restore_groups(cx);
                self.sync_undos(UndoFocus::Keep, window, cx);
                self.changed(cx);
            }
            _ => {}
        }
    }

    /// The stored dictionaries, then the pending ones.
    fn groups(&self) -> impl Iterator<Item = &Group> {
        let pending = self
            .pending
            .iter()
            .filter(|pending| !self.stored.iter().any(|stored| stored.key == pending.key));

        self.stored.iter().chain(pending)
    }

    pub fn len(&self) -> usize {
        self.groups().count()
    }

    /// Whether there are any active timed, in-flight, or failed Undo rows for
    /// this scope.
    pub fn has_undos(&self, cx: &App) -> bool {
        self.local_undo.is_some()
            || !self
                .restore_store
                .read(cx)
                .entries_for(self.scope.queue_scope())
                .is_empty()
    }

    /// Why the stored dictionaries could not be listed. The page shows that
    /// instead of the dictionaries, which would look empty or out of date.
    pub fn load_error(&self) -> Option<SharedString> {
        self.load_error.clone()
    }

    /// The bundle ids or domains that have a dictionary.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.groups().map(|group| group.key.as_str())
    }

    /// The icons the apps' groups show.
    pub fn set_icons(&mut self, icons: Rc<AppIcons>, cx: &mut Context<Self>) {
        self.icons = icons;
        self.changed(cx);
    }

    /// Opens the dictionary of an app or a site, starting an empty one if it
    /// has none.
    pub fn add(&mut self, key: String, name: Option<String>, cx: &mut Context<Self>) {
        let open = SharedString::from(key.clone());
        self.editor.update(cx, |editor, cx| editor.expand(open, cx));

        if self.keys().any(|existing| existing == key) {
            return;
        }

        self.pending.push(Group {
            key,
            name,
            words: Vec::new(),
        });
        analytics::capture(cx, self.scope.group_added_event(), &[]);
        self.changed(cx);
    }

    /// Fetches the stored dictionaries.
    pub fn fetch(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.fetch_seq += 1;
        let seq = self.fetch_seq;
        let rev = self.revision;
        let scope = self.scope;
        let request = ops::run(scope.queue_scope(), cx, move |backend| scope.list(backend));

        cx.spawn(async move |this, cx| {
            let result = request.await;

            let refetch = this.update(cx, |this, cx| {
                if seq != this.fetch_seq {
                    return None;
                }
                if rev != this.revision {
                    return Some(this.fetch(cx));
                }

                match result {
                    Ok(mut stored) => {
                        for group in &mut stored {
                            group
                                .words
                                .retain(|word| !this.deleting.contains_key(&word.id));
                        }
                        this.pending
                            .retain(|pending| !stored.iter().any(|group| group.key == pending.key));
                        this.stored = stored;
                        this.load_error = None;
                        this.ensure_restore_groups(cx);
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "list_failed scope={scope:?} error={error}");
                        this.load_error = Some(error.into());
                    }
                }
                this.changed(cx);
                None
            });
            if let Ok(Some(task)) = refetch {
                task.await;
            }
        })
    }

    /// Stores a change the list already shows. If it is refused, the stored
    /// dictionaries are fetched again, and what was `changed` says why with
    /// the notice titled by `failure`.
    fn persist<T: Send + 'static>(
        &self,
        changed: Changed,
        failure: &'static str,
        change: impl FnOnce(&dyn Backend) -> CommandResult<T> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let scope = self.scope;
        let request = ops::run(scope.queue_scope(), cx, change);

        cx.spawn(async move |this, cx| {
            let Err(error) = request.await else {
                return;
            };
            log::error!(target: "dictionary", "change_failed scope={scope:?} error={error}");

            let fetch = this.update(cx, |this, cx| {
                let notice = Notice::new(failure, error, cx);
                match changed {
                    Changed::Word { id, mutation } => {
                        if this.word_mutations.get(&id) == Some(&mutation) {
                            this.editor
                                .update(cx, |editor, cx| editor.fail_word(id, notice, cx));
                        }
                    }
                    Changed::Group(key) => {
                        this.editor
                            .update(cx, |editor, cx| editor.fail_group(key, notice, cx));
                    }
                }
                this.fetch(cx)
            });
            if let Ok(fetch) = fetch {
                fetch.await;
            }
        })
        .detach();
    }

    /// Shows the groups as they are now.
    fn changed(&mut self, cx: &mut Context<Self>) {
        let list_hidden = self.load_error.is_some();
        let groups = self
            .groups()
            .map(|group| {
                let key = SharedString::from(group.key.clone());

                match &group.name {
                    Some(name) => WordGroup {
                        icon: GroupIcon::App(self.icons.get(&group.key)),
                        title: name.clone().into(),
                        subtitle: Some(key.clone()),
                        key,
                        words: group.words.clone(),
                    },
                    None => WordGroup {
                        icon: GroupIcon::Site,
                        title: key.clone(),
                        subtitle: None,
                        key,
                        words: group.words.clone(),
                    },
                }
            })
            .collect();

        self.editor.update(cx, |editor, cx| {
            editor.set_list_hidden(list_hidden, cx);
            editor.set_groups(groups, cx);
        });
        cx.notify();
    }

    fn sync_undos(&self, focus: UndoFocus, window: &mut Window, cx: &mut Context<Self>) {
        let mut group_undos: Vec<GroupUndo> = self
            .restore_store
            .read(cx)
            .entries_for(self.scope.queue_scope())
            .into_iter()
            .filter_map(|entry| {
                let key = SharedString::from(entry.target.group_key()?.to_string());
                let (restoring, error) = match entry.state {
                    RestoreState::Restoring => (true, None),
                    RestoreState::Failed(error) => {
                        (false, Some(Notice::new("dictionary.undoFailed", error, cx)))
                    }
                };
                Some(GroupUndo {
                    key,
                    token: entry.token,
                    index: entry.index,
                    banner: UndoBanner {
                        word: entry.word.word.into(),
                        restoring,
                        error,
                    },
                })
            })
            .collect();

        if let Some(local) = &self.local_undo {
            group_undos.push(GroupUndo {
                key: SharedString::from(local.group.key.clone()),
                token: local.token,
                index: local.index,
                banner: UndoBanner {
                    word: local.word.word.clone().into(),
                    restoring: false,
                    error: None,
                },
            });
        }

        self.editor.update(cx, |editor, cx| {
            editor.set_undos(group_undos, focus, window, cx);
        });
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
                    this.sync_undos(UndoFocus::Keep, window, cx);
                    cx.notify();
                }
            })
            .ok();
        })
    }

    fn handle_editor_event(
        &mut self,
        _: &Entity<WordListEditor>,
        event: &WordListEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            WordListEvent::AddWord { key, word } => self.add_word(key, word, window, cx),
            WordListEvent::EditWord { word_id, word } => self.edit_word(word_id, word, cx),
            WordListEvent::DeleteWord {
                key,
                word_id,
                from_keyboard,
            } => self.delete_word(key, word_id, from_keyboard, window, cx),
            WordListEvent::UndoDelete(token) => self.undo_delete(token, window, cx),
            WordListEvent::DismissUndo(token) => self.dismiss_undo(token, window, cx),
            WordListEvent::RemoveGroup(key) => self.remove_group(&key, window, cx),
        }
    }

    fn add_word(
        &mut self,
        key: SharedString,
        word: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = self.scope;
        let Some(group) = self
            .groups()
            .find(|group| group.key == key.as_ref())
            .cloned()
        else {
            self.editor
                .update(cx, |editor, cx| editor.finish_adding(&key, None, cx));
            return;
        };
        let request = ops::run(scope.queue_scope(), cx, move |backend| {
            scope.add_word(backend, &group, &word)
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = request.await;

            if result.is_ok() {
                let fetch = this.update_in(cx, |this, window, cx| {
                    this.editor
                        .update(cx, |editor, cx| editor.clear_new_word(&key, window, cx));
                    analytics::capture(cx, scope.word_added_event(), &[]);

                    if let Some(local) = &mut this.local_undo
                        && local.group.key == key.as_ref()
                    {
                        local.index += 1;
                        this.sync_undos(UndoFocus::Keep, window, cx);
                    }
                    ops::shift_indices_on_add(scope.queue_scope(), Some(key.as_ref()), cx);

                    // With its first word the dictionary is a stored one. It
                    // stays on screen until the stored ones have been fetched.
                    this.pending.retain(|pending| pending.key != key.as_ref());
                    this.fetch(cx)
                });
                if let Ok(fetch) = fetch {
                    fetch.await;
                }
            }

            this.update(cx, |this, cx| {
                // What was typed stays in the field, to be corrected or sent
                // again.
                let error = result.err().map(|error| {
                    log::error!(target: "dictionary", "add_failed scope={scope:?} error={error}");
                    Notice::new("dictionary.addFailed", error, cx)
                });
                this.editor
                    .update(cx, |editor, cx| editor.finish_adding(&key, error, cx));
            })
            .ok();
        })
        .detach();
    }

    fn edit_word(&mut self, word_id: String, word: String, cx: &mut Context<Self>) {
        let scope = self.scope;
        analytics::capture(cx, scope.word_edited_event(), &[]);
        self.revision += 1;
        let mutation = self.bump_word_mutation(&word_id);

        let stored_words = self.stored.iter_mut().flat_map(|group| &mut group.words);
        for stored in stored_words.filter(|stored| stored.id == word_id) {
            stored.word = word.clone();
        }
        self.changed(cx);

        self.persist(
            Changed::Word {
                id: word_id.clone(),
                mutation,
            },
            "dictionary.saveFailed",
            move |backend| scope.update_word(backend, &word_id, &word),
            cx,
        );
    }

    fn remove_word_locally(
        &mut self,
        key: &str,
        word_id: &str,
    ) -> Option<(Group, DictionaryWord, usize)> {
        let group_index = self
            .stored
            .iter()
            .position(|group| group.key == key && group.words.iter().any(|word| word.id == word_id))
            .or_else(|| {
                self.stored
                    .iter()
                    .position(|group| group.words.iter().any(|word| word.id == word_id))
            })?;
        let group = &mut self.stored[group_index];

        let index = group.words.iter().position(|word| word.id == word_id)?;
        let deleted = group.words.remove(index);
        let group_meta = Group {
            key: group.key.clone(),
            name: group.name.clone(),
            words: Vec::new(),
        };
        if group.words.is_empty()
            && !self
                .pending
                .iter()
                .any(|pending| pending.key == group_meta.key)
        {
            self.pending.push(group_meta.clone());
        }

        Some((group_meta, deleted, index))
    }

    fn delete_word(
        &mut self,
        key: SharedString,
        word_id: String,
        from_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((group_meta, deleted, index)) = self.remove_word_locally(&key, &word_id) else {
            return;
        };

        let scope = self.scope;
        ops::shift_indices_on_delete(scope.queue_scope(), Some(&group_meta.key), index, cx);

        analytics::capture(cx, scope.word_deleted_event(), &[]);
        self.bump_word_mutation(&deleted.id);
        self.revision += 1;
        let token = ops::next_token(cx);
        let ticket = DeleteTicket::new();
        let restore_requested = Rc::new(Cell::new(false));
        let removal_count = self
            .group_removals
            .get(&group_meta.key)
            .copied()
            .unwrap_or(0);
        self.deleting.insert(deleted.id.clone(), token);

        self.local_undo = Some(LocalGroupUndo {
            token,
            group: group_meta.clone(),
            word: deleted.clone(),
            index,
            ticket: ticket.clone(),
            restore_requested: restore_requested.clone(),
            ready: false,
            _timer: None,
        });
        self.changed(cx);
        self.sync_undos(
            if from_keyboard {
                UndoFocus::Button(token)
            } else {
                UndoFocus::Container
            },
            window,
            cx,
        );

        let request = ops::run(scope.queue_scope(), cx, {
            let word_id = word_id.clone();
            let ticket = ticket.clone();
            move |backend| {
                let result = scope.delete_word(backend, &word_id);
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
                        this.sync_undos(UndoFocus::Keep, window, cx);
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
                        this.sync_undos(UndoFocus::Keep, window, cx);
                        cx.notify();
                    }
                    (!requested).then(|| this.fetch(cx))
                });
                if let Ok(Some(fetch)) = fetch {
                    fetch.await;
                }
            }
            Err(error) => {
                log::error!(target: "dictionary", "change_failed scope={scope:?} error={error}");
                let requested = restore_requested.get();
                let fetch = this.update_in(cx, |this, window, cx| {
                    this.clear_deleting(&word_id, token);
                    let group_removed = this
                        .group_removals
                        .get(&group_meta.key)
                        .copied()
                        .unwrap_or(0)
                        != removal_count;
                    if !requested {
                        if this
                            .local_undo
                            .as_ref()
                            .is_some_and(|local| local.token == token)
                        {
                            this.local_undo = None;
                            this.sync_undos(UndoFocus::Keep, window, cx);
                            cx.notify();
                        }
                        if !group_removed {
                            let notice = Notice::new("dictionary.deleteFailed", error, cx);
                            this.editor
                                .update(cx, |editor, cx| editor.fail_word(word_id, notice, cx));
                        }
                        Some(this.fetch(cx))
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
            let target = self.scope.restore_target(&local.group);
            ops::start_restore(
                local.token,
                self.scope.queue_scope(),
                target,
                local.word,
                local.index,
                local.ticket,
                cx,
            );
            self.sync_undos(UndoFocus::Keep, window, cx);
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
            self.sync_undos(UndoFocus::Keep, window, cx);
            cx.notify();
            return;
        }

        ops::dismiss_restore(token, cx);
    }

    fn remove_group(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.scope;
        analytics::capture(cx, scope.group_removed_event(), &[]);
        *self.group_removals.entry(key.to_string()).or_default() += 1;
        self.revision += 1;

        if self
            .local_undo
            .as_ref()
            .is_some_and(|local| local.group.key == key)
        {
            self.local_undo = None;
        }
        ops::cancel_group_restores(scope.queue_scope(), key, cx);

        let removed_ids: Vec<String> = self
            .stored
            .iter()
            .filter(|group| group.key == key)
            .flat_map(|group| group.words.iter().map(|word| word.id.clone()))
            .collect();
        for word_id in removed_ids {
            self.bump_word_mutation(&word_id);
        }

        self.pending.retain(|group| group.key != key);
        self.stored.retain(|group| group.key != key);
        self.sync_undos(UndoFocus::Keep, window, cx);
        self.changed(cx);

        // Always enqueue the group delete on the FIFO worker even if the group
        // currently looks empty locally: an earlier word restore or add may
        // have just written a row before the list refreshed.
        let key = key.to_string();
        self.persist(
            Changed::Group(key.clone().into()),
            "dictionary.removeFailed",
            move |backend| scope.delete_group(backend, &key),
            cx,
        );
    }
}

impl Render for DictionaryGroups {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}
