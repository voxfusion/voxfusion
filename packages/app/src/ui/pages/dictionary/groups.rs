//! The dictionaries of the apps or of the sites: the stored ones, the ones
//! just added that have no words yet, and the editor that shows them all.

use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, Render, SharedString, Subscription, Task, Window,
};
use std::collections::HashMap;
use std::rc::Rc;

use crate::analytics;
use crate::backend::{
    self, AppDictionary, Backend, CommandResult, DUPLICATE_WORD_ERROR, DictionaryQueue,
    DictionaryWord, SiteDictionary,
};
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
    fn queue(self) -> DictionaryQueue {
        match self {
            Scope::Apps => DictionaryQueue::Apps,
            Scope::Sites => DictionaryQueue::Sites,
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
    icons: Rc<AppIcons>,
    editor: Entity<WordListEditor>,
    /// Monotonic counter bumped on every local change and every fetch start so
    /// a stale read snapshot never overwrites newer state.
    revision: u64,
    latest_fetch_rev: u64,
    /// Per-group generation bumped whenever a group is removed, so an earlier
    /// in-flight word add cannot put a deleted group back on screen.
    group_generations: HashMap<String, u64>,
    /// Number of in-flight word adds per group key, so deleting a newly added
    /// group still queues a backend delete behind its pending first word.
    adding_groups: HashMap<String, usize>,
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
            icons: Rc::default(),
            editor,
            revision: 0,
            latest_fetch_rev: 0,
            group_generations: HashMap::new(),
            adding_groups: HashMap::new(),
            _subscription: subscription,
        }
    }

    fn group_generation(&self, key: &str) -> u64 {
        self.group_generations.get(key).copied().unwrap_or(0)
    }

    fn finish_pending_add(&mut self, key: &str) {
        if let Some(count) = self.adding_groups.get_mut(key) {
            if *count <= 1 {
                self.adding_groups.remove(key);
            } else {
                *count -= 1;
            }
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
        self.revision += 1;
        let fetch_rev = self.revision;
        self.latest_fetch_rev = fetch_rev;
        let request =
            backend::call_dictionary(cx, scope.queue(), move |backend| scope.list(backend));

        cx.spawn(async move |this, cx| {
            let result = request.await;

            let retry = this.update(cx, |this, cx| {
                if this.revision != fetch_rev {
                    return (this.latest_fetch_rev == fetch_rev).then(|| this.fetch(cx));
                }

                match result {
                    Ok(stored) => {
                        this.stored = stored;
                        this.load_error = None;
                    }
                    Err(error) => {
                        log::error!(target: "dictionary", "list_failed scope={scope:?} error={error}");
                        this.load_error = Some(error.into());
                    }
                }
                this.changed(cx);
                None
            });
            if let Ok(Some(retry)) = retry {
                retry.await;
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
        let request = backend::call_dictionary(cx, scope.queue(), change);

        cx.spawn(async move |this, cx| {
            let Err(error) = request.await else {
                return;
            };
            log::error!(target: "dictionary", "change_failed scope={scope:?} error={error}");

            let fetch = this.update(cx, |this, cx| {
                let notice = Notice::new(failure, error, cx);
                this.editor.update(cx, |editor, cx| match changed {
                    Changed::Word(word_id) => editor.fail_word(word_id, notice, cx),
                    Changed::Group(key) => editor.fail_group(key, notice, cx),
                });
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
            WordListEvent::AddWord { key, word, add_id } => {
                self.add_word(key, word, add_id, window, cx)
            }
            WordListEvent::EditWord {
                word_id,
                word,
                save_id,
            } => self.edit_word(word_id, word, save_id, window, cx),
            WordListEvent::DeleteWord { word_id } => self.delete_word(word_id, cx),
            WordListEvent::RemoveGroup(key) => self.remove_group(&key, cx),
        }
    }

    fn add_word(
        &mut self,
        key: SharedString,
        word: String,
        add_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = self.scope;
        let submitted = word.clone();
        let Some(group) = self
            .groups()
            .find(|group| group.key == key.as_ref())
            .cloned()
        else {
            self.editor.update(cx, |editor, cx| {
                editor.finish_adding(&key, add_id, &submitted, None, cx)
            });
            return;
        };
        let group_key = group.key.clone();
        let group_name = group.name.clone();
        let group_gen = self.group_generation(key.as_ref());
        *self.adding_groups.entry(key.to_string()).or_default() += 1;
        let request = backend::call_dictionary(cx, scope.queue(), move |backend| {
            scope.add_word(backend, &group, &word)
        });

        cx.spawn_in(window, async move |this, cx| match request.await {
            Ok(added) => {
                let fetch = this.update_in(cx, |this, window, cx| {
                    this.finish_pending_add(key.as_ref());
                    if this.group_generation(key.as_ref()) != group_gen {
                        // The group was removed while this add was in flight;
                        // a queued backend delete will remove the word from
                        // storage, so do not put the group back on screen.
                        this.editor.update(cx, |editor, cx| {
                            editor.finish_adding(&key, add_id, &submitted, None, cx)
                        });
                        return None;
                    }

                    this.editor.update(cx, |editor, cx| {
                        editor.clear_new_word(&key, &submitted, window, cx)
                    });
                    analytics::capture(cx, scope.word_added_event(), &[]);

                    if let Some(existing) = this.stored.iter_mut().find(|g| g.key == key.as_ref()) {
                        if !existing.words.iter().any(|word| word.id == added.id) {
                            existing.words.insert(0, added);
                        }
                    } else {
                        this.stored.push(Group {
                            key: group_key,
                            name: group_name,
                            words: vec![added],
                        });
                    }
                    // With its first word the dictionary is a stored one.
                    this.pending.retain(|pending| pending.key != key.as_ref());
                    this.revision += 1;
                    this.changed(cx);
                    Some(this.fetch(cx))
                });

                if let Ok(Some(fetch)) = fetch {
                    fetch.await;

                    this.update(cx, |this, cx| {
                        this.editor.update(cx, |editor, cx| {
                            editor.finish_adding(&key, add_id, &submitted, None, cx)
                        });
                    })
                    .ok();
                }
            }
            Err(error) => {
                // What was typed stays in the field, to be corrected or
                // sent again.
                if error != DUPLICATE_WORD_ERROR {
                    log::error!(target: "dictionary", "add_failed scope={scope:?} error={error}");
                }
                this.update(cx, |this, cx| {
                    this.finish_pending_add(key.as_ref());
                    let notice = (this.group_generation(key.as_ref()) == group_gen)
                        .then(|| Notice::from_backend("dictionary.addFailed", error, cx));
                    this.editor.update(cx, |editor, cx| {
                        editor.finish_adding(&key, add_id, &submitted, notice, cx)
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    fn edit_word(
        &mut self,
        word_id: String,
        word: String,
        save_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = self.scope;
        let submitted = word.clone();
        let request = backend::call_dictionary(cx, scope.queue(), {
            let word_id = word_id.clone();
            move |backend| scope.update_word(backend, &word_id, &word)
        });

        cx.spawn_in(window, async move |this, cx| match request.await {
            Ok(updated) => {
                this.update_in(cx, |this, window, cx| {
                    analytics::capture(cx, scope.word_edited_event(), &[]);
                    let stored_words =
                        this.stored.iter_mut().flat_map(|group| &mut group.words);
                    for stored in stored_words.filter(|stored| stored.id == word_id) {
                        stored.word = updated.word.clone();
                    }
                    this.revision += 1;
                    this.changed(cx);
                    this.editor.update(cx, |editor, cx| {
                        editor.finish_editing(&word_id, save_id, &submitted, None, window, cx);
                    });
                })
                .ok();
            }
            Err(error) => {
                if error != DUPLICATE_WORD_ERROR {
                    log::error!(target: "dictionary", "change_failed scope={scope:?} error={error}");
                }

                let fetch = this.update_in(cx, |this, window, cx| {
                    let notice = Notice::from_backend("dictionary.saveFailed", error, cx);
                    this.editor.update(cx, |editor, cx| {
                        editor.finish_editing(
                            &word_id,
                            save_id,
                            &submitted,
                            Some(notice),
                            window,
                            cx,
                        );
                    });
                    this.fetch(cx)
                });
                if let Ok(fetch) = fetch {
                    fetch.await;
                }
            }
        })
        .detach();
    }

    fn delete_word(&mut self, word_id: String, cx: &mut Context<Self>) {
        let scope = self.scope;
        analytics::capture(cx, scope.word_deleted_event(), &[]);

        for group in &mut self.stored {
            group.words.retain(|stored| stored.id != word_id);
        }
        self.revision += 1;
        self.changed(cx);

        self.persist(
            Changed::Word(word_id.clone()),
            "dictionary.deleteFailed",
            move |backend| scope.delete_word(backend, &word_id),
            cx,
        );
    }

    fn remove_group(&mut self, key: &str, cx: &mut Context<Self>) {
        let scope = self.scope;
        analytics::capture(cx, scope.group_removed_event(), &[]);

        // A dictionary without stored words or an in-flight first word is not
        // stored. If a word add is in flight, queue the delete behind it.
        let stored = self
            .groups()
            .any(|group| group.key == key && !group.words.is_empty())
            || self.adding_groups.contains_key(key);

        *self.group_generations.entry(key.to_string()).or_default() += 1;
        self.pending.retain(|group| group.key != key);
        self.stored.retain(|group| group.key != key);
        self.revision += 1;
        self.changed(cx);

        if stored {
            let key = key.to_string();
            self.persist(
                Changed::Group(key.clone().into()),
                "dictionary.removeFailed",
                move |backend| scope.delete_group(backend, &key),
                cx,
            );
        }
    }
}

impl Render for DictionaryGroups {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}
