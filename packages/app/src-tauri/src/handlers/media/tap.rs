//! Changes other apps' audio on an output (macOS 14.2+): muffles it with a
//! filter, or mutes outputs that have no mute control, such as USB audio
//! interfaces and displays.
//!
//! A process tap on the output collects the audio every other process sends
//! to it and keeps that audio off the output while the tap is being read. A
//! private aggregate device reads the tap and plays it back on the same output
//! through the muffle filter and at a set volume; at zero volume the output is
//! muted.
//!
//! Reading a tap needs the System Audio Recording permission. Without it the
//! tap delivers silence but still holds the audio back, so the output is muted
//! rather than turned down.
//!
//! The tap and the aggregate device belong to this process: if the app quits
//! or crashes, the audio goes straight to the output again.

use std::cell::UnsafeCell;
use std::ffi::{c_char, c_void};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use objc2::msg_send;
use objc2::rc::{Allocated, Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject, Bool};

use super::filter::MuffleFilter;
use super::{
    AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT, AUDIO_DEVICE_PROPERTY_STREAMS,
    AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN, AUDIO_OBJECT_SYSTEM_OBJECT, AUDIO_OBJECT_UNKNOWN,
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectId,
    AudioObjectPropertyAddress, AudioObjectPropertySelector, OsStatus, core_audio_error,
    global_address, size_of_val_u32,
};

const AUDIO_HARDWARE_PROPERTY_TRANSLATE_PID_TO_PROCESS_OBJECT: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"id2p");

const AUDIO_TAP_PROPERTY_FORMAT: AudioObjectPropertySelector = u32::from_be_bytes(*b"tfmt");

const AUDIO_FORMAT_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");

const AUDIO_FORMAT_FLAG_IS_FLOAT: u32 = 1;

const AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED: u32 = 1 << 5;

/// `CATapUnmuted`: the tapped audio still reaches the output.
const TAP_UNMUTED: isize = 0;

/// `CATapMutedWhenTapped`: the tapped audio is held back from the output only
/// while the tap is read, so it comes back as soon as playback stops.
const TAP_MUTED_WHEN_TAPPED: isize = 2;

/// How long a permission request waits for an answer to the prompt.
const PERMISSION_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const PERMISSION_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How quickly the playback gain follows a new target, so gain changes do not
/// click.
const GAIN_SMOOTHING_SECONDS: f64 = 0.01;

const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

#[repr(C)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

#[repr(C)]
#[derive(Default)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

type AudioDeviceIoProc = unsafe extern "C" fn(
    device: AudioObjectId,
    now: *const c_void,
    input_data: *const AudioBufferList,
    input_time: *const c_void,
    output_data: *mut AudioBufferList,
    output_time: *const c_void,
    client_data: *mut c_void,
) -> OsStatus;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioHardwareCreateAggregateDevice(
        in_description: CFDictionaryRef,
        out_device_id: *mut AudioObjectId,
    ) -> OsStatus;

    fn AudioHardwareDestroyAggregateDevice(in_device_id: AudioObjectId) -> OsStatus;

    fn AudioDeviceCreateIOProcID(
        in_device: AudioObjectId,
        in_proc: AudioDeviceIoProc,
        in_client_data: *mut c_void,
        out_io_proc_id: *mut *mut c_void,
    ) -> OsStatus;

    fn AudioDeviceDestroyIOProcID(in_device: AudioObjectId, in_io_proc_id: *mut c_void)
    -> OsStatus;

    fn AudioDeviceStart(in_device: AudioObjectId, in_io_proc_id: *mut c_void) -> OsStatus;

    fn AudioDeviceStop(in_device: AudioObjectId, in_io_proc_id: *mut c_void) -> OsStatus;
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_LAZY: i32 = 1;

type CreateProcessTap = unsafe extern "C" fn(
    in_description: *mut AnyObject,
    out_tap_id: *mut AudioObjectId,
) -> OsStatus;

type DestroyProcessTap = unsafe extern "C" fn(in_tap_id: AudioObjectId) -> OsStatus;

struct TapApi {
    create: CreateProcessTap,
    destroy: DestroyProcessTap,
    description_class: &'static AnyClass,
}

