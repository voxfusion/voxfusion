//! Keeps the app to one running copy per profile. A second copy asks the
//! first to show its main window and exits.
//!
//! A lock on a file in the data directory decides which copy runs. The
//! system drops the lock when its holder exits, however it exits, so a crash
//! leaves nothing to clean up. The request to show the window travels over a
//! Unix socket beside the lock.

use std::fs::{self, File, TryLockError};
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use crate::backend::{AppEvent, EventSender};
use crate::paths;

const LOCK_FILE: &str = "instance.lock";
const SOCKET_FILE: &str = "instance.sock";

static RUNNING: Mutex<Option<Instance>> = Mutex::new(None);

/// Makes this the running copy of the app. Returns `false` when another copy
/// already is: that one has been asked to show its main window, which
/// reaches it as [`AppEvent::ShowMainWindow`], and this one should exit.
pub fn acquire(events: EventSender) -> bool {
    match Instance::claim(&paths::data_dir(), events) {
        Ok(Some(instance)) => {
            *RUNNING.lock().unwrap_or_else(PoisonError::into_inner) = Some(instance);
            true
        }
        Ok(None) => false,
        Err(error) => {
            // Without a lock there is no telling; running twice is the
            // lesser evil compared to not starting.
            log::warn!(target: "runtime", "single_instance_unavailable error={error}");
            true
        }
    }
}

/// Lets another copy start while this one is still running. A relaunch needs
/// this: the new copy starts before the old one is gone.
pub fn release() {
    let running = RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();

    if let Some(instance) = running {
        instance.release();
    }
}

struct Instance {
    lock: File,
    socket: PathBuf,
}

impl Instance {
    /// `None` when another copy holds the lock in `directory`.
    fn claim(directory: &Path, events: EventSender) -> io::Result<Option<Self>> {
        fs::create_dir_all(directory)?;

        let socket = directory.join(SOCKET_FILE);
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(LOCK_FILE))?;

        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                // A copy that has the lock but no socket yet is still
                // starting, and shows its window anyway.
                if let Err(error) = UnixStream::connect(&socket) {
                    log::warn!(target: "runtime", "single_instance_notify_failed error={error}");
                }

                return Ok(None);
            }
            Err(TryLockError::Error(error)) => return Err(error),
        }

        // Holding the lock is what makes this the running copy. Failing to
        // listen only means later copies exit without showing the window.
        if let Err(error) = listen(&socket, events) {
            log::warn!(target: "runtime", "single_instance_listen_failed error={error}");
        }

        Ok(Some(Self { lock, socket }))
    }

    fn release(self) {
        let _ = fs::remove_file(&self.socket);
        let _ = self.lock.unlock();
    }
}

fn listen(socket: &Path, events: EventSender) -> io::Result<()> {
    // The socket file of a copy that crashed is still there, and no one can
    // be listening on it while this copy holds the lock.
    match fs::remove_file(socket) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }

    let listener = UnixListener::bind(socket)?;

    thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || {
            for connection in listener.incoming() {
                match connection {
                    Ok(_) => {
                        log::info!(target: "runtime", "single_instance_requested");
                        events.emit(AppEvent::ShowMainWindow);
                    }
                    // Most likely out of file descriptors; do not spin while
                    // that lasts.
                    Err(_) => thread::sleep(Duration::from_secs(1)),
                }
            }
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn events() -> (EventSender, async_channel::Receiver<AppEvent>) {
        let (sender, receiver) = async_channel::unbounded();
        (EventSender(sender), receiver)
    }

    fn next_event(receiver: &async_channel::Receiver<AppEvent>) -> Option<AppEvent> {
        let deadline = Instant::now() + Duration::from_secs(5);

        while Instant::now() < deadline {
            if let Ok(event) = receiver.try_recv() {
                return Some(event);
            }
            thread::sleep(Duration::from_millis(5));
        }

        None
    }

    #[test]
    fn a_second_copy_defers_to_the_first_and_asks_for_its_window() {
        let directory = tempfile::tempdir().unwrap();
        let (first_events, first_received) = events();

        let first = Instance::claim(directory.path(), first_events).unwrap();
        assert!(first.is_some());

        let second = Instance::claim(directory.path(), events().0).unwrap();
        assert!(second.is_none());

        assert_eq!(next_event(&first_received), Some(AppEvent::ShowMainWindow));
    }

    #[test]
    fn every_later_copy_asks_again() {
        let directory = tempfile::tempdir().unwrap();
        let (first_events, first_received) = events();
        let _first = Instance::claim(directory.path(), first_events).unwrap();

        for _ in 0..3 {
            assert!(
                Instance::claim(directory.path(), events().0)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(next_event(&first_received), Some(AppEvent::ShowMainWindow));
        }
    }

    #[test]
    fn takes_over_after_a_copy_that_crashed() {
        let directory = tempfile::tempdir().unwrap();

        // Dropped without releasing, as when the process dies: the lock goes
        // with it, the socket file stays.
        let crashed = Instance::claim(directory.path(), events().0).unwrap();
        drop(crashed);
        assert!(directory.path().join(SOCKET_FILE).exists());

        let (events_after, received_after) = events();
        let after = Instance::claim(directory.path(), events_after).unwrap();
        assert!(after.is_some());

        assert!(
            Instance::claim(directory.path(), events().0)
                .unwrap()
                .is_none()
        );
        assert_eq!(next_event(&received_after), Some(AppEvent::ShowMainWindow));
    }

    #[test]
    fn a_released_copy_makes_room_for_the_next() {
        let directory = tempfile::tempdir().unwrap();
        let (old_events, old_received) = events();

        let old = Instance::claim(directory.path(), old_events)
            .unwrap()
            .unwrap();
        old.release();
        assert!(!directory.path().join(SOCKET_FILE).exists());

        let (new_events, new_received) = events();
        let new = Instance::claim(directory.path(), new_events).unwrap();
        assert!(new.is_some());

        assert!(
            Instance::claim(directory.path(), events().0)
                .unwrap()
                .is_none()
        );
        assert_eq!(next_event(&new_received), Some(AppEvent::ShowMainWindow));
        assert!(old_received.try_recv().is_err());
    }

    #[test]
    fn profiles_in_different_directories_do_not_see_each_other() {
        let personal = tempfile::tempdir().unwrap();
        let testing = tempfile::tempdir().unwrap();

        let first = Instance::claim(personal.path(), events().0).unwrap();
        let second = Instance::claim(testing.path(), events().0).unwrap();

        assert!(first.is_some());
        assert!(second.is_some());
    }

    #[test]
    fn still_runs_alone_when_the_socket_cannot_be_created() {
        let directory = tempfile::tempdir().unwrap();

        // Socket paths are limited to about a hundred bytes.
        let deep = directory.path().join("d".repeat(120));
        let first = Instance::claim(&deep, events().0).unwrap();
        assert!(first.is_some());
        assert!(!deep.join(SOCKET_FILE).exists());

        assert!(Instance::claim(&deep, events().0).unwrap().is_none());
    }
}
