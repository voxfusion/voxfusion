//! The sound server on Linux, through the PulseAudio protocol, which PipeWire
//! also speaks on most desktops: the microphones, recording from one, the
//! output's mute and volume, and changes to the devices.

use std::ffi::{CStr, CString};
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::time::Duration;

use pulseaudio::protocol::{self, Command, CommandReply};

const CLIENT_NAME: &CStr = c"VoxFusion";

/// How long a command may take before the server is taken to be gone.
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

/// How long device events are gathered into one notification. Plugging a
/// headset in adds a sink, a source and a new default at once.
const EVENT_COALESCING: Duration = Duration::from_millis(150);

/// How long to wait before connecting again after the server went away, such
/// as when PipeWire restarts.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Whether a sound server answers. Without one, recording goes through ALSA
/// and media cannot be muted.
pub fn is_available() -> bool {
    pulseaudio::socket_path_from_env().is_some()
}

/// A blocking connection for commands.
pub struct Connection {
    socket: BufReader<UnixStream>,
    version: u16,
    sequence: u32,
}

fn error(context: &str, err: impl std::fmt::Display) -> String {
    format!("{context}: {err}")
}

impl Connection {
    pub fn open() -> Result<Self, String> {
        let path = pulseaudio::socket_path_from_env().ok_or("No sound server is running")?;
        let socket = UnixStream::connect(&path).map_err(|err| error("Connecting", err))?;
        socket
            .set_read_timeout(Some(REPLY_TIMEOUT))
            .map_err(|err| error("Connecting", err))?;

        let cookie = pulseaudio::cookie_path_from_env()
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_default();
        let mut connection = Self {
            socket: BufReader::new(socket),
            version: protocol::MAX_VERSION,
            sequence: 0,
        };

        let auth: protocol::AuthReply = connection.reply(Command::Auth(protocol::AuthParams {
            version: protocol::MAX_VERSION,
            supports_shm: false,
            supports_memfd: false,
            cookie,
        }))?;
        connection.version = protocol::MAX_VERSION.min(auth.version);

        let mut props = protocol::Props::new();
        props.set(protocol::Prop::ApplicationName, CLIENT_NAME);
        let _: protocol::SetClientNameReply = connection.reply(Command::SetClientName(props))?;

        Ok(connection)
    }

    fn send(&mut self, command: &Command) -> Result<u32, String> {
        self.sequence = self.sequence.wrapping_add(1);
        protocol::write_command_message(
            self.socket.get_mut(),
            self.sequence,
            command,
            self.version,
        )
        .map_err(|err| error("Sending a command", err))?;
        Ok(self.sequence)
    }

    fn reply<R: CommandReply>(&mut self, command: Command) -> Result<R, String> {
        let sequence = self.send(&command)?;
        let (replied, reply) = protocol::read_reply_message::<R>(&mut self.socket, self.version)
            .map_err(|err| error("Reading a reply", err))?;
        if replied != sequence {
            return Err("The sound server replied out of turn".to_string());
        }
        Ok(reply)
    }

    fn ack(&mut self, command: Command) -> Result<(), String> {
        let sequence = self.send(&command)?;
        let replied = protocol::read_ack_message(&mut self.socket)
            .map_err(|err| error("Reading a reply", err))?;
        if replied != sequence {
            return Err("The sound server replied out of turn".to_string());
        }
        Ok(())
    }

    pub fn server_info(&mut self) -> Result<protocol::ServerInfo, String> {
        self.reply(Command::GetServerInfo)
    }

    pub fn sinks(&mut self) -> Result<Vec<protocol::SinkInfo>, String> {
        self.reply(Command::GetSinkInfoList)
    }

    pub fn sources(&mut self) -> Result<Vec<protocol::SourceInfo>, String> {
        self.reply(Command::GetSourceInfoList)
    }

    pub fn sink(&mut self, index: u32) -> Result<protocol::SinkInfo, String> {
        self.reply(Command::GetSinkInfo(protocol::GetSinkInfo {
            index: Some(index),
            name: None,
        }))
    }

    pub fn set_sink_mute(&mut self, index: u32, mute: bool) -> Result<(), String> {
        self.ack(Command::SetSinkMute(protocol::SetDeviceMuteParams {
            device_index: Some(index),
            device_name: None,
            mute,
        }))
    }

    pub fn set_sink_volume(
        &mut self,
        index: u32,
        volume: protocol::ChannelVolume,
    ) -> Result<(), String> {
        self.ack(Command::SetSinkVolume(protocol::SetDeviceVolumeParams {
            device_index: Some(index),
            device_name: None,
            volume,
        }))
    }
}

/// The connection that media changes go through, opened when first needed.
static CONTROL: Mutex<Option<Connection>> = Mutex::new(None);

