//! Editing dictionary words: the form that adds a word, a word's row with
//! its actions, and the editor for words grouped by app or by site.

use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable as _, Image, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
    prelude::*, px,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use crate::backend::{DictionaryWord, MAX_WORD_CHARS};
use crate::ui::motion::{Faded, Transitions as _};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::app_icon::{app_icon, site_icon};
use crate::ui::widgets::text_field::{TextField, TextFieldEvent, field_box};
use crate::ui::widgets::{icon, turning};
use crate::ui::{t, t_with, upper};

/// Where a form or a row is shown: on the page itself, or inside a group of
/// the grouped editor, where everything is a size smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Page,
    Nested,
}

/// What could not be done to a word or a dictionary, and the error the
/// services gave for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: SharedString,
    pub error: SharedString,
}

impl Notice {
    /// A notice titled by the interface string `key`.
    pub fn new(key: &str, error: impl Into<SharedString>, cx: &App) -> Self {
        Self {
            title: t(cx, key),
            error: error.into(),
        }
    }
}

/// The notice under a form or a word: an alert with what failed, and the
/// error under it.
fn notice(notice: &Notice, cx: &App) -> Div {
    let p = palette(cx);

    div()
        .flex()
        .flex_col()
        .gap_1()
        .type_xs()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(p.ac)
                .child(icon("alert-circle").size_4())
                .child(text(notice.title.clone())),
        )
        .when(!notice.error.is_empty(), |block| {
            // Under the title, past the icon and the gap.
            block.child(
                div()
                    .pl_6()
                    .text_color(p.txt_muted)
                    .child(text(notice.error.clone()).break_all()),
            )
        })
}

/// Whether `word` is longer than a dictionary takes.
pub fn too_long(word: &str) -> bool {
    word.trim().chars().count() > MAX_WORD_CHARS
}

/// The line under a field whose word is too long, saying by how much.
fn length_warning(word: &str, cx: &App) -> Option<Div> {
    let p = palette(cx);

    too_long(word).then(|| {
        let count = word.trim().chars().count().to_string();
        let message = t_with(
            cx,
            "dictionary.wordTooLong",
            &[("max", &MAX_WORD_CHARS.to_string()), ("count", &count)],
        );

        div().flex().justify_end().child(
            div()
                .type_xs()
                .text_right()
                .text_color(p.ac)
                .child(text(message)),
        )
    })
}

/// A field for a new word, with the placeholder the add forms share.
pub fn new_word_field<T: 'static>(window: &mut Window, cx: &mut Context<T>) -> Entity<TextField> {
    cx.new(|cx| TextField::new(window, cx).placeholder(|cx| t(cx, "dictionary.wordPlaceholder")))
}