/// The process tap API. Looked up at runtime because the app also runs on
/// macOS versions that predate it.
fn tap_api() -> Option<&'static TapApi> {
    static API: OnceLock<Option<TapApi>> = OnceLock::new();
    API.get_or_init(|| {
        let description_class = AnyClass::get(c"CATapDescription")?;
        let create = unsafe { dlsym(RTLD_DEFAULT, c"AudioHardwareCreateProcessTap".as_ptr()) };
        let destroy = unsafe { dlsym(RTLD_DEFAULT, c"AudioHardwareDestroyProcessTap".as_ptr()) };
        if create.is_null() || destroy.is_null() {
            return None;
        }
        Some(TapApi {
            create: unsafe { std::mem::transmute::<*mut c_void, CreateProcessTap>(create) },
            destroy: unsafe { std::mem::transmute::<*mut c_void, DestroyProcessTap>(destroy) },
            description_class,
        })
    })
    .as_ref()
}

pub(super) fn is_supported() -> bool {
    tap_api().is_some()
}

/// The System Audio Recording permission, which reading a tap needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Permission {
    Granted,
    Denied,
    /// Not asked yet.
    Undetermined,
    /// The private API to check it is gone.
    Unknown,
}

type AccessPreflight = unsafe extern "C" fn(
    service: core_foundation::string::CFStringRef,
    options: CFDictionaryRef,
) -> i32;

/// Asks TCC without prompting. There is no public API for this permission.
pub(super) fn audio_capture_permission() -> Permission {
    static PREFLIGHT: OnceLock<Option<AccessPreflight>> = OnceLock::new();
    let preflight = PREFLIGHT.get_or_init(|| {
        let framework = unsafe {
            dlopen(
                c"/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC".as_ptr(),
                RTLD_LAZY,
            )
        };
        if framework.is_null() {
            return None;
        }
        let symbol = unsafe { dlsym(framework, c"TCCAccessPreflight".as_ptr()) };
        (!symbol.is_null())
            .then(|| unsafe { std::mem::transmute::<*mut c_void, AccessPreflight>(symbol) })
    });
    let Some(preflight) = preflight else {
        return Permission::Unknown;
    };
    let service = CFString::from_static_string("kTCCServiceAudioCapture");
    match unsafe { preflight(service.as_concrete_TypeRef(), std::ptr::null()) } {
        0 => Permission::Granted,
        1 => Permission::Denied,
        2 => Permission::Undetermined,
        _ => Permission::Unknown,
    }
}

/// Command-line flag that starts the app only to ask for the System Audio
/// Recording permission.
pub(super) const REQUEST_PERMISSION_FLAG: &str = "--request-audio-capture-permission";

/// Shows the System Audio Recording prompt without changing any audio. macOS
/// only asks once a tap has been read for a moment. Reading one in the app
/// process does not bring the prompt up, while reading it in a separate
/// process does, so the app runs itself again just for that; the permission
/// still belongs to the app.
pub(super) fn request_permission(device_uid: &str) {
    static REQUESTING: AtomicBool = AtomicBool::new(false);
    if REQUESTING.swap(true, Ordering::AcqRel) {
        return;
    }
    let child = std::env::current_exe().and_then(|executable| {
        std::process::Command::new(executable)
            .arg(REQUEST_PERMISSION_FLAG)
            .arg(device_uid)
            .spawn()
    });
    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            log::warn!(target: "media", "audio_capture_permission_request_failed error={err}");
            REQUESTING.store(false, Ordering::Release);
            return;
        }
    };
    std::thread::spawn(move || {
        let _ = child.wait();
        log::info!(
            target: "media",
            "audio_capture_permission_requested result={:?}",
            audio_capture_permission()
        );
        REQUESTING.store(false, Ordering::Release);
    });
}

/// Runs in the process started with [`REQUEST_PERMISSION_FLAG`]: reads an
/// unmuted tap on the output until the prompt is answered.
pub(super) fn hold_permission_request(device_uid: &str) {
    let Ok(probe) = OutputTap::open(device_uid, TAP_UNMUTED, false, 0.0) else {
        return;
    };
    let deadline = Instant::now() + PERMISSION_REQUEST_TIMEOUT;
    while audio_capture_permission() == Permission::Undetermined && Instant::now() < deadline {
        std::thread::sleep(PERMISSION_POLL_INTERVAL);
    }
    drop(probe);
}

