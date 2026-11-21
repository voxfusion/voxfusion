mod api;
pub mod native;

pub use api::*;

use gpui_kit::{App, AppContext as _, Global, Task};
use std::sync::OnceLock;

type BackendJob = Box<dyn FnOnce(&dyn Backend) + Send>;

/// Which dictionary scope's operations are serialized together. Sharing one
/// queue per scope across tab switches and window reopens ensures a fresh tab
/// reads after any write started before the switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryQueue {
    Default,
    Apps,
    Sites,
}

struct GlobalBackend {
    backend: SharedBackend,
    updater: SharedUpdater,
    default_dictionary: OnceLock<SerialCalls>,
    app_dictionaries: OnceLock<SerialCalls>,
    site_dictionaries: OnceLock<SerialCalls>,
}

impl Global for GlobalBackend {}

pub fn init(backend: SharedBackend, updater: SharedUpdater, cx: &mut App) {
    cx.set_global(GlobalBackend {
        backend,
        updater,
        default_dictionary: OnceLock::new(),
        app_dictionaries: OnceLock::new(),
        site_dictionaries: OnceLock::new(),
    });
}

pub fn backend(cx: &App) -> SharedBackend {
    cx.global::<GlobalBackend>().backend.clone()
}

pub fn updater(cx: &App) -> SharedUpdater {
    cx.global::<GlobalBackend>().updater.clone()
}

/// Runs a backend call on its own thread, since any of them may block: on
/// the database, on CoreAudio, or for the length of a download.
pub fn call<T: Send + 'static>(
    cx: &App,
    request: impl FnOnce(&dyn Backend) -> T + Send + 'static,
) -> Task<T> {
    run_off_thread(cx, backend(cx), move |backend| request(&**backend))
}

/// Runs a dictionary backend call on the shared FIFO queue for `queue`.
pub fn call_dictionary<T: Send + 'static>(
    cx: &App,
    queue: DictionaryQueue,
    request: impl FnOnce(&dyn Backend) -> T + Send + 'static,
) -> Task<T> {
    let global = cx.global::<GlobalBackend>();
    let slot = match queue {
        DictionaryQueue::Default => &global.default_dictionary,
        DictionaryQueue::Apps => &global.app_dictionaries,
        DictionaryQueue::Sites => &global.site_dictionaries,
    };
    let serial = slot.get_or_init(|| SerialCalls::new(global.backend.clone()));
    serial.call(cx, request)
}

/// Runs backend calls one at a time, in submission order, on a single worker
/// thread.
struct SerialCalls {
    sender: async_channel::Sender<BackendJob>,
}

impl SerialCalls {
    fn new(service: SharedBackend) -> Self {
        let (sender, receiver) = async_channel::unbounded::<BackendJob>();

        std::thread::spawn(move || {
            while let Ok(job) = receiver.recv_blocking() {
                job(&*service);
            }
        });

        Self { sender }
    }

    fn call<T: Send + 'static>(
        &self,
        cx: &App,
        request: impl FnOnce(&dyn Backend) -> T + Send + 'static,
    ) -> Task<T> {
        let (reply_tx, reply_rx) = async_channel::bounded(1);
        let _ = self.sender.try_send(Box::new(move |backend| {
            let _ = reply_tx.send_blocking(request(backend));
        }));

        cx.background_spawn(async move {
            reply_rx
                .recv()
                .await
                .expect("a serialized backend call always reports its result")
        })
    }
}

/// [`call`] for the updater.
pub fn call_updater<T: Send + 'static>(
    cx: &App,
    request: impl FnOnce(&dyn Updater) -> T + Send + 'static,
) -> Task<T> {
    run_off_thread(cx, updater(cx), move |updater| request(&**updater))
}

fn run_off_thread<S: Send + 'static, T: Send + 'static>(
    cx: &App,
    service: S,
    request: impl FnOnce(&S) -> T + Send + 'static,
) -> Task<T> {
    let (sender, receiver) = async_channel::bounded(1);

    std::thread::spawn(move || {
        let _ = sender.send_blocking(request(&service));
    });

    cx.background_spawn(async move {
        receiver
            .recv()
            .await
            .expect("a backend call always reports its result")
    })
}

/// The message the interface shows for a failed call.
pub fn command_error(command: &str, error: &str) -> String {
    format!("{command} failed: {error}")
}
