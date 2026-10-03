//! Muffling makes other apps' audio sound muffled for the length of a
//! recording: a tap plays it through a low-pass filter, so music sounds like
//! it comes from the next room. Where the filter cannot run, the output's
//! volume is turned down instead. Outputs without a mute control are muted
//! here too, through a tap that turns their audio all the way down.
//!
//! A single worker thread owns every change. Commands only post a message, so
//! they return at once and never delay the start of a recording. A fade that
//! is still running is retargeted by the next command instead of racing it: a
//! recording that starts while the previous one is still fading back fades
//! from wherever it is, and still restores the original afterward.

use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// Share of the original volume that stays audible when muffling turns the
/// volume down instead of filtering.
const MUFFLED_VOLUME_RATIO: f32 = 0.35;
const FADE_DOWN: Duration = Duration::from_millis(150);
const FADE_UP: Duration = Duration::from_millis(300);
const FADE_STEP: Duration = Duration::from_millis(15);
const EXIT_RESTORE_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) type DeviceId = u32;

/// What a tap does to the audio it plays back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum TapEffect {
    /// The muffle filter.
    Muffle,
    /// Turns the audio down to silence.
    Silence,
}

/// The system output, as the worker changes it.
pub(super) trait Output {
    fn default_output_device(&self) -> Result<DeviceId, String>;
    fn device_uid(&self, device: DeviceId) -> Option<String>;
    fn find_device_by_uid(&self, uid: &str) -> Option<DeviceId>;
    /// The output's own volume from 0 to 1, as the macOS volume slider sees
    /// it, or an error when it has none the app can change.
    fn adjustable_volume(&self, device: DeviceId) -> Result<f32, String>;
    fn set_volume(&self, device: DeviceId, volume: f32) -> Result<(), String>;
    /// Starts tapping other apps' audio on the device, with no effect applied
    /// yet. Fails when the effect cannot work on this output.
    fn start_tap(&self, device: DeviceId, effect: TapEffect) -> Result<(), String>;
    /// How far the tap's effect is applied, from 0 to 1.
    fn set_tap(&self, device: DeviceId, amount: f32) -> Result<(), String>;
    fn stop_tap(&self, device: DeviceId);
}

enum Command {
    Muffle,
    Silence,
    Restore,
    /// Restores without a fade, then acknowledges.
    RestoreNow(Sender<()>),
    DevicesChanged,
}

static COMMANDS: OnceLock<Sender<Command>> = OnceLock::new();

fn send(command: Command) {
    let commands = COMMANDS.get_or_init(|| spawn(super::CoreAudioOutput::default()));
    let _ = commands.send(command);
}

pub(super) fn muffle() {
    send(Command::Muffle);
}

/// Mutes an output that has no mute control.
pub(super) fn silence() {
    send(Command::Silence);
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

fn spawn<O: Output + Send + 'static>(output: O) -> Sender<Command> {
    let (commands, received) = mpsc::channel();
    // If the thread cannot start, the receiver is dropped with it and
    // muffling quietly does nothing.
    if let Err(err) = std::thread::Builder::new()
        .name("media-muffle".to_string())
        .spawn(move || run(Muffler::new(output), received))
    {
        log::warn!(target: "media", "muffle_worker_spawn_failed error={err}");
    }
    commands
}

