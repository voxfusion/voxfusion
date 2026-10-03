//! Muffling turns the system output down for the length of a recording
//! instead of muting it.
//!
//! A single worker thread owns every volume change. Commands only post a
//! message, so they return at once and never delay the start of a recording.
//! A fade that is still running is retargeted by the next command instead of
//! racing it: a recording that starts while the previous one is still fading
//! back up fades down from wherever the volume is, and still restores the
//! original level afterward.

use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// Share of the original volume that stays audible while muffled.
const MUFFLED_VOLUME_RATIO: f32 = 0.35;
const FADE_DOWN: Duration = Duration::from_millis(150);
const FADE_UP: Duration = Duration::from_millis(300);
const FADE_STEP: Duration = Duration::from_millis(15);
const EXIT_RESTORE_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) type DeviceId = u32;

/// The system output volume, as the macOS volume slider sees it.
pub(super) trait OutputVolume {
    fn default_output_device(&self) -> Result<DeviceId, String>;
    fn device_uid(&self, device: DeviceId) -> Option<String>;
    fn find_device_by_uid(&self, uid: &str) -> Option<DeviceId>;
    /// Volume from 0 to 1, or an error when the device has no volume the app
    /// can change.
    fn adjustable_volume(&self, device: DeviceId) -> Result<f32, String>;
    fn set_volume(&self, device: DeviceId, volume: f32) -> Result<(), String>;
}

enum Command {
    Muffle,
    Restore,
    /// Restores without a fade, then acknowledges.
    RestoreNow(Sender<()>),
    DevicesChanged,
}

static COMMANDS: OnceLock<Sender<Command>> = OnceLock::new();

pub(super) fn muffle() {
    let commands = COMMANDS.get_or_init(|| spawn(super::CoreAudioVolume));
    let _ = commands.send(Command::Muffle);
}

/// Messages the worker only if muffling has ever started it; otherwise there
/// is nothing to restore.
fn send_if_running(command: Command) {
    if let Some(commands) = COMMANDS.get() {
        let _ = commands.send(command);
    }
}

pub(super) fn restore() {
    send_if_running(Command::Restore);
}

/// Restores the output before the app quits, waiting briefly for it.
pub(super) fn restore_now() {
    let Some(commands) = COMMANDS.get() else {
        return;
    };
    let (done, finished) = mpsc::channel();
    if commands.send(Command::RestoreNow(done)).is_ok() {
        let _ = finished.recv_timeout(EXIT_RESTORE_TIMEOUT);
    }
}

pub(super) fn devices_changed() {
    send_if_running(Command::DevicesChanged);
}

fn spawn<V: OutputVolume + Send + 'static>(volume: V) -> Sender<Command> {
    let (commands, received) = mpsc::channel();
    // If the thread cannot start, the receiver is dropped with it and
    // muffling quietly does nothing.
    if let Err(err) = std::thread::Builder::new()
        .name("media-muffle".to_string())
        .spawn(move || run(Muffler::new(volume), received))
    {
        log::warn!(target: "media", "muffle_worker_spawn_failed error={err}");
    }
    commands
}

