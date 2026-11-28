//! Keeps the app to one running copy per profile. A second copy asks the
//! first to show its main window and exits.
//!
//! A lock on a file in the data directory decides which copy runs. The
//! system drops the lock when its holder exits, however it exits, so a crash
//! leaves nothing to clean up. The request to show the window travels over a
//! Unix socket beside the lock.

use std::fs::{self, File, TryLockError};
use std::io;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use crate::backend::{AppEvent, EventSender};
use crate::paths;

const LOCK_FILE: &str = "instance.lock";
#[cfg(unix)]
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
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(windows)]
    pipe_name: Vec<u16>,
    #[cfg(windows)]
    stop: Arc<AtomicBool>,
    #[cfg(windows)]
    listener: Option<thread::JoinHandle<()>>,
}

impl Instance {
    /// `None` when another copy holds the lock in `directory`.
    fn claim(directory: &Path, events: EventSender) -> io::Result<Option<Self>> {
        fs::create_dir_all(directory)?;

        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(LOCK_FILE))?;

        #[cfg(unix)]
        {
            let socket = directory.join(SOCKET_FILE);

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

        #[cfg(windows)]
        {
            let pipe_name = windows_pipe::pipe_name(directory);

            match lock.try_lock() {
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => {
                    if let Err(error) = windows_pipe::notify(&pipe_name) {
                        log::warn!(target: "runtime", "single_instance_notify_failed error={error}");
                    }
                    return Ok(None);
                }
                Err(TryLockError::Error(error)) => return Err(error),
            }

            let stop = Arc::new(AtomicBool::new(false));
            let listener = match windows_pipe::listen(pipe_name.clone(), stop.clone(), events) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    log::warn!(target: "runtime", "single_instance_listen_failed error={error}");
                    None
                }
            };

            Ok(Some(Self {
                lock,
                pipe_name,
                stop,
                listener,
            }))
        }
    }

    fn release(self) {
        #[cfg(unix)]
        let _ = fs::remove_file(&self.socket);
        let _ = self.lock.unlock();
        drop(self);
    }
}

#[cfg(windows)]
impl Drop for Instance {
    fn drop(&mut self) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            let _ = windows_pipe::notify(&self.pipe_name);
        }
        if let Some(handle) = self.listener.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(unix)]
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

#[cfg(windows)]
mod windows_pipe {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::ffi::c_void;
    use std::time::Instant;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
        HANDLE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING,
        PIPE_ACCESS_INBOUND,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
    };
    use windows_core::PCWSTR;

    const NOTIFY_TIMEOUT: Duration = Duration::from_millis(500);
    const MAX_PIPE_INSTANCES: u32 = 4;

    pub fn pipe_name(directory: &Path) -> Vec<u16> {
        let canonical = fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
        let normalized = canonical.to_string_lossy().to_ascii_lowercase();
        let digest = Sha256::digest(normalized.as_bytes());
        let mut hex = String::with_capacity(32);
        for byte in &digest[..16] {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
        }
        let name = format!(r"\\.\pipe\voxfusion-instance-{hex}");
        name.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn create_pipe(pipe_name: &[u16]) -> io::Result<HANDLE> {
        let pipe = unsafe {
            CreateNamedPipeW(
                PCWSTR(pipe_name.as_ptr()),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                MAX_PIPE_INSTANCES,
                0,
                0,
                0,
                None,
            )
        };
        if pipe.is_invalid() {
            Err(io::Error::last_os_error())
        } else {
            Ok(pipe)
        }
    }

    pub fn notify(pipe_name: &[u16]) -> io::Result<()> {
        let deadline = Instant::now() + NOTIFY_TIMEOUT;
        loop {
            match unsafe {
                CreateFileW(
                    PCWSTR(pipe_name.as_ptr()),
                    FILE_GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            } {
                Ok(handle) => {
                    unsafe {
                        let _ = CloseHandle(handle);
                    }
                    return Ok(());
                }
                Err(err) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(io::Error::other(err.to_string()));
                    }
                    if err.code() == ERROR_PIPE_BUSY.to_hresult() {
                        let remaining_ms = deadline.duration_since(now).as_millis().min(50) as u32;
                        let _ = unsafe {
                            WaitNamedPipeW(PCWSTR(pipe_name.as_ptr()), remaining_ms.max(10))
                        };
                    } else if err.code() == ERROR_FILE_NOT_FOUND.to_hresult() {
                        thread::sleep(Duration::from_millis(20));
                    } else {
                        return Err(io::Error::other(err.to_string()));
                    }
                }
            }
        }
    }

    pub fn listen(
        pipe_name: Vec<u16>,
        stop: Arc<AtomicBool>,
        events: EventSender,
    ) -> io::Result<thread::JoinHandle<()>> {
        // Create the first pipe instance synchronously before `claim()` returns
        // so an immediate second launch can connect without racing thread start.
        let first_pipe = create_pipe(&pipe_name)?.0 as isize;

        thread::Builder::new()
            .name("single-instance".into())
            .spawn(move || {
                let mut pending_pipe = Some(HANDLE(first_pipe as *mut c_void));

                while !stop.load(Ordering::SeqCst) {
                    let pipe = match pending_pipe.take() {
                        Some(handle) => handle,
                        None => match create_pipe(&pipe_name) {
                            Ok(handle) => handle,
                            Err(_) => {
                                thread::sleep(Duration::from_secs(1));
                                continue;
                            }
                        },
                    };

                    // On a freshly created pipe instance, `ERROR_PIPE_CONNECTED` means a
                    // client connected before `ConnectNamedPipe` ran, and `ERROR_NO_DATA`
                    // means it already connected and closed its handle before we got here.
                    let connected = match unsafe { ConnectNamedPipe(pipe, None) } {
                        Ok(()) => true,
                        Err(err) => {
                            err.code() == ERROR_PIPE_CONNECTED.to_hresult()
                                || err.code() == ERROR_NO_DATA.to_hresult()
                        }
                    };

                    if connected && !stop.load(Ordering::SeqCst) {
                        log::info!(target: "runtime", "single_instance_requested");
                        events.emit(AppEvent::ShowMainWindow);
                    }

                    unsafe {
                        let _ = DisconnectNamedPipe(pipe);
                        let _ = CloseHandle(pipe);
                    }
                }

                if let Some(handle) = pending_pipe {
                    unsafe {
                        let _ = CloseHandle(handle);
                    }
                }
            })
    }
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
        // with it, the socket file stays on Unix.
        let crashed = Instance::claim(directory.path(), events().0).unwrap();
        drop(crashed);
        #[cfg(unix)]
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
        #[cfg(unix)]
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

    #[cfg(unix)]
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