fn run<O: Output>(mut muffler: Muffler<O>, commands: Receiver<Command>) {
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
    fn value_at(&self, now: Instant) -> f32 {
        // Ends exactly on the target, so a restore puts back the original.
        if self.is_done(now) {
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

#[derive(Clone, Copy, PartialEq)]
enum Quieting {
    Muffle,
    /// Down to zero, at once, like a mute.
    Silence,
}

impl Quieting {
    fn name(self) -> &'static str {
        match self {
            Quieting::Muffle => "muffle",
            Quieting::Silence => "mute",
        }
    }

    fn fade_down(self) -> Duration {
        match self {
            Quieting::Muffle => FADE_DOWN,
            Quieting::Silence => Duration::ZERO,
        }
    }

    fn fade_up(self) -> Duration {
        match self {
            Quieting::Muffle => FADE_UP,
            Quieting::Silence => Duration::ZERO,
        }
    }
}

/// What a session changes.
#[derive(Clone, Copy, PartialEq)]
enum Control {
    /// The output's own volume. Values are volumes.
    Volume,
    /// A tap on other apps' audio. Values are how far its effect is applied.
    Tap(TapEffect),
}

struct Session {
    device: DeviceId,
    uid: Option<String>,
    quieting: Quieting,
    control: Control,
    /// Value to restore.
    original: f32,
    /// Value last set.
    current: f32,
    restoring: bool,
    fade: Option<Fade>,
}

impl Session {
    /// The value while quieted.
    fn target(&self) -> f32 {
        match (self.control, self.quieting) {
            (Control::Tap(_), _) => 1.0,
            (Control::Volume, Quieting::Muffle) => self.original * MUFFLED_VOLUME_RATIO,
            (Control::Volume, Quieting::Silence) => 0.0,
        }
    }

    fn fade_to(&mut self, to: f32, duration: Duration, now: Instant) {
        self.fade = Some(Fade {
            from: self.current,
            to,
            started: now,
            duration,
        });
    }
}

struct Muffler<O> {
    output: O,
    /// The quieted device, from the start of the fade down to the end of the
    /// fade back.
    session: Option<Session>,
    /// UIDs and original volumes of devices that were unplugged while turned
    /// down. macOS restores a device's saved volume when it reconnects, so
    /// they are turned back up once they are back.
    pending_restores: Vec<(String, f32)>,
}

impl<O: Output> Muffler<O> {
    fn new(output: O) -> Self {
        Self {
            output,
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
            Command::Muffle => self.quiet(Quieting::Muffle, now),
            Command::Silence => self.quiet(Quieting::Silence, now),
            Command::Restore => self.restore(false, now),
            Command::RestoreNow(done) => {
                self.restore(true, now);
                self.step(now);
                let _ = done.send(());
            }
            Command::DevicesChanged => {
                self.restore_returned_devices();
                self.follow_default_output(now);
            }
        }
    }

    fn quiet(&mut self, quieting: Quieting, now: Instant) {
        self.restore_returned_devices();
        let default_device = self.output.default_output_device();

        if let Some(session) = self.session.as_mut() {
            if !session.restoring {
                return;
            }
            // The previous recording is still fading back.
            if session.quieting == quieting
                && default_device
                    .as_ref()
                    .is_ok_and(|device| *device == session.device)
            {
                session.restoring = false;
                session.fade_to(session.target(), quieting.fade_down(), now);
                return;
            }
        }
        if self.session.is_some() {
            // The output or the setting changed; finish restoring first.
            self.restore(true, now);
            self.step(now);
        }

        let name = quieting.name();
        let device = match default_device {
            Ok(device) => device,
            Err(err) => {
                log::warn!(target: "media", "{name}_failed error={err}");
                return;
            }
        };
        let (control, original) = match self.choose_control(quieting, device) {
            Ok(choice) => choice,
            Err(err) => {
                log::warn!(target: "media", "{name}_unsupported device_id={device} error={err}");
                return;
            }
        };
        let mut session = Session {
            device,
            uid: self.output.device_uid(device),
            quieting,
            control,
            original,
            current: original,
            restoring: false,
            fade: None,
        };
        session.fade_to(session.target(), quieting.fade_down(), now);
        self.session = Some(session);
    }

    /// Muffles through the filter where it can and turns the volume down
    /// otherwise. Mutes with the volume where there is one, and with a tap
    /// otherwise.
    fn choose_control(
        &self,
        quieting: Quieting,
        device: DeviceId,
    ) -> Result<(Control, f32), String> {
        if quieting == Quieting::Muffle {
            match self.output.start_tap(device, TapEffect::Muffle) {
                Ok(()) => return Ok((Control::Tap(TapEffect::Muffle), 0.0)),
                Err(err) => log::info!(
                    target: "media",
                    "muffle_filter_unavailable device_id={device} error={err}"
                ),
            }
        }
        match self.output.adjustable_volume(device) {
            Ok(volume) => Ok((Control::Volume, volume)),
            Err(err) if quieting == Quieting::Muffle => Err(err),
            Err(_) => self
                .output
                .start_tap(device, TapEffect::Silence)
                .map(|()| (Control::Tap(TapEffect::Silence), 0.0)),
        }
    }

    fn restore(&mut self, immediately: bool, now: Instant) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.restoring && !immediately {
            return;
        }
        // A device unplugged and reconnected mid-recording can come back
        // under a new ID, so look it up by UID first.
        if let Some(uid) = &session.uid {
            let Some(device) = self.output.find_device_by_uid(uid) else {
                log::info!(
                    target: "media",
                    "muffled_output_disconnected device_id={}",
                    session.device
                );
                match session.control {
                    Control::Volume => self.pending_restores.push((uid.clone(), session.original)),
                    // Its audio no longer goes through the tap.
                    Control::Tap(_) => self.output.stop_tap(session.device),
                }
                self.session = None;
                return;
            };
            session.device = device;
        }
        session.restoring = true;
        let duration = if immediately {
            Duration::ZERO
        } else {
            session.quieting.fade_up()
        };
        session.fade_to(session.original, duration, now);
    }

    /// Moves a tap to the new default output when the output changes during a
    /// recording, such as when headphones are plugged in. Audio on the old
    /// output plays normally again once its tap stops.
    fn follow_default_output(&mut self, now: Instant) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Control::Tap(effect) = session.control else {
            return;
        };
        if session.restoring {
            return;
        }
        let Ok(device) = self.output.default_output_device() else {
            return;
        };
        if device == session.device {
            return;
        }
        self.output.stop_tap(session.device);
        if let Err(err) = self.output.start_tap(device, effect) {
            log::info!(
                target: "media",
                "{}_follow_failed device_id={device} error={err}",
                session.quieting.name()
            );
            self.session = None;
            return;
        }
        session.device = device;
        session.uid = self.output.device_uid(device);
        // The new tap starts with no effect.
        session.current = session.original;
        session.fade_to(session.target(), session.quieting.fade_down(), now);
    }

    fn step(&mut self, now: Instant) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(fade) = &session.fade else {
            return;
        };
        let value = fade.value_at(now);
        let done = fade.is_done(now);

        let result = match session.control {
            Control::Volume => self.output.set_volume(session.device, value),
            Control::Tap(_) => self.output.set_tap(session.device, value),
        };
        if let Err(err) = result {
            log::warn!(
                target: "media",
                "{}_volume_failed device_id={} error={err}",
                session.quieting.name(),
                session.device
            );
            session.fade = None;
            match session.control {
                // Most likely unplugged. A failed fade down is restored with
                // the recording; a failed fade up is retried when the device
                // is back.
                Control::Volume => {
                    if session.restoring {
                        if let Some(uid) = session.uid.take() {
                            self.pending_restores.push((uid, session.original));
                        }
                        self.session = None;
                    }
                }
                // Audio plays normally again once the tap stops.
                Control::Tap(_) => {
                    self.output.stop_tap(session.device);
                    self.session = None;
                }
            }
            return;
        }

        session.current = value;
        if done {
            session.fade = None;
            if session.restoring {
                if let Control::Tap(_) = session.control {
                    self.output.stop_tap(session.device);
                }
                self.session = None;
            }
        }
    }

    fn restore_returned_devices(&mut self) {
        let output = &self.output;
        self.pending_restores.retain(|(uid, original)| {
            let Some(device) = output.find_device_by_uid(uid) else {
                return true;
            };
            match output.set_volume(device, *original) {
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
        has_volume: bool,
        connected: bool,
        taps_work: bool,
        /// Effect and amount of the running tap.
        tap: Option<(TapEffect, f32)>,
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
                    has_volume: true,
                    connected: true,
                    taps_work: false,
                    tap: None,
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

        fn allow_taps(&self, device: DeviceId) {
            self.devices
                .lock()
                .unwrap()
                .get_mut(&device)
                .unwrap()
                .taps_work = true;
        }

        fn remove_volume_control(&self, device: DeviceId) {
            self.devices
                .lock()
                .unwrap()
                .get_mut(&device)
                .unwrap()
                .has_volume = false;
        }

        fn tap(&self, device: DeviceId) -> Option<(TapEffect, f32)> {
            self.devices.lock().unwrap()[&device].tap
        }
    }

    impl Output for Arc<FakeOutput> {
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
            let devices = self.devices.lock().unwrap();
            let device = &devices[&device];
            if !device.has_volume {
                return Err("fixed".to_string());
            }
            Ok(device.volume)
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

        fn start_tap(&self, device: DeviceId, effect: TapEffect) -> Result<(), String> {
            let mut devices = self.devices.lock().unwrap();
            let device = devices.get_mut(&device).unwrap();
            if !device.connected || !device.taps_work {
                return Err("no tap".to_string());
            }
            device.tap = Some((effect, 0.0));
            Ok(())
        }

        fn set_tap(&self, device: DeviceId, amount: f32) -> Result<(), String> {
            let mut devices = self.devices.lock().unwrap();
            let device = devices.get_mut(&device).unwrap();
            match (&mut device.tap, device.connected) {
                (Some((_, current)), true) => {
                    *current = amount;
                    Ok(())
                }
                _ => Err("no tap".to_string()),
            }
        }

        fn stop_tap(&self, device: DeviceId) {
            self.devices.lock().unwrap().get_mut(&device).unwrap().tap = None;
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
    fn silence_mutes_at_once_and_restores_the_exact_volume() {
        let output = FakeOutput::with_device(1, 0.6);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Silence, start);
        muffler.step(start);
        assert_eq!(output.volume(1), 0.0);
        assert!(!muffler.is_fading());

        muffler.handle(Command::Restore, ms(start, 1000));
        muffler.step(ms(start, 1000));
        assert_eq!(output.volume(1), 0.6);
        assert!(muffler.session.is_none());
    }

    #[test]
    fn a_fade_back_up_ends_on_the_exact_original_volume() {
        let output = FakeOutput::with_device(1, 0.7);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        muffler.handle(Command::Restore, ms(start, 200));
        muffler.step(ms(start, 350));
        muffler.step(ms(start, 517));
        assert_eq!(output.volume(1), 0.7);
    }

    #[test]
    fn a_silence_during_the_fade_up_mutes_and_still_restores() {
        let output = FakeOutput::with_device(1, 0.5);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        muffler.handle(Command::Restore, ms(start, 200));
        muffler.step(ms(start, 300));

        muffler.handle(Command::Silence, ms(start, 300));
        muffler.step(ms(start, 300));
        assert_eq!(output.volume(1), 0.0);

        muffler.handle(Command::Restore, ms(start, 900));
        muffler.step(ms(start, 900));
        assert_eq!(output.volume(1), 0.5);
        assert!(muffler.session.is_none());
    }

    #[test]
    fn muffles_through_the_filter_and_leaves_the_volume_alone() {
        let output = FakeOutput::with_device(1, 0.8);
        output.allow_taps(1);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 75));
        let (effect, amount) = output.tap(1).unwrap();
        assert_eq!(effect, TapEffect::Muffle);
        assert!(amount > 0.0 && amount < 1.0);
        muffler.step(ms(start, 150));
        assert_eq!(output.tap(1), Some((TapEffect::Muffle, 1.0)));

        muffler.handle(Command::Restore, ms(start, 1000));
        muffler.step(ms(start, 1150));
        assert!(output.tap(1).is_some_and(|(_, amount)| amount < 1.0));
        muffler.step(ms(start, 1300));
        assert_eq!(output.tap(1), None);
        assert!(muffler.session.is_none());
        assert_eq!(output.volume(1), 0.8);
    }

    #[test]
    fn muffle_turns_the_volume_down_where_the_filter_cannot_run() {
        let output = FakeOutput::with_device(1, 0.8);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        assert_eq!(output.tap(1), None);
        assert_volume(output.volume(1), 0.8 * MUFFLED_VOLUME_RATIO);
    }

    #[test]
    fn muffle_does_nothing_without_a_filter_or_a_volume() {
        let output = FakeOutput::with_device(1, 0.8);
        output.remove_volume_control(1);
        let mut muffler = Muffler::new(Arc::clone(&output));

        muffler.handle(Command::Muffle, Instant::now());
        assert!(muffler.session.is_none());
    }

    #[test]
    fn silence_taps_an_output_without_a_volume() {
        let output = FakeOutput::with_device(1, 0.8);
        output.remove_volume_control(1);
        output.allow_taps(1);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Silence, start);
        muffler.step(start);
        assert_eq!(output.tap(1), Some((TapEffect::Silence, 1.0)));

        muffler.handle(Command::Restore, ms(start, 500));
        muffler.step(ms(start, 500));
        assert_eq!(output.tap(1), None);
        assert!(muffler.session.is_none());
    }

    #[test]
    fn the_filter_follows_the_default_output() {
        let output = FakeOutput::with_device(1, 0.8);
        output.connect(2, 0.6);
        output.allow_taps(1);
        output.allow_taps(2);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        output.set_default(2);
        muffler.handle(Command::DevicesChanged, ms(start, 500));
        assert_eq!(output.tap(1), None);
        muffler.step(ms(start, 650));
        assert_eq!(output.tap(2), Some((TapEffect::Muffle, 1.0)));

        muffler.handle(Command::Restore, ms(start, 1000));
        muffler.step(ms(start, 1300));
        assert_eq!(output.tap(2), None);
        assert_eq!(output.volume(1), 0.8);
        assert_eq!(output.volume(2), 0.6);
    }

    #[test]
    fn a_filter_on_an_unplugged_output_is_dropped() {
        let output = FakeOutput::with_device(1, 0.8);
        output.allow_taps(1);
        let mut muffler = Muffler::new(Arc::clone(&output));
        let start = Instant::now();

        muffler.handle(Command::Muffle, start);
        muffler.step(ms(start, 150));
        output.set_connected(1, false);
        muffler.handle(Command::Restore, ms(start, 500));
        assert!(muffler.session.is_none());
        assert_eq!(output.tap(1), None);
        assert!(muffler.pending_restores.is_empty());
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