fn run<V: OutputVolume>(mut muffler: Muffler<V>, commands: Receiver<Command>) {
    loop {
        let received = if muffler.is_fading() {
            commands.recv_timeout(FADE_STEP)
        } else {
            commands.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match received {
            Ok(command) => muffler.handle(command, Instant::now()),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        muffler.step(Instant::now());
    }
}

struct Fade {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
}

impl Fade {
    fn volume_at(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.started);
        let progress = (elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
        self.from + (self.to - self.from) * progress
    }

    fn is_done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= self.duration
    }
}

struct Session {
    device: DeviceId,
    uid: Option<String>,
    original: f32,
    /// Volume last set on the device.
    current: f32,
    restoring: bool,
    fade: Option<Fade>,
}

impl Session {
    fn fade_to(&mut self, to: f32, duration: Duration, now: Instant) {
        self.fade = Some(Fade {
            from: self.current,
            to,
            started: now,
            duration,
        });
    }
}

struct Muffler<V> {
    volume: V,
    /// The muffled device, from the start of the fade down to the end of the
    /// fade back up.
    session: Option<Session>,
    /// UIDs and original volumes of devices that were unplugged while
    /// muffled. macOS restores a device's saved volume when it reconnects, so
    /// they are turned back up once they are back.
    pending_restores: Vec<(String, f32)>,
}

impl<V: OutputVolume> Muffler<V> {
    fn new(volume: V) -> Self {
        Self {
            volume,
            session: None,
            pending_restores: Vec::new(),
        }
    }

    fn is_fading(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.fade.is_some())
    }

    fn handle(&mut self, command: Command, now: Instant) {
        match command {
            Command::Muffle => self.muffle(now),
            Command::Restore => self.restore(FADE_UP, now),
            Command::RestoreNow(done) => {
                self.restore(Duration::ZERO, now);
                self.step(now);
                let _ = done.send(());
            }
            Command::DevicesChanged => self.restore_returned_devices(),
        }
    }

    fn muffle(&mut self, now: Instant) {
        self.restore_returned_devices();
        let default_device = self.volume.default_output_device();

        if let Some(session) = self.session.as_mut() {
            if !session.restoring {
                return;
            }
            // The previous recording is still fading back up.
            if default_device
                .as_ref()
                .is_ok_and(|device| *device == session.device)
            {
                session.restoring = false;
                session.fade_to(session.original * MUFFLED_VOLUME_RATIO, FADE_DOWN, now);
                return;
            }
        }
        if self.session.is_some() {
            // The output moved to another device; finish restoring the old one.
            self.restore(Duration::ZERO, now);
            self.step(now);
        }

        let device = match default_device {
            Ok(device) => device,
            Err(err) => {
                log::warn!(target: "media", "muffle_failed error={err}");
                return;
            }
        };
        let original = match self.volume.adjustable_volume(device) {
            Ok(volume) => volume,
            Err(err) => {
                log::warn!(target: "media", "muffle_unsupported device_id={device} error={err}");
                return;
            }
        };
        let mut session = Session {
            device,
            uid: self.volume.device_uid(device),
            original,
            current: original,
            restoring: false,
            fade: None,
        };
        session.fade_to(original * MUFFLED_VOLUME_RATIO, FADE_DOWN, now);
        self.session = Some(session);
    }

    fn restore(&mut self, duration: Duration, now: Instant) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.restoring && !duration.is_zero() {
            return;
        }
        // A device unplugged and reconnected mid-recording can come back
        // under a new ID, so look it up by UID first.
        if let Some(uid) = &session.uid {
            let Some(device) = self.volume.find_device_by_uid(uid) else {
                log::info!(
                    target: "media",
                    "muffled_output_disconnected device_id={}",
                    session.device
                );
                self.pending_restores.push((uid.clone(), session.original));
                self.session = None;
                return;
            };
            session.device = device;
        }
        session.restoring = true;
        session.fade_to(session.original, duration, now);
    }

    fn step(&mut self, now: Instant) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(fade) = &session.fade else {
            return;
        };
        let volume = fade.volume_at(now);
        let done = fade.is_done(now);

        if let Err(err) = self.volume.set_volume(session.device, volume) {
            log::warn!(
                target: "media",
                "muffle_volume_failed device_id={} error={err}",
                session.device
            );
            // Most likely unplugged. A failed fade down is restored with the
            // recording; a failed fade up is retried when the device is back.
            session.fade = None;
            if session.restoring {
                if let Some(uid) = session.uid.take() {
                    self.pending_restores.push((uid, session.original));
                }
                self.session = None;
            }
            return;
        }

        session.current = volume;
        if done {
            session.fade = None;
            if session.restoring {
                self.session = None;
            }
        }
    }

    fn restore_returned_devices(&mut self) {
        let volume = &self.volume;
        self.pending_restores.retain(|(uid, original)| {
            let Some(device) = volume.find_device_by_uid(uid) else {
                return true;
            };
            match volume.set_volume(device, *original) {
                Ok(()) => {
                    log::info!(target: "media", "returned_output_unmuffled device_id={device}");
                    false
                }
                Err(err) => {
                    log::warn!(
                        target: "media",
                        "returned_output_unmuffle_failed device_id={device} error={err}"
                    );
                    true
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct FakeDevice {
        uid: String,
        volume: f32,
        connected: bool,
    }

    #[derive(Default)]
    struct FakeOutput {
        default: Mutex<DeviceId>,
        devices: Mutex<HashMap<DeviceId, FakeDevice>>,
    }

    impl FakeOutput {
        fn with_device(device: DeviceId, volume: f32) -> Arc<Self> {
            let output = Self::default();
            output.connect(device, volume);
            output.set_default(device);
            Arc::new(output)
        }

        fn connect(&self, device: DeviceId, volume: f32) {
            self.devices.lock().unwrap().insert(
                device,
                FakeDevice {
                    uid: format!("uid-{device}"),
                    volume,
                    connected: true,
                },
            );
        }

        fn set_default(&self, device: DeviceId) {
            *self.default.lock().unwrap() = device;
        }

        fn volume(&self, device: DeviceId) -> f32 {
            self.devices.lock().unwrap()[&device].volume
        }

        fn set_connected(&self, device: DeviceId, connected: bool) {
            self.devices
                .lock()
                .unwrap()
                .get_mut(&device)
                .unwrap()
                .connected = connected;
        }
    }

    impl OutputVolume for Arc<FakeOutput> {
        fn default_output_device(&self) -> Result<DeviceId, String> {
            Ok(*self.default.lock().unwrap())
        }

        fn device_uid(&self, device: DeviceId) -> Option<String> {
            self.devices
                .lock()
                .unwrap()
                .get(&device)
                .map(|d| d.uid.clone())
        }

        fn find_device_by_uid(&self, uid: &str) -> Option<DeviceId> {
            self.devices
                .lock()
                .unwrap()
                .iter()
                .find(|(_, d)| d.connected && d.uid == uid)
                .map(|(id, _)| *id)
        }

        fn adjustable_volume(&self, device: DeviceId) -> Result<f32, String> {
            Ok(self.devices.lock().unwrap()[&device].volume)
        }

        fn set_volume(&self, device: DeviceId, volume: f32) -> Result<(), String> {
            let mut devices = self.devices.lock().unwrap();
            let device = devices.get_mut(&device).unwrap();
            if !device.connected {
                return Err("disconnected".to_string());
            }
            device.volume = volume;
            Ok(())
        }
    }

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    fn assert_volume(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "volume {actual} != {expected}"
        );
    }

    #[test]
    fn fades_down_while_recording_and_back_up_afterward() {
        let output = FakeOutput::with_device(1, 0.8);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 75));
        assert!(output.volume(1) < 0.8 && output.volume(1) > 0.8 * MUFFLED_VOLUME_RATIO);
        muffler.step(ms(start, 150));
        assert_volume(output.volume(1), 0.8 * MUFFLED_VOLUME_RATIO);
        assert!(!muffler.is_fading());

        muffler.handle(Command::Restore, ms(start, 1000));
        muffler.step(ms(start, 1300));
        assert_volume(output.volume(1), 0.8);
        assert!(muffler.session.is_none());
    }

    #[test]
    fn a_new_recording_during_the_fade_up_keeps_the_original_volume() {
        let output = FakeOutput::with_device(1, 0.6);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        muffler.handle(Command::Restore, ms(start, 500));
        muffler.step(ms(start, 600));

        muffler.handle(Command::Muffle, ms(start, 600));
        muffler.step(ms(start, 750));
        assert_volume(output.volume(1), 0.6 * MUFFLED_VOLUME_RATIO);

        muffler.handle(Command::Restore, ms(start, 900));
        muffler.step(ms(start, 1200));
        assert_volume(output.volume(1), 0.6);
    }

    #[test]
    fn repeated_commands_do_not_restart_fades() {
        let output = FakeOutput::with_device(1, 0.5);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        muffler.handle(Command::Muffle, ms(start, 200));
        muffler.handle(Command::Restore, ms(start, 300));
        muffler.handle(Command::Restore, ms(start, 550));
        muffler.step(ms(start, 600));
        assert_volume(output.volume(1), 0.5);
        assert!(muffler.session.is_none());
    }

    #[test]
    fn restore_now_skips_the_fade() {
        let output = FakeOutput::with_device(1, 0.5);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();
        let (done, finished) = mpsc::channel();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        muffler.handle(Command::Restore, ms(start, 200));
        muffler.handle(Command::RestoreNow(done), ms(start, 210));
        assert_volume(output.volume(1), 0.5);
        assert!(finished.try_recv().is_ok());
        assert!(muffler.session.is_none());
    }

    #[test]
    fn restores_the_muffled_device_after_the_default_output_moves() {
        let output = FakeOutput::with_device(1, 0.5);
        output.connect(2, 0.9);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        output.set_default(2);
        muffler.handle(Command::Restore, ms(start, 200));
        muffler.step(ms(start, 500));
        assert_volume(output.volume(1), 0.5);
        assert_volume(output.volume(2), 0.9);
    }

    #[test]
    fn a_new_recording_on_another_output_finishes_the_old_restore_first() {
        let output = FakeOutput::with_device(1, 0.5);
        output.connect(2, 0.9);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        muffler.handle(Command::Restore, ms(start, 200));
        muffler.step(ms(start, 250));
        output.set_default(2);
        muffler.handle(Command::Muffle, ms(start, 260));
        assert_volume(output.volume(1), 0.5);
        muffler.step(ms(start, 410));
        assert_volume(output.volume(2), 0.9 * MUFFLED_VOLUME_RATIO);
    }

    #[test]
    fn restores_a_device_unplugged_while_muffled_once_it_returns() {
        let output = FakeOutput::with_device(1, 0.5);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        output.set_connected(1, false);
        muffler.handle(Command::Restore, ms(start, 200));
        muffler.step(ms(start, 500));
        assert!(muffler.session.is_none());
        assert_volume(output.volume(1), 0.5 * MUFFLED_VOLUME_RATIO);

        output.set_connected(1, true);
        muffler.handle(Command::DevicesChanged, ms(start, 900));
        assert_volume(output.volume(1), 0.5);
        assert!(muffler.pending_restores.is_empty());
    }

    #[test]
    fn restores_a_device_unplugged_during_the_fade_up_once_it_returns() {
        let output = FakeOutput::with_device(1, 0.5);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        muffler.handle(Command::Restore, ms(start, 200));
        output.set_connected(1, false);
        muffler.step(ms(start, 250));
        assert!(muffler.session.is_none());

        output.set_connected(1, true);
        muffler.handle(Command::DevicesChanged, ms(start, 900));
        assert_volume(output.volume(1), 0.5);
    }

    #[test]
    fn restore_without_muffle_changes_nothing() {
        let output = FakeOutput::with_device(1, 0.5);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Restore, start);
        muffler.step(ms(start, 300));
        assert_volume(output.volume(1), 0.5);
        assert!(muffler.session.is_none());
    }

    #[test]
    fn the_worker_finishes_a_fade_without_further_commands() {
        let output = FakeOutput::with_device(1, 0.8);
        let commands = spawn(Arc::clone(&output));
        let muffled = 0.8 * MUFFLED_VOLUME_RATIO;

        commands.send(Command::Muffle).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while (output.volume(1) - muffled).abs() > 1e-6 && Instant::now() < deadline {
            std::thread::sleep(FADE_STEP);
        }
        assert_volume(output.volume(1), muffled);

        let (done, finished) = mpsc::channel();
        commands.send(Command::Restore).unwrap();
        commands.send(Command::RestoreNow(done)).unwrap();
        finished.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_volume(output.volume(1), 0.8);
    }
}
