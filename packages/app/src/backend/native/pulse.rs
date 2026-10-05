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

        let auth: protocol::AuthReply =
            connection.reply(Command::Auth(protocol::AuthParams {
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
        protocol::write_command_message(self.socket.get_mut(), self.sequence, command, self.version)
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
        let replied =
            protocol::read_ack_message(&mut self.socket).map_err(|err| error("Reading a reply", err))?;
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

/// Runs `command` on the shared connection, connecting again once if the
/// server dropped it.
pub fn with_connection<T>(
    mut command: impl FnMut(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let mut control = CONTROL.lock().map_err(|err| err.to_string())?;
    for attempt in 0..2 {
        if control.is_none() {
            *control = Some(Connection::open()?);
        }
        let connection = control.as_mut().expect("connected above");
        match command(connection) {
            Ok(value) => return Ok(value),
            Err(err) if attempt == 0 => {
                log::info!(target: "audio", "sound_server_reconnecting error={err}");
                *control = None;
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("the second attempt returns")
}

/// A microphone: a source that is not the monitor of an output.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The server's name for it, which recording asks for.
    pub name: CString,
    /// What the system's sound settings call it.
    pub label: String,
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
                name: source.name,
                channels: source.sample_spec.channels,
                sample_rate: source.sample_spec.sample_rate,
            })
            .collect();
        Ok((sources, default))
    })
}

/// A running recording. Dropping it ends the stream.
pub struct Recording {
    _client: pulseaudio::Client,
    _stream: pulseaudio::RecordStream,
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

    let client = pulseaudio::Client::from_env(CLIENT_NAME)
        .map_err(|err| error("Connecting to the sound server", err))?;

    // About 20 ms per callback, so the level meter moves smoothly.
    let fragment = sample_rate / 50 * 4;
    let mut props = protocol::Props::new();
    props.set(protocol::Prop::MediaName, c"Dictation");
    props.set(protocol::Prop::MediaRole, c"phone");
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

    Ok(Recording {
        _client: client,
        _stream: stream,
    })
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
        let (_, command) = protocol::read_command_message(&mut connection.socket, connection.version)
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