fn own_process_object() -> Result<AudioObjectId, String> {
    let address = global_address(AUDIO_HARDWARE_PROPERTY_TRANSLATE_PID_TO_PROCESS_OBJECT);
    let pid = std::process::id() as i32;
    let mut process: AudioObjectId = AUDIO_OBJECT_UNKNOWN;
    let mut data_size = size_of_val_u32(&process)?;

    let status = unsafe {
        AudioObjectGetPropertyData(
            AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            size_of_val_u32(&pid)?,
            (&pid as *const i32).cast(),
            &mut data_size,
            (&mut process as *mut AudioObjectId).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Finding the app's audio process", status));
    }
    if process == AUDIO_OBJECT_UNKNOWN {
        return Err("The app has no audio process".to_string());
    }
    Ok(process)
}

/// Passes a Core Foundation object where Objective-C expects its toll-free
/// bridged counterpart.
fn as_object<T: TCFType>(value: &T) -> &AnyObject {
    unsafe { &*value.as_CFTypeRef().cast::<AnyObject>() }
}

struct Tap {
    id: AudioObjectId,
    uid: String,
}

/// Taps the audio every other process sends to the first stream of the output.
/// The app's own process is left out, because it plays the tapped audio back.
fn create_tap(api: &TapApi, device_uid: &str, mute_behavior: isize) -> Result<Tap, String> {
    let own_process = own_process_object()?;
    let excluded = CFArray::from_CFTypes(&[CFNumber::from(i64::from(own_process))]);
    let device_uid = CFString::new(device_uid);

    autoreleasepool(|_| {
        let allocated: Allocated<AnyObject> = unsafe { msg_send![api.description_class, alloc] };
        let description: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                allocated,
                initExcludingProcesses: as_object(&excluded),
                andDeviceUID: as_object(&device_uid),
                withStream: 0isize
            ]
        };
        let description = description.ok_or("Describing the output tap failed")?;
        let uid: Retained<AnyObject> = unsafe {
            let _: () = msg_send![&*description, setPrivate: Bool::YES];
            let _: () = msg_send![&*description, setMuteBehavior: mute_behavior];
            let uuid: Retained<AnyObject> = msg_send![&*description, UUID];
            msg_send![&*uuid, UUIDString]
        };
        let uid = unsafe { CFString::wrap_under_get_rule(Retained::as_ptr(&uid).cast()) };

        let mut id = AUDIO_OBJECT_UNKNOWN;
        let status = unsafe { (api.create)(Retained::as_ptr(&description).cast_mut(), &mut id) };
        if status != 0 {
            return Err(core_audio_error("Creating the output tap", status));
        }
        Ok(Tap {
            id,
            uid: uid.to_string(),
        })
    })
}

fn tap_format(tap: AudioObjectId) -> Result<AudioStreamBasicDescription, String> {
    let address = global_address(AUDIO_TAP_PROPERTY_FORMAT);
    let mut format = AudioStreamBasicDescription::default();
    let mut data_size = size_of_val_u32(&format)?;

    let status = unsafe {
        AudioObjectGetPropertyData(
            tap,
            &address,
            0,
            std::ptr::null(),
            &mut data_size,
            (&mut format as *mut AudioStreamBasicDescription).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Getting the output tap format", status));
    }
    Ok(format)
}

/// A private device that reads the tap and plays it on the output it taps.
fn create_aggregate_device(device_uid: &str, tap_uid: &str) -> Result<AudioObjectId, String> {
    let key = CFString::from_static_string;
    let device_uid = CFString::new(device_uid).as_CFType();
    let sub_device = CFDictionary::from_CFType_pairs(&[(key("uid"), device_uid.clone())]);
    let sub_tap = CFDictionary::from_CFType_pairs(&[
        (key("uid"), CFString::new(tap_uid).as_CFType()),
        (key("drift"), CFBoolean::true_value().as_CFType()),
    ]);
    let aggregate_uid = format!("io.voxfusion.media-tap.{}", uuid::Uuid::new_v4());
    let description: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[
        (
            key("name"),
            CFString::from_static_string("VoxFusion").as_CFType(),
        ),
        (key("uid"), CFString::new(&aggregate_uid).as_CFType()),
        (key("master"), device_uid),
        (key("private"), CFBoolean::true_value().as_CFType()),
        (key("stacked"), CFBoolean::false_value().as_CFType()),
        (key("tapautostart"), CFBoolean::true_value().as_CFType()),
        (
            key("subdevices"),
            CFArray::from_CFTypes(&[sub_device]).as_CFType(),
        ),
        (key("taps"), CFArray::from_CFTypes(&[sub_tap]).as_CFType()),
    ]);

    let mut device = AUDIO_OBJECT_UNKNOWN;
    let status = unsafe {
        AudioHardwareCreateAggregateDevice(description.as_concrete_TypeRef(), &mut device)
    };
    if status != 0 {
        return Err(core_audio_error("Creating the output tap device", status));
    }
    Ok(device)
}

