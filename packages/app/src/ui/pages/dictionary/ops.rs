//! App-owned serialized queues and restore tracking for dictionary operations.
//!
//! Each dictionary scope (`Default`, `Apps`, `Sites`) executes its backend
//! calls in FIFO order on a dedicated worker thread, and accepted Undo restores
//! are tracked in an app-level [`RestoreStore`], so:
//! - Accepted Undo restores finish even after switching tabs or leaving
//!   Dictionary, and failed restores keep their error notice and retry action
//!   across navigation.
//! - A `list_*` call started after a delete or restore always observes the
//!   committed post-mutation database state.
//! - A group removal queued after a word restore always runs after that
//!   restore and cannot be undone by it.

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global, SharedString, Task};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::backend::{self, Backend, CommandResult, DictionaryWord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueueScope {
    Default,
    Apps,
    Sites,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreTarget {
    Default,
    App { bundle_id: String, app_name: String },
    Site { domain: String },
}

impl RestoreTarget {
    pub fn group_key(&self) -> Option<&str> {
        match self {
            RestoreTarget::Default => None,
            RestoreTarget::App { bundle_id, .. } => Some(bundle_id),
            RestoreTarget::Site { domain } => Some(domain),
        }
    }

    pub fn app_name(&self) -> Option<&str> {
        match self {
            RestoreTarget::App { app_name, .. } => Some(app_name),
            RestoreTarget::Default | RestoreTarget::Site { .. } => None,
        }
    }

    fn restore(
        &self,
        backend: &dyn Backend,
        word: &DictionaryWord,
    ) -> CommandResult<DictionaryWord> {
        match self {
            RestoreTarget::Default => backend.restore_dictionary_word(word),
            RestoreTarget::App {
                bundle_id,
                app_name,
            } => backend.restore_app_dictionary_word(bundle_id, app_name, word),
            RestoreTarget::Site { domain } => backend.restore_site_dictionary_word(domain, word),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreState {
    /// Queued or running on the scope's FIFO worker.
    Restoring,
    /// Backend refused the restore; the user can retry or dismiss.
    Failed(SharedString),
}

#[derive(Clone)]
pub struct TrackedRestore {
    pub token: u64,
    pub scope: QueueScope,
    pub target: RestoreTarget,
    pub word: DictionaryWord,
    pub index: usize,
    ticket: DeleteTicket,
    pub state: RestoreState,
    cancelled: Arc<AtomicBool>,
}

/// Outcome of a queued word restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// The word was restored in storage.
    Restored,
    /// The preceding delete found no stored row, so nothing needed to be
    /// restored.
    NotFound,
    /// The preceding delete failed, so the word remained in storage and the
    /// view should report the delete error after reconciling.
    DeleteFailed(String),
    /// Restore was cancelled before running (for example, its group was removed).
    Cancelled,
}

/// Events emitted by [`RestoreStore`] when an accepted restore finishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreEvent {
    Settled {
        scope: QueueScope,
        token: u64,
        word_id: String,
        group_key: Option<String>,
        outcome: RestoreOutcome,
    },
    Failed {
        scope: QueueScope,
        token: u64,
        word_id: String,
    },
}

/// Outcome of a queued word deletion, recorded on the FIFO worker so a
/// follow-up restore distinguishes a deleted row (with its authoritative saved
/// copy) from a missing row or a failed delete.
#[derive(Clone, Debug, Default)]
pub enum DeleteOutcome {
    #[default]
    NotDeleted,
    Deleted(DictionaryWord),
    Failed(String),
}

/// Shared handle carrying the [`DeleteOutcome`] from a delete job to any
/// follow-up restore queued for the same deletion.
#[derive(Clone, Default)]
pub struct DeleteTicket {
    outcome: Arc<Mutex<DeleteOutcome>>,
}

impl DeleteTicket {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_deleted(&self, word: DictionaryWord) {
        *self
            .outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = DeleteOutcome::Deleted(word);
    }

    pub fn record_failed(&self, error: String) {
        *self
            .outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = DeleteOutcome::Failed(error);
    }

    pub fn outcome(&self) -> DeleteOutcome {
        self.outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn deleted_row(&self) -> Option<DictionaryWord> {
        match self.outcome() {
            DeleteOutcome::Deleted(word) => Some(word),
            DeleteOutcome::NotDeleted | DeleteOutcome::Failed(_) => None,
        }
    }
}

type Job = Box<dyn FnOnce(&dyn Backend) + Send>;

#[derive(Default)]
struct DictionaryQueues {
    workers: HashMap<QueueScope, async_channel::Sender<Job>>,
    next_token: u64,
    store: Option<Entity<RestoreStore>>,
}

impl Global for DictionaryQueues {}

/// App-level tracker for accepted Undo restores so in-flight and failed
/// restores survive tab switches and leaving the Dictionary page.
#[derive(Default)]
pub struct RestoreStore {
    entries: Vec<TrackedRestore>,
}

impl EventEmitter<RestoreEvent> for RestoreStore {}

impl RestoreStore {
    pub fn entries_for(&self, scope: QueueScope) -> Vec<TrackedRestore> {
        self.entries
            .iter()
            .filter(|entry| entry.scope == scope)
            .cloned()
            .collect()
    }

    fn enqueue_restore(
        &mut self,
        token: u64,
        scope: QueueScope,
        target: RestoreTarget,
        word_id: String,
        ticket: DeleteTicket,
        cancelled: Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) {
        let group_key = target.group_key().map(str::to_string);
        let cancelled_for_worker = cancelled.clone();

        let request = run(
            scope,
            cx,
            move |backend| -> Result<RestoreOutcome, (String, DictionaryWord)> {
                if cancelled_for_worker.load(Ordering::Acquire) {
                    return Ok(RestoreOutcome::Cancelled);
                }
                let saved_word = match ticket.outcome() {
                    DeleteOutcome::Deleted(saved) => saved,
                    DeleteOutcome::NotDeleted => return Ok(RestoreOutcome::NotFound),
                    DeleteOutcome::Failed(error) => return Ok(RestoreOutcome::DeleteFailed(error)),
                };
                match target.restore(backend, &saved_word) {
                    Ok(_) => Ok(RestoreOutcome::Restored),
                    Err(error) => {
                        log::error!(target: "dictionary", "restore_failed scope={scope:?} error={error}");
                        Err((error, saved_word))
                    }
                }
            },
        );

        cx.spawn(async move |this, cx| match request.await {
            Ok(outcome) => {
                this.update(cx, |store, cx| {
                    store.entries.retain(|entry| entry.token != token);
                    cx.emit(RestoreEvent::Settled {
                        scope,
                        token,
                        word_id,
                        group_key,
                        outcome,
                    });
                    cx.notify();
                })
                .ok();
            }
            Err((error, saved_word)) => {
                this.update(cx, |store, cx| {
                    if cancelled.load(Ordering::Acquire) {
                        store.entries.retain(|entry| entry.token != token);
                    } else if let Some(entry) =
                        store.entries.iter_mut().find(|entry| entry.token == token)
                    {
                        entry.word = saved_word;
                        entry.state = RestoreState::Failed(error.into());
                    }
                    cx.emit(RestoreEvent::Failed {
                        scope,
                        token,
                        word_id,
                    });
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

fn worker(scope: QueueScope, cx: &mut App) -> async_channel::Sender<Job> {
    if let Some(sender) = cx
        .try_global::<DictionaryQueues>()
        .and_then(|queues| queues.workers.get(&scope))
    {
        return sender.clone();
    }

    let backend = backend::backend(cx);
    let (sender, receiver) = async_channel::unbounded::<Job>();
    let thread_name = match scope {
        QueueScope::Default => "dictionary-default-ops",
        QueueScope::Apps => "dictionary-apps-ops",
        QueueScope::Sites => "dictionary-sites-ops",
    };

    std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            while let Ok(job) = receiver.recv_blocking() {
                job(&*backend);
            }
        })
        .expect("dictionary worker thread spawns");

    cx.default_global::<DictionaryQueues>()
        .workers
        .insert(scope, sender.clone());
    sender
}

/// Returns the app-level [`RestoreStore`], creating it on first use.
pub fn store(cx: &mut App) -> Entity<RestoreStore> {
    if let Some(store) = cx
        .try_global::<DictionaryQueues>()
        .and_then(|queues| queues.store.clone())
    {
        return store;
    }

    let store = cx.new(|_| RestoreStore::default());
    cx.default_global::<DictionaryQueues>().store = Some(store.clone());
    store
}

/// Allocates a unique token shared across local Undo rows and persisted
/// [`TrackedRestore`] entries.
pub fn next_token(cx: &mut App) -> u64 {
    let queues = cx.default_global::<DictionaryQueues>();
    queues.next_token += 1;
    queues.next_token
}

/// Runs `request` on `scope`'s FIFO backend worker and resolves with its result.
/// The job runs to completion even if the calling view is dropped.
pub fn run<T: Send + 'static>(
    scope: QueueScope,
    cx: &mut App,
    request: impl FnOnce(&dyn Backend) -> T + Send + 'static,
) -> Task<T> {
    let sender = worker(scope, cx);
    let (reply_tx, reply_rx) = async_channel::bounded(1);

    let sent = sender.try_send(Box::new(move |backend| {
        let _ = reply_tx.send_blocking(request(backend));
    }));
    debug_assert!(sent.is_ok(), "dictionary worker channel stays open");

    cx.background_spawn(async move {
        reply_rx
            .recv()
            .await
            .expect("dictionary worker always reports its result")
    })
}

/// Accepts an Undo request, tracking it in [`RestoreStore`] and queuing the
/// restore onto `scope`'s FIFO worker so it finishes across navigation.
pub fn start_restore(
    token: u64,
    scope: QueueScope,
    target: RestoreTarget,
    word: DictionaryWord,
    index: usize,
    ticket: DeleteTicket,
    cx: &mut App,
) {
    let store = store(cx);
    store.update(cx, |store, cx| {
        if store.entries.iter().any(|entry| entry.token == token) {
            return;
        }
        let word = ticket.deleted_row().unwrap_or(word);
        let word_id = word.id.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        store.entries.push(TrackedRestore {
            token,
            scope,
            target: target.clone(),
            word,
            index,
            ticket: ticket.clone(),
            state: RestoreState::Restoring,
            cancelled: cancelled.clone(),
        });
        cx.notify();
        store.enqueue_restore(token, scope, target, word_id, ticket, cancelled, cx);
    });
}

/// Updates the displayed word on a tracked restore once the preceding delete
/// returns the authoritative row removed from storage.
pub fn sync_deleted_word(token: u64, word: DictionaryWord, cx: &mut App) {
    let Some(store) = cx
        .try_global::<DictionaryQueues>()
        .and_then(|queues| queues.store.clone())
    else {
        return;
    };
    store.update(cx, |store, cx| {
        if let Some(entry) = store.entries.iter_mut().find(|entry| entry.token == token)
            && entry.word != word
        {
            entry.word = word;
            cx.notify();
        }
    });
}

/// Retries a previously failed restore identified by `token`.
pub fn retry_restore(token: u64, cx: &mut App) {
    let store = store(cx);
    store.update(cx, |store, cx| {
        let Some(entry) = store.entries.iter_mut().find(|entry| entry.token == token) else {
            return;
        };
        if matches!(entry.state, RestoreState::Restoring) {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        entry.state = RestoreState::Restoring;
        entry.cancelled = cancelled.clone();
        let scope = entry.scope;
        let target = entry.target.clone();
        let word_id = entry.word.id.clone();
        let ticket = entry.ticket.clone();
        cx.notify();
        store.enqueue_restore(token, scope, target, word_id, ticket, cancelled, cx);
    });
}

/// Dismisses a failed restore identified by `token`.
pub fn dismiss_restore(token: u64, cx: &mut App) {
    let store = store(cx);
    store.update(cx, |store, cx| {
        let before = store.entries.len();
        store
            .entries
            .retain(|entry| entry.token != token || matches!(entry.state, RestoreState::Restoring));
        if store.entries.len() != before {
            cx.notify();
        }
    });
}

/// Cancels and removes all tracked restores belonging to `group_key` in `scope`.
pub fn cancel_group_restores(scope: QueueScope, group_key: &str, cx: &mut App) {
    let store = store(cx);
    store.update(cx, |store, cx| {
        let before = store.entries.len();
        store.entries.retain(|entry| {
            if entry.scope == scope && entry.target.group_key() == Some(group_key) {
                entry.cancelled.store(true, Ordering::Release);
                return false;
            }
            true
        });
        if store.entries.len() != before {
            cx.notify();
        }
    });
}

/// Shifts tracked restore indices down when a word is added at the top.
pub fn shift_indices_on_add(scope: QueueScope, group_key: Option<&str>, cx: &mut App) {
    let store = store(cx);
    store.update(cx, |store, cx| {
        let mut changed = false;
        for entry in &mut store.entries {
            if entry.scope == scope && entry.target.group_key() == group_key {
                entry.index += 1;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    });
}

/// Shifts tracked restore indices up when a word above them is deleted.
pub fn shift_indices_on_delete(
    scope: QueueScope,
    group_key: Option<&str>,
    deleted_index: usize,
    cx: &mut App,
) {
    let store = store(cx);
    store.update(cx, |store, cx| {
        let mut changed = false;
        for entry in &mut store.entries {
            if entry.scope == scope
                && entry.target.group_key() == group_key
                && entry.index > deleted_index
            {
                entry.index -= 1;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    });
}
