//! The dictionaries of the apps or of the sites: the stored ones, the ones
//! just added that have no words yet, and the editor that shows them all.

use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, Render, SharedString, Subscription, Task, Window,
};
use std::collections::HashSet;
use std::rc::Rc;

use crate::analytics;
use crate::backend::{self, AppDictionary, Backend, CommandResult, DictionaryWord, SiteDictionary};
use crate::ui::widgets::app_icon::AppIcons;
use crate::ui::widgets::word_list::{GroupIcon, Notice, WordGroup, WordListEditor, WordListEvent};

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

    fn add_words(
        self,
        backend: &dyn Backend,
        group: &Group,
        words: &[String],
    ) -> CommandResult<Vec<DictionaryWord>> {
        match self {
            Scope::Apps => backend.add_app_dictionary_words(
                &group.key,
                group.name.as_deref().unwrap_or_default(),
                words,
            ),
            Scope::Sites => backend.add_site_dictionary_words(&group.key, words),
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

    fn delete_word(self, backend: &dyn Backend, id: &str) -> CommandResult<()> {
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

    fn words_bulk_added_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_dictionary_words_bulk_added",
            Scope::Sites => "site_dictionary_words_bulk_added",
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
    Word(String),
    Group(SharedString),
}

pub struct DictionaryGroups {
    scope: Scope,
    stored: Vec<Group>,
    /// Why the stored dictionaries could not be listed, the last time.
    load_error: Option<SharedString>,
    /// Apps or sites added here that have no words yet. A dictionary is
    /// stored from its first word on.
    pending: Vec<Group>,
    /// Keys with an in-flight word or bulk-word add; removal is refused while
    /// a write is running so a deleted pending group cannot reappear.
    adding_groups: HashSet<String>,
    /// Chains dictionary writes in user-action order so a bulk save cannot
    /// overtake an earlier delete or edit on another thread.
    write_tail: Option<async_channel::Receiver<()>>,
    /// Incremented on every local write so an earlier `fetch` snapshot cannot
    /// overwrite newer additions, edits, or deletions.
    fetch_generation: u64,
    /// Monotonic id of the latest `fetch` started, so an older slow refresh
    /// never overwrites a newer refresh.
    latest_fetch_id: u64,
    /// Number of backend writes still in flight (adds, bulk adds, edits,
    /// deletes, and group removals).
    pending_writes: usize,
    /// Number of optimistic edits, deletions, or removals awaiting backend
    /// confirmation.
    pending_optimistic_writes: usize,
    /// Whether a failed optimistic write is waiting for a refresh to restore
    /// confirmed backend state before bulk submit can proceed.
    reconciling_after_failure: bool,
    /// Whether a fetch should run once `pending_writes` reaches zero.
    refetch_after_writes: bool,
    icons: Rc<AppIcons>,
    editor: Entity<WordListEditor>,
    _subscription: Subscription,
}

impl DictionaryGroups {
    pub fn new(scope: Scope, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(WordListEditor::new);
        let subscription = cx.subscribe_in(&editor, window, Self::handle_editor_event);

        Self {
            scope,
            stored: Vec::new(),
            load_error: None,
            pending: Vec::new(),
            adding_groups: HashSet::new(),
            write_tail: None,
            fetch_generation: 0,
            latest_fetch_id: 0,
            pending_writes: 0,
            pending_optimistic_writes: 0,
            reconciling_after_failure: false,
            refetch_after_writes: false,
            icons: Rc::default(),
            editor,
            _subscription: subscription,
        }
    }

    fn has_unconfirmed_changes(&self) -> bool {
        self.pending_optimistic_writes > 0 || self.reconciling_after_failure
    }

    fn sync_bulk_submit_blocked(&self, cx: &mut Context<Self>) {
        let blocked = self.has_unconfirmed_changes();
        self.editor
            .update(cx, |editor, cx| editor.set_bulk_submit_blocked(blocked, cx));
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

    /// Merges newly persisted words into `self.stored` immediately so a
    /// pending group is recognized as stored even if a full refresh waits
    /// behind another in-flight write.
    fn record_stored_words(&mut self, group: &Group, added: Vec<DictionaryWord>) {
        if added.is_empty() {
            return;
        }
        self.pending.retain(|pending| pending.key != group.key);
        if let Some(existing) = self
            .stored
            .iter_mut()
            .find(|stored| stored.key == group.key)
        {
            existing.words.splice(0..0, added);
        } else {
            self.stored.push(Group {
                key: group.key.clone(),
                name: group.name.clone(),
                words: added,
            });
            self.stored
                .sort_by_key(|entry| entry.name.as_deref().unwrap_or(&entry.key).to_lowercase());
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
        let scope = self.scope;
        self.latest_fetch_id += 1;
        let fetch_id = self.latest_fetch_id;
        let generation = self.fetch_generation;
        let request = backend::call(cx, move |backend| scope.list(backend));

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
                        this.fetch(cx).detach();
                    }
                    return;
                }

                match result {
                    Ok(stored) => {
                        // Any pending group that now exists in storage moves
                        // out of `pending` atomically when `stored` updates.
                        this.pending
                            .retain(|pending| !stored.iter().any(|s| s.key == pending.key));
                        this.stored = stored;
                        this.load_error = None;
                        this.reconciling_after_failure = false;
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "list_failed scope={scope:?} error={error}");
                        this.load_error = Some(error.into());
                    }
                }
                this.changed(cx);
                this.sync_bulk_submit_blocked(cx);
            })
            .ok();
        })
    }

    /// Stores a change the list already shows. If it is refused, the stored
    /// dictionaries are fetched again, and what was `changed` says why with
    /// the notice titled by `failure`.
    fn persist<T: Send + 'static>(
        &mut self,
        changed: Changed,
        failure: &'static str,
        change: impl FnOnce(&dyn Backend) -> CommandResult<T> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let scope = self.scope;
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
                            Some(this.fetch(cx))
                        } else {
                            None
                        }
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "change_failed scope={scope:?} error={error}");
                        this.reconciling_after_failure = true;
                        let notice = Notice::new(failure, error, cx);
                        this.editor.update(cx, |editor, cx| match changed {
                            Changed::Word(word_id) => editor.fail_word(word_id, notice, cx),
                            Changed::Group(key) => editor.fail_group(key, notice, cx),
                        });
                        if this.pending_writes == 0 {
                            this.refetch_after_writes = false;
                            Some(this.fetch(cx))
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

    /// Shows the groups as they are now.
    fn changed(&mut self, cx: &mut Context<Self>) {
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

        self.editor
            .update(cx, |editor, cx| editor.set_groups(groups, cx));
        cx.notify();
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
            WordListEvent::BulkAddWords { key, words } => self.add_words(key, words, window, cx),
            WordListEvent::EditWord { word_id, word } => self.edit_word(word_id, word, cx),
            WordListEvent::DeleteWord { word_id } => self.delete_word(word_id, cx),
            WordListEvent::RemoveGroup(key) => self.remove_group(&key, cx),
        }
    }

    fn add_words(
        &mut self,
        key: SharedString,
        words: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = self.scope;
        let Some(group) = self
            .groups()
            .find(|group| group.key == key.as_ref())
            .cloned()
        else {
            self.editor.update(cx, |editor, cx| {
                editor.finish_bulk_adding(&key, None, window, cx)
            });
            return;
        };
        self.adding_groups.insert(key.to_string());
        self.fetch_generation += 1;
        self.pending_writes += 1;
        let (prev, done_tx) = self.enqueue_write();
        let saved_group = group.clone();

        cx.spawn_in(window, async move |this, cx| {
            if let Some(prev) = prev {
                let _ = prev.recv().await;
            }
            let request = cx.update(|_, cx| {
                backend::call(cx, move |backend| scope.add_words(backend, &group, &words))
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
                this.adding_groups.remove(key.as_ref());

                match result {
                    Ok(added) => {
                        let has_added = !added.is_empty();
                        if has_added {
                            analytics::capture(
                                cx,
                                scope.words_bulk_added_event(),
                                &[("count", (added.len() as i64).into())],
                            );
                            this.record_stored_words(&saved_group, added);
                        }
                        this.editor.update(cx, |editor, cx| {
                            editor.finish_bulk_adding(&key, None, window, cx)
                        });
                        if has_added {
                            this.changed(cx);
                        }
                        if this.pending_writes == 0 {
                            this.refetch_after_writes = false;
                            Some(this.fetch(cx))
                        } else {
                            this.refetch_after_writes = true;
                            None
                        }
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "bulk_add_failed scope={scope:?} error={error}");
                        let notice = Notice::new("dictionary.bulkAddFailed", error, cx);
                        this.editor.update(cx, |editor, cx| {
                            editor.finish_bulk_adding(&key, Some(notice), window, cx)
                        });
                        if this.pending_writes == 0
                            && std::mem::take(&mut this.refetch_after_writes)
                        {
                            Some(this.fetch(cx))
                        } else {
                            None
                        }
                    }
                }
            });
            drop(done_tx);

            if let Ok(Some(fetch)) = fetch {
                fetch.await;
            }
        })
        .detach();
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
        self.adding_groups.insert(key.to_string());
        self.fetch_generation += 1;
        self.pending_writes += 1;
        let (prev, done_tx) = self.enqueue_write();
        let saved_group = group.clone();

        cx.spawn_in(window, async move |this, cx| {
            if let Some(prev) = prev {
                let _ = prev.recv().await;
            }
            let request = cx.update(|_, cx| {
                backend::call(cx, move |backend| scope.add_word(backend, &group, &word))
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
                this.adding_groups.remove(key.as_ref());

                match result {
                    Ok(added) => {
                        this.editor
                            .update(cx, |editor, cx| editor.clear_new_word(&key, window, cx));
                        analytics::capture(cx, scope.word_added_event(), &[]);
                        this.record_stored_words(&saved_group, vec![added]);
                        this.changed(cx);
                        this.editor
                            .update(cx, |editor, cx| editor.finish_adding(&key, None, cx));
                        if this.pending_writes == 0 {
                            this.refetch_after_writes = false;
                            Some(this.fetch(cx))
                        } else {
                            this.refetch_after_writes = true;
                            None
                        }
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "add_failed scope={scope:?} error={error}");
                        let notice = Notice::new("dictionary.addFailed", error, cx);
                        this.editor
                            .update(cx, |editor, cx| editor.finish_adding(&key, Some(notice), cx));
                        if this.pending_writes == 0
                            && std::mem::take(&mut this.refetch_after_writes)
                        {
                            Some(this.fetch(cx))
                        } else {
                            None
                        }
                    }
                }
            });
            drop(done_tx);

            if let Ok(Some(fetch)) = fetch {
                fetch.await;
            }
        })
        .detach();
    }

    fn edit_word(&mut self, word_id: String, word: String, cx: &mut Context<Self>) {
        let scope = self.scope;
        analytics::capture(cx, scope.word_edited_event(), &[]);

        let stored_words = self.stored.iter_mut().flat_map(|group| &mut group.words);
        for stored in stored_words.filter(|stored| stored.id == word_id) {
            stored.word = word.clone();
        }
        self.changed(cx);

        self.persist(
            Changed::Word(word_id.clone()),
            "dictionary.saveFailed",
            move |backend| scope.update_word(backend, &word_id, &word),
            cx,
        );
    }

    fn delete_word(&mut self, word_id: String, cx: &mut Context<Self>) {
        let scope = self.scope;
        analytics::capture(cx, scope.word_deleted_event(), &[]);

        for group in &mut self.stored {
            group.words.retain(|stored| stored.id != word_id);
        }
        self.changed(cx);

        self.persist(
            Changed::Word(word_id.clone()),
            "dictionary.deleteFailed",
            move |backend| scope.delete_word(backend, &word_id),
            cx,
        );
    }

    fn remove_group(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.adding_groups.contains(key) {
            return;
        }
        let scope = self.scope;
        analytics::capture(cx, scope.group_removed_event(), &[]);

        // A dictionary without words is not stored.
        let stored = self
            .groups()
            .any(|group| group.key == key && !group.words.is_empty());

        self.pending.retain(|group| group.key != key);
        self.stored.retain(|group| group.key != key);
        self.changed(cx);

        if stored {
            let key = key.to_string();
            self.persist(
                Changed::Group(key.clone().into()),
                "dictionary.removeFailed",
                move |backend| scope.delete_group(backend, &key),
                cx,
            );
        } else {
            self.fetch_generation += 1;
        }
    }
}

impl Render for DictionaryGroups {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}