/// The accent button of the add forms: a plus and `label`. The forms lie on
/// a surface-colored card.
pub fn add_button(
    variant: Variant,
    label: SharedString,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Faded {
    let p = palette(cx);

    let button = div()
        .id("add")
        .transition_colors()
        .flex()
        .items_center()
        .text_right()
        .map(|button| match variant {
            Variant::Page => button.gap_2().px_4().py_2().type_sm(),
            Variant::Nested => button.gap_1().px_3().py_1p5().type_xs(),
        });

    let button = if disabled {
        // `opacity-50` fades the button as a whole. Opacity here fades each
        // layer on its own, which would let the background show through the
        // label, so the label gets the color it ends up with over the card.
        button
            .bg(p.ac.opacity(0.5))
            .hover(|button| button.bg(p.ac_hover.opacity(0.5)))
            .text_color(p.surface.blend(p.ac_on.opacity(0.5)))
    } else {
        button
            .bg(p.ac)
            .hover(|button| button.bg(p.ac_hover))
            .text_color(p.ac_on)
            .on_click(on_click)
    };

    button
        .child(text("+").tracking_wider())
        .child(text(upper(&label)).tracking_wider())
}

/// The form that adds a word: the field and its button, then a warning
/// while the word is too long, or `error` once one could not be added.
pub fn add_word_form(
    variant: Variant,
    field: &Entity<TextField>,
    adding: bool,
    error: Option<&Notice>,
    on_submit: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> Div {
    let p = palette(cx);
    let focused = field.focus_handle(cx).is_focused(window);
    let value = field.read(cx).value(cx);
    let disabled = value.trim().is_empty() || too_long(&value) || adding;

    let row = div()
        .flex()
        .map(|row| match variant {
            Variant::Page => row.gap_3(),
            Variant::Nested => row.gap_2(),
        })
        .child(
            field_box(field, cx)
                .id("new-word")
                .transition_colors()
                .flex_1()
                .map(|input| match variant {
                    Variant::Page => input.px_4().py_2(),
                    Variant::Nested => input.px_3().py_1p5().type_sm(),
                })
                .bg(p.input)
                .border_1()
                .border_color(if focused { p.ac } else { p.border_strong })
                .text_color(p.txt_primary),
        )
        .child(add_button(
            variant,
            t(cx, "dictionary.addWord"),
            disabled,
            on_submit,
            cx,
        ));

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(row)
        .children(length_warning(&value, cx))
        .children(error.map(|error| notice(error, cx)))
}

/// What the buttons of a word's row ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordAction {
    StartEdit,
    SaveEdit,
    CancelEdit,
    Delete,
}

/// A word with its edit and delete buttons, or, while it is being edited,
/// `editing` with save and cancel and a warning while the word is too long.
/// `error` says why the last change to the word was refused.
pub fn word_row(
    variant: Variant,
    word: &DictionaryWord,
    editing: Option<&Entity<TextField>>,
    error: Option<&Notice>,
    on_action: impl Fn(&WordAction, &mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> Faded {
    let p = palette(cx);
    let on_action = Rc::new(on_action);

    let button = |label: &'static str, action: WordAction| {
        let on_action = on_action.clone();

        div()
            .id(label)
            .on_click(move |_, window, cx| on_action(&action, window, cx))
            .child(text(label).tracking_wider())
    };

    // Shown only while the pointer is over the row.
    let hover_button = |label: &'static str, action: WordAction| {
        button(label, action)
            .transition_all()
            .text_color(p.txt_muted)
            .opacity(0.)
            .group_hover("word", |button| button.opacity(1.))
            .hover(|button| button.text_color(p.ac))
    };

    let content = match editing {
        Some(field) => {
            let focused = field.focus_handle(cx).is_focused(window);

            field_box(field, cx)
                .id("edit-field")
                .transition_colors()
                .flex_1()
                .px_2()
                .py_1()
                .map(|input| match variant {
                    Variant::Page => input.mr_4().bg(p.input),
                    Variant::Nested => input.type_sm().bg(p.base),
                })
                .border_1()
                .border_color(if focused { p.ac } else { p.border_strong })
                .text_color(p.txt_primary)
                .into_any_element()
        }
        None => match variant {
            Variant::Page => div()
                .text_color(p.txt_primary)
                .child(text(word.word.clone())),
            Variant::Nested => div()
                .flex_1()
                .min_w_0()
                .type_sm()
                .text_color(p.txt_primary)
                .child(text(word.word.clone()).truncate()),
        }
        .into_any_element(),
    };

    let actions = div()
        .flex()
        .items_center()
        .gap_2()
        .type_xs()
        .text_right()
        .when(variant == Variant::Nested, |actions| {
            actions.flex_shrink_0()
        })
        .map(|actions| {
            if editing.is_some() {
                actions
                    .child(
                        button("[SAVE]", WordAction::SaveEdit)
                            .text_color(p.success)
                            .hover(|button| button.opacity(0.8)),
                    )
                    .child(
                        button("[CANCEL]", WordAction::CancelEdit)
                            .text_color(p.txt_muted)
                            .hover(|button| button.text_color(p.txt_secondary)),
                    )
            } else {
                actions
                    .child(hover_button("[EDIT]", WordAction::StartEdit))
                    .child(hover_button("[DEL]", WordAction::Delete))
            }
        });

    let line = div()
        .flex()
        .items_center()
        .justify_between()
        .when(variant == Variant::Nested, |line| line.gap_2())
        .child(content)
        .child(actions);
    let warning = editing.and_then(|field| length_warning(&field.read(cx).value(cx), cx));

    word_row_frame(variant, &word.id, cx)
        .child(line)
        .children(warning)
        .children(error.map(|error| notice(error, cx)))
}

fn word_row_frame(variant: Variant, word_id: &str, cx: &App) -> Faded {
    let p = palette(cx);

    let row = div()
        .id(SharedString::from(format!("word-{word_id}")))
        .group("word")
        .transition_colors()
        .flex()
        .flex_col()
        .gap_2()
        .border_1()
        .border_color(p.border);

    match variant {
        Variant::Page => row
            .px_4()
            .py_3()
            .bg(p.surface)
            .hover(|row| row.border_color(p.border_strong)),
        Variant::Nested => row.px_3().py_2().bg(p.input),
    }
}

/// The picture at the start of a group's header.
#[derive(Clone)]
pub enum GroupIcon {
    /// An app's icon, when it has one.
    App(Option<Arc<Image>>),
    /// The first letter of the site's domain, which is the group's title.
    Site,
}

/// An app or a site, with the words in its dictionary.
#[derive(Clone)]
pub struct WordGroup {
    /// What tells the groups apart: the app's bundle id or the site's domain.
    pub key: SharedString,
    pub icon: GroupIcon,
    pub title: SharedString,
    pub subtitle: Option<SharedString>,
    pub words: Vec<DictionaryWord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordListEvent {
    /// The user asked for the group to be removed. It is already collapsed.
    RemoveGroup(SharedString),
    /// Answer with [`WordListEditor::finish_adding`] once the word is stored
    /// or has been refused.
    AddWord {
        key: SharedString,
        word: String,
    },
    EditWord {
        word_id: String,
        word: String,
    },
    DeleteWord {
        word_id: String,
    },
}

struct NewWord {
    field: Entity<TextField>,
    _subscription: Subscription,
}

struct Editing {
    word_id: String,
    field: Entity<TextField>,
    _subscription: Subscription,
}

/// The words of several apps or sites: one collapsible group for each, with
/// a form to add a word and a row for every word it has.
pub struct WordListEditor {
    groups: Vec<WordGroup>,
    expanded: HashSet<SharedString>,
    new_words: HashMap<SharedString, NewWord>,
    adding: HashSet<SharedString>,
    /// Why a word could not be added to a group, or the group removed.
    group_errors: HashMap<SharedString, Notice>,
    /// Why a change to a word was refused, by the word's id.
    word_errors: HashMap<String, Notice>,
    editing: Option<Editing>,
    focus_handle: FocusHandle,
}

impl EventEmitter<WordListEvent> for WordListEditor {}

impl WordListEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            groups: Vec::new(),
            expanded: HashSet::new(),
            new_words: HashMap::new(),
            adding: HashSet::new(),
            group_errors: HashMap::new(),
            word_errors: HashMap::new(),
            editing: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Shows `groups`. The editor is not on screen while there are none, and
    /// starts over when it comes back: only which groups are open is kept.
    pub fn set_groups(&mut self, groups: Vec<WordGroup>, cx: &mut Context<Self>) {
        if groups.is_empty() {
            self.new_words.clear();
            self.adding.clear();
            self.group_errors.clear();
            self.word_errors.clear();
            self.editing = None;
        }

        self.groups = groups;
        cx.notify();
    }

    pub fn expand(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.expanded.insert(key);
        cx.notify();
    }

    fn toggle(&mut self, key: SharedString, cx: &mut Context<Self>) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        cx.notify();
    }

    /// Makes the field for the group's next word, the first time the group
    /// is shown open. What is typed there stays while the editor is shown.
    fn ensure_new_word(&mut self, key: &SharedString, window: &mut Window, cx: &mut Context<Self>) {
        if self.new_words.contains_key(key) {
            return;
        }

        let field = new_word_field(window, cx);
        let subscription = cx.subscribe(&field, {
            let key = key.clone();
            move |this, _, event, cx| match event {
                TextFieldEvent::Change => {
                    this.group_errors.remove(&key);
                    cx.notify();
                }
                TextFieldEvent::Enter => this.add_word(&key, cx),
                _ => {}
            }
        });

        self.new_words.insert(
            key.clone(),
            NewWord {
                field,
                _subscription: subscription,
            },
        );
    }

    fn remove_group(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.expanded.remove(&key);
        self.group_errors.remove(&key);
        cx.emit(WordListEvent::RemoveGroup(key));
        cx.notify();
    }

    fn add_word(&mut self, key: &SharedString, cx: &mut Context<Self>) {
        let Some(new_word) = self.new_words.get(key) else {
            return;
        };
        let word = new_word.field.read(cx).value(cx).trim().to_string();

        if word.is_empty() || too_long(&word) || self.adding.contains(key) {
            return;
        }

        self.group_errors.remove(key);
        self.adding.insert(key.clone());
        cx.emit(WordListEvent::AddWord {
            key: key.clone(),
            word,
        });
        cx.notify();
    }

    /// Empties the group's field for the next word, once one has been stored.
    pub fn clear_new_word(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(new_word) = self.new_words.get(key) {
            new_word
                .field
                .update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    /// Lets the group take another word. `error` says why this one was
    /// refused; what was typed stays in the field.
    pub fn finish_adding(&mut self, key: &str, error: Option<Notice>, cx: &mut Context<Self>) {
        self.adding.remove(key);
        if let Some(error) = error {
            self.group_errors.insert(key.to_string().into(), error);
        }
        cx.notify();
    }

    /// Says, on the word, why a change to it was refused.
    pub fn fail_word(&mut self, word_id: String, error: Notice, cx: &mut Context<Self>) {
        self.word_errors.insert(word_id, error);
        cx.notify();
    }

    /// Says, in the group, why it could not be removed. The group is opened,
    /// so that the notice shows once it is back.
    pub fn fail_group(&mut self, key: SharedString, error: Notice, cx: &mut Context<Self>) {
        self.expanded.insert(key.clone());
        self.group_errors.insert(key, error);
        cx.notify();
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

    fn save_edit(&mut self, word_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = &self.editing else {
            return;
        };
        let word = editing.field.read(cx).value(cx).trim().to_string();

        if word.is_empty() || too_long(&word) {
            return;
        }

        self.cancel_edit(window, cx);
        cx.emit(WordListEvent::EditWord {
            word_id: word_id.to_string(),
            word,
        });
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
            WordAction::Delete => {
                self.word_errors.remove(&word.id);
                cx.emit(WordListEvent::DeleteWord {
                    word_id: word.id.clone(),
                });
            }
        }
    }

    fn render_header(
        &self,
        group: &WordGroup,
        open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = palette(cx);
        let key = group.key.clone();
        let word_count = word_count_label(group.words.len(), cx);

        let icon_element = match &group.icon {
            GroupIcon::App(image) => app_icon(image.clone(), &group.title, cx).into_any_element(),
            GroupIcon::Site => site_icon(Some(&group.title), px(32.), cx).into_any_element(),
        };

        div()
            .flex()
            .child(
                div()
                    .id("toggle")
                    .flex_1()
                    .min_w_0()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .on_click(cx.listener({
                        let key = key.clone();
                        move |this, _, _, cx| this.toggle(key.clone(), cx)
                    }))
                    .child(icon_element)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_color(p.txt_primary)
                                    .child(text(group.title.clone()).truncate()),
                            )
                            .children(group.subtitle.clone().map(|subtitle| {
                                div()
                                    .type_xs()
                                    .text_color(p.txt_muted)
                                    .child(text(subtitle).truncate())
                            })),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .type_xs()
                            .text_right()
                            .text_color(p.txt_muted)
                            .child(text(upper(&word_count)).tracking_wider()),
                    )
                    .child(turning(
                        open,
                        icon("chevron-down").size_4().text_color(p.txt_muted),
                    )),
            )
            .child(
                div()
                    .id("remove")
                    .transition_all()
                    .px_3()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .type_xs()
                    .text_right()
                    .text_color(p.txt_muted)
                    .opacity(0.)
                    .group_hover("word-group", |button| button.opacity(1.))
                    .hover(|button| button.text_color(p.ac))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_group(key.clone(), cx);
                    }))
                    .child(text("[DEL]").tracking_wider()),
            )
    }

    fn render_words(
        &self,
        group: &WordGroup,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let p = palette(cx);
        let new_word = self.new_words.get(&group.key)?;
        let key = group.key.clone();

        let form = add_word_form(
            Variant::Nested,
            &new_word.field,
            self.adding.contains(&group.key),
            self.group_errors.get(&group.key),
            cx.listener(move |this, _, _, cx| this.add_word(&key, cx)),
            window,
            cx,
        );

        let rows = group.words.iter().map(|word| {
            let editing = self
                .editing
                .as_ref()
                .filter(|editing| editing.word_id == word.id)
                .map(|editing| &editing.field);

            word_row(
                Variant::Nested,
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
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(p.border)
                .flex()
                .flex_col()
                .gap_2()
                .child(form)
                .when(!group.words.is_empty(), |content| {
                    content.child(div().pt_1().flex().flex_col().gap_1().children(rows))
                }),
        )
    }
}

impl Render for WordListEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        let open_keys: Vec<SharedString> = self
            .groups
            .iter()
            .map(|group| group.key.clone())
            .filter(|key| self.expanded.contains(key))
            .collect();
        for key in &open_keys {
            self.ensure_new_word(key, window, cx);
        }

        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .gap_1()
            .children(self.groups.iter().map(|group| {
                let open = self.expanded.contains(&group.key);

                div()
                    .id(group.key.clone())
                    .group("word-group")
                    .transition_colors()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .hover(|group| group.border_color(p.border_strong))
                    .child(self.render_header(group, open, cx))
                    .when(open, |card| {
                        card.children(self.render_words(group, window, cx))
                    })
            }))
    }
}

/// How many words a list has, such as "4 words".
pub fn word_count_label(count: usize, cx: &App) -> SharedString {
    t_with(cx, "dictionary.wordCount", &[("count", &count.to_string())])
}