/// An output made of other devices, such as an Aggregate or Multi-Output
/// Device, cannot be nested in the playback device, which then has no output
/// to play the tap on.
fn has_output_stream(device: AudioObjectId) -> Result<bool, String> {
    let address = AudioObjectPropertyAddress {
        selector: AUDIO_DEVICE_PROPERTY_STREAMS,
        scope: AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut data_size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(device, &address, 0, std::ptr::null(), &mut data_size)
    };
    if status != 0 {
        return Err(core_audio_error(
            "Getting output tap device streams",
            status,
        ));
    }
    Ok(data_size > 0)
}

const NO_PLAYBACK: &str = "The output tap device has no output to play on";

/// Shared with the IO thread.
struct Playback {
    /// Whether to play the tapped audio at all.
    play: bool,
    /// Gain the IO thread moves toward.
    gain: AtomicU32,
    /// Muffle filter amount the IO thread moves toward, from 0 to 1.
    muffle: AtomicU32,
    /// Share of the remaining distance to the target gain covered per sample.
    gain_smoothing: f32,
    io: UnsafeCell<IoState>,
}

/// Only the IO thread uses it, one callback at a time.
struct IoState {
    /// Gain of the last sample played.
    gain: f32,
    filter: MuffleFilter,
}

// The atomics are shared; `io` belongs to the IO thread.
unsafe impl Sync for Playback {}

/// Plays the tap, the last input stream after the output device's own, on the
/// output's first stream, and silence on the others.
unsafe extern "C" fn play_tap(
    _device: AudioObjectId,
    _now: *const c_void,
    input_data: *const AudioBufferList,
    _input_time: *const c_void,
    output_data: *mut AudioBufferList,
    _output_time: *const c_void,
    client_data: *mut c_void,
) -> OsStatus {
    let playback = unsafe { &*client_data.cast::<Playback>() };
    let (Some(input_data), Some(output_data)) = (unsafe { input_data.as_ref() }, unsafe {
        output_data.as_mut()
    }) else {
        return 0;
    };
    let inputs = unsafe {
        std::slice::from_raw_parts(
            input_data.buffers.as_ptr(),
            input_data.number_buffers as usize,
        )
    };
    let outputs = unsafe {
        std::slice::from_raw_parts_mut(
            output_data.buffers.as_mut_ptr(),
            output_data.number_buffers as usize,
        )
    };
    for output in outputs.iter_mut().filter(|output| !output.data.is_null()) {
        unsafe {
            std::ptr::write_bytes(output.data.cast::<u8>(), 0, output.data_byte_size as usize)
        };
    }
    if !playback.play {
        return 0;
    }
    let (Some(tap), Some(output)) = (inputs.last(), outputs.first_mut()) else {
        return 0;
    };
    if tap.data.is_null() || output.data.is_null() {
        return 0;
    }

    let tap_channels = tap.number_channels.max(1) as usize;
    let output_channels = output.number_channels.max(1) as usize;
    let frames = (tap.data_byte_size as usize / 4 / tap_channels)
        .min(output.data_byte_size as usize / 4 / output_channels);
    let tap_samples =
        unsafe { std::slice::from_raw_parts(tap.data.cast::<f32>(), frames * tap_channels) };
    let output_samples = unsafe {
        std::slice::from_raw_parts_mut(output.data.cast::<f32>(), frames * output_channels)
    };

    let io = unsafe { &mut *playback.io.get() };
    let target_gain = f32::from_bits(playback.gain.load(Ordering::Relaxed));
    for (tap_frame, output_frame) in tap_samples
        .chunks_exact(tap_channels)
        .zip(output_samples.chunks_exact_mut(output_channels))
    {
        io.gain += (target_gain - io.gain) * playback.gain_smoothing;
        for (sample, tapped) in output_frame.iter_mut().zip(tap_frame) {
            *sample = tapped * io.gain;
        }
    }
    let muffle = f32::from_bits(playback.muffle.load(Ordering::Relaxed));
    io.filter.process(output_samples, output_channels, muffle);
    0
}