/// Runs `command` on the shared connection. When it fails because the server
/// dropped the connection, as when PipeWire restarts, it connects again and
/// runs `command` once more.
pub fn with_connection<T>(
    mut command: impl FnMut(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let mut control = CONTROL.lock().map_err(|err| err.to_string())?;
    let connection = match control.as_mut() {
        Some(connection) => connection,
        None => control.insert(Connection::open()?),
    };
    let err = match command(connection) {
        Ok(value) => return Ok(value),
        Err(err) => err,
    };
    // A server that still answers turned the command down.
    if connection.server_info().is_ok() {
        return Err(err);
    }

    log::info!(target: "audio", "sound_server_reconnecting error={err}");
    *control = None;
    let connection = control.insert(Connection::open()?);
    command(connection)
}

/// A microphone: a source that is not the monitor of an output.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The server's name for it, which recording asks for.
    pub name: CString,
    /// What the interface lists and saves it as: what the system's sound
    /// settings call it, made unique by `distinct_labels`.
    pub label: String,
    /// What the system's sound settings call it.
    description: String,
    pub channels: u8,
    pub sample_rate: u32,
}

fn label(description: Option<&CStr>, name: &CStr) -> String {
    description.unwrap_or(name).to_string_lossy().into_owned()
}

/// The microphones, and the name of the default one.
pub fn sources() -> Result<(Vec<Source>, Option<CString>), String> {
    with_connection(|connection| {
        let default = connection.server_info()?.default_source_name;
        let sources = connection
            .sources()?
            .into_iter()
            .filter(|source| source.monitor_of_sink_index.is_none())
            .map(|source| Source {
                label: label(source.description.as_deref(), &source.name),
                description: label(source.description.as_deref(), &source.name),
                name: source.name,
                channels: source.sample_spec.channels,
                sample_rate: source.sample_spec.sample_rate,
            })
            .collect();
        Ok((distinct_labels(sources), default))
    })
}

/// Labels that tell every microphone apart, since the interface lists and
/// saves them by label. Microphones that share a description, such as two of
/// one USB model, are told apart by the server's name for each, which stays
/// the same when they are plugged in again.
fn distinct_labels(mut sources: Vec<Source>) -> Vec<Source> {
    let shared: Vec<String> = sources
        .iter()
        .map(|source| source.label.clone())
        .filter(|label| {
            sources
                .iter()
                .filter(|source| source.label == *label)
                .count()
                > 1
        })
        .collect();
    for source in &mut sources {
        if shared.contains(&source.label) {
            source.label = source.qualified_label();
        }
    }
    sources
}

impl Source {
    fn qualified_label(&self) -> String {
        format!("{} ({})", self.description, self.name.to_string_lossy())
    }

    /// Whether `saved` names this microphone. A label saved while another
    /// microphone shared its description still does once that one is gone.
    pub fn answers_to(&self, saved: &str) -> bool {
        self.label == saved || self.qualified_label() == saved
    }
}

/// A running recording. Dropping it ends the stream.
pub struct Recording {
    _stream: pulseaudio::RecordStream,
}

/// The client that recordings stream through, kept between recordings.
static RECORDING_CLIENT: Mutex<Option<pulseaudio::Client>> = Mutex::new(None);

/// The recording client, connected again if the server has dropped it.
fn recording_client() -> Result<pulseaudio::Client, String> {
    use futures::executor::block_on;

    let mut client = RECORDING_CLIENT.lock().map_err(|err| err.to_string())?;
    if let Some(connected) = client.as_ref()
        && block_on(connected.server_info()).is_ok()
    {
        return Ok(connected.clone());
    }
    let connected = pulseaudio::Client::from_env(CLIENT_NAME)
        .map_err(|err| error("Connecting to the sound server", err))?;
    *client = Some(connected.clone());
    Ok(connected)
}

/// Records from `source`, or from the default microphone, as mono 32-bit
/// float samples at `sample_rate`. `on_samples` runs on the client's thread.
///
/// A stream from a chosen microphone is not moved to another one when it is
/// unplugged: it stops, and the recorder notices the silence.
pub fn record(
    source: Option<&CStr>,
    sample_rate: u32,
    mut on_samples: impl FnMut(&[f32]) + Send + 'static,
) -> Result<Recording, String> {
    use futures::executor::block_on;

    let client = recording_client()?;

    // About 20 ms per callback, so the level meter moves smoothly.
    let fragment = sample_rate / 50 * 4;
    let mut props = protocol::Props::new();
    props.set(protocol::Prop::MediaName, c"Dictation");
    let params = protocol::RecordStreamParams {
        sample_spec: protocol::SampleSpec {
            format: protocol::SampleFormat::Float32Le,
            channels: 1,
            sample_rate,
        },
        channel_map: protocol::ChannelMap::new([protocol::ChannelPosition::Mono]),
        source_name: Some(source.unwrap_or(protocol::DEFAULT_SOURCE).to_owned()),
        buffer_attr: protocol::stream::BufferAttr {
            max_length: u32::MAX,
            fragment_size: fragment,
            ..Default::default()
        },
        flags: protocol::stream::StreamFlags {
            adjust_latency: true,
            no_move: source.is_some(),
            ..Default::default()
        },
        props,
        ..Default::default()
    };

    let mut samples = Vec::new();
    let stream = block_on(client.create_record_stream(params, move |bytes: &[u8]| {
        samples.clear();
        samples.extend(
            bytes
                .chunks_exact(4)
                .map(|sample| f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]])),
        );
        on_samples(&samples);
    }))
    .map_err(|err| error("Starting the recording", err))?;

    Ok(Recording { _stream: stream })
}

