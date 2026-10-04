//! The dictionaries of the apps or of the sites: the stored ones, the ones
//! just added that have no words yet, and the editor that shows them all.

use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, Render, SharedString, Subscription, Task, Window,
};
use std::rc::Rc;

use crate::analytics;
use crate::backend::{self, AppDictionary, Backend, CommandResult, DictionaryWord, SiteDictionary};
use crate::ui::widgets::app_icon::AppIcons;
use crate::ui::widgets::word_list::{GroupIcon, WordGroup, WordListEditor, WordListEvent};

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

pub struct DictionaryGroups {
    scope: Scope,
    stored: Vec<Group>,
    /// Apps or sites added here that have no words yet. A dictionary is
    /// stored from its first word on.
    pending: Vec<Group>,
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
            pending: Vec::new(),
            icons: Rc::default(),
            editor,
            _subscription: subscription,
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
    pub fn fetch(&self, cx: &mut Context<Self>) -> Task<()> {
        let scope = self.scope;
        let request = backend::call(cx, move |backend| scope.list(backend));

        cx.spawn(async move |this, cx| {
            let result = request.await;

            this.update(cx, |this, cx| {
                match result {
                    Ok(stored) => this.stored = stored,
                    Err(error) => {
                        log::error!(target: "dictionary", "list_failed scope={scope:?} error={error}");
                    }
                }
                this.changed(cx);
            })
            .ok();
        })
    }

    /// Stores a change the list already shows. If it is refused, the stored
    /// dictionaries are fetched again.
    fn persist<T: Send + 'static>(
        &self,
        change: impl FnOnce(&dyn Backend) -> CommandResult<T> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let request = backend::call(cx, change);

        cx.spawn(async move |this, cx| {
            if request.await.is_err()
                && let Ok(fetch) = this.update(cx, |this, cx| this.fetch(cx))
            {
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
            WordListEvent::EditWord { word_id, word } => self.edit_word(word_id, word, cx),
            WordListEvent::DeleteWord { word_id } => self.delete_word(word_id, cx),
            WordListEvent::RemoveGroup(key) => self.remove_group(&key, cx),
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
                .update(cx, |editor, cx| editor.finish_adding(&key, cx));
            return;
        };
        let request = backend::call(cx, move |backend| scope.add_word(backend, &group, &word));

        cx.spawn_in(window, async move |this, cx| {
            if request.await.is_ok() {
                let fetch = this.update_in(cx, |this, window, cx| {
                    this.editor
                        .update(cx, |editor, cx| editor.clear_new_word(&key, window, cx));
                    analytics::capture(cx, scope.word_added_event(), &[]);

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
                this.editor
                    .update(cx, |editor, cx| editor.finish_adding(&key, cx));
            })
            .ok();
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

        self.persist(move |backend| scope.delete_word(backend, &word_id), cx);
    }

    fn remove_group(&mut self, key: &str, cx: &mut Context<Self>) {
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
            self.persist(move |backend| scope.delete_group(backend, &key), cx);
        }
    }
}

impl Render for DictionaryGroups {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}