/// Plays other apps' audio on an output, muffled and at a volume as set, for
/// as long as it exists.
pub(super) struct OutputTap {
    api: &'static TapApi,
    tap: AudioObjectId,
    aggregate_device: AudioObjectId,
    io_proc: *mut c_void,
    playback: Box<Playback>,
}

// The CoreAudio objects may be used and released from any thread.
unsafe impl Send for OutputTap {}

impl OutputTap {
    /// Holds other apps' audio back and plays it unmuffled at `volume`.
    pub(super) fn start(device_uid: &str, volume: f32) -> Result<Self, String> {
        Self::open(device_uid, TAP_MUTED_WHEN_TAPPED, true, volume)
    }

    fn open(
        device_uid: &str,
        mute_behavior: isize,
        play: bool,
        volume: f32,
    ) -> Result<Self, String> {
        let api = tap_api().ok_or("Output taps need macOS 14.2 or later")?;
        let tap = create_tap(api, device_uid, mute_behavior)?;
        let format = match tap_format(tap.id) {
            Ok(format) => format,
            Err(err) => {
                unsafe { (api.destroy)(tap.id) };
                return Err(err);
            }
        };
        let sample_rate = format.sample_rate.max(1.0);
        // From here on, Drop releases whatever was created.
        let mut output_tap = OutputTap {
            api,
            tap: tap.id,
            aggregate_device: AUDIO_OBJECT_UNKNOWN,
            io_proc: std::ptr::null_mut(),
            playback: Box::new(Playback {
                play,
                // Playback starts at full volume, where the output already is.
                gain: AtomicU32::new(1.0f32.to_bits()),
                muffle: AtomicU32::new(0.0f32.to_bits()),
                gain_smoothing: (1.0 - (-1.0 / (GAIN_SMOOTHING_SECONDS * sample_rate)).exp())
                    as f32,
                io: UnsafeCell::new(IoState {
                    gain: 1.0,
                    filter: MuffleFilter::new(
                        sample_rate as f32,
                        format.channels_per_frame as usize,
                    ),
                }),
            }),
        };

        if format.format_id != AUDIO_FORMAT_LINEAR_PCM
            || format.format_flags & AUDIO_FORMAT_FLAG_IS_FLOAT == 0
            || format.format_flags & AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED != 0
            || format.bits_per_channel != 32
        {
            return Err("The output tap delivers an unsupported format".to_string());
        }

        output_tap.aggregate_device = create_aggregate_device(device_uid, &tap.uid)?;
        // Without playback the tapped audio is only held back, which mutes
        // but cannot play anything.
        if play && volume > 0.0 && !has_output_stream(output_tap.aggregate_device)? {
            return Err(NO_PLAYBACK.to_string());
        }
        output_tap.set_volume(volume);
        let playback: *const Playback = &*output_tap.playback;
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                output_tap.aggregate_device,
                play_tap,
                playback.cast_mut().cast(),
                &mut output_tap.io_proc,
            )
        };
        if status != 0 {
            return Err(core_audio_error("Creating output tap playback", status));
        }
        let status = unsafe { AudioDeviceStart(output_tap.aggregate_device, output_tap.io_proc) };
        if status != 0 {
            return Err(core_audio_error("Starting output tap playback", status));
        }
        Ok(output_tap)
    }

    /// Volume on the scale of the macOS volume slider, which is close to cubic
    /// in amplitude.
    pub(super) fn set_volume(&self, volume: f32) {
        let gain = volume.clamp(0.0, 1.0).powi(3);
        self.playback.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    /// How far the muffle filter is applied, from 0 (not at all) to 1.
    pub(super) fn set_muffle(&self, amount: f32) {
        let amount = amount.clamp(0.0, 1.0);
        self.playback
            .muffle
            .store(amount.to_bits(), Ordering::Relaxed);
    }
}

impl Drop for OutputTap {
    fn drop(&mut self) {
        unsafe {
            if !self.io_proc.is_null() {
                // Stopping from outside the IO thread waits for the IO cycle
                // in progress, so the playback state outlives every callback.
                AudioDeviceStop(self.aggregate_device, self.io_proc);
                AudioDeviceDestroyIOProcID(self.aggregate_device, self.io_proc);
            }
            if self.aggregate_device != AUDIO_OBJECT_UNKNOWN {
                AudioHardwareDestroyAggregateDevice(self.aggregate_device);
            }
            (self.api.destroy)(self.tap);
        }
    }
}