/// Calls `changed` from a thread of its own whenever an output or a
/// microphone is added or removed, or the default one changes.
pub fn watch_devices(changed: impl Fn() + Send + 'static) {
    let (events, received) = std::sync::mpsc::channel::<()>();

    let watching = std::thread::Builder::new()
        .name("audio-device-watch".into())
        .spawn(move || {
            let mut reconnected = false;
            loop {
                if let Err(err) = watch_until_disconnected(&events, reconnected) {
                    log::info!(target: "audio", "device_watch_interrupted error={err}");
                }
                reconnected = true;
                std::thread::sleep(RECONNECT_DELAY);
            }
        });
    if let Err(err) = watching {
        log::warn!(target: "audio", "device_watch_not_started error={err}");
        return;
    }

    let reporting = std::thread::Builder::new()
        .name("audio-device-report".into())
        .spawn(move || {
            while received.recv().is_ok() {
                while received.recv_timeout(EVENT_COALESCING).is_ok() {}
                changed();
            }
        });
    if let Err(err) = reporting {
        log::warn!(target: "audio", "device_watch_not_started error={err}");
    }
}

fn watch_until_disconnected(
    events: &std::sync::mpsc::Sender<()>,
    reconnected: bool,
) -> Result<(), String> {
    use protocol::{SubscriptionEventFacility as Facility, SubscriptionEventType as Type};

    let mut connection = Connection::open()?;
    connection.ack(Command::Subscribe(
        protocol::SubscriptionMask::SINK
            | protocol::SubscriptionMask::SOURCE
            | protocol::SubscriptionMask::SERVER,
    ))?;
    // Events come whenever something happens; only replies have a deadline.
    connection
        .socket
        .get_ref()
        .set_read_timeout(None)
        .map_err(|err| error("Watching devices", err))?;
    log::info!(target: "audio", "device_watch_started");

    // Devices may have changed while the server was away.
    if reconnected {
        let _ = events.send(());
    }

    loop {
        let (_, command) =
            protocol::read_command_message(&mut connection.socket, connection.version)
                .map_err(|err| error("Watching devices", err))?;
        let Command::SubscribeEvent(event) = command else {
            continue;
        };
        // A sink or source changes whenever its volume does; only added and
        // removed ones matter, and the server's defaults.
        let relevant = match event.event_facility {
            Facility::Sink | Facility::Source => event.event_type != Type::Changed,
            Facility::Server => true,
            _ => false,
        };
        if relevant && events.send(()).is_err() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, label: &str) -> Source {
        Source {
            name: CString::new(name).unwrap(),
            label: label.to_string(),
            description: label.to_string(),
            channels: 1,
            sample_rate: 48_000,
        }
    }

    #[test]
    fn microphones_sharing_a_description_get_their_names_added() {
        let labels: Vec<String> = distinct_labels(vec![
            source("alsa_input.usb-Mic_1", "USB Microphone"),
            source("alsa_input.pci-0000", "Built-in Audio"),
            source("alsa_input.usb-Mic_2", "USB Microphone"),
        ])
        .into_iter()
        .map(|source| source.label)
        .collect();

        assert_eq!(
            labels,
            [
                "USB Microphone (alsa_input.usb-Mic_1)",
                "Built-in Audio",
                "USB Microphone (alsa_input.usb-Mic_2)",
            ]
        );
    }

    #[test]
    fn a_microphone_saved_beside_its_twin_is_found_once_the_twin_is_gone() {
        let first = "USB Microphone (alsa_input.usb-Mic_1)";
        let pair = distinct_labels(vec![
            source("alsa_input.usb-Mic_1", "USB Microphone"),
            source("alsa_input.usb-Mic_2", "USB Microphone"),
        ]);
        assert!(pair[0].answers_to(first));
        assert!(!pair[1].answers_to(first));

        let alone = distinct_labels(vec![source("alsa_input.usb-Mic_1", "USB Microphone")]);
        assert_eq!(alone[0].label, "USB Microphone");
        assert!(alone[0].answers_to(first));
        assert!(alone[0].answers_to("USB Microphone"));
    }
}
