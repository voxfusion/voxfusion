//! Turns other apps' audio down on outputs that have no volume or mute
//! control, such as USB audio interfaces and displays (macOS 14.2+).
//!
//! A process tap on the output collects the audio every other process sends
//! to it and keeps that audio off the output while the tap is being read. A
//! private aggregate device reads the tap and plays it back on the same output
//! at a lower volume; at zero volume the output is muted.
//!
//! Reading a tap needs the System Audio Recording permission. Without it the
//! tap delivers silence but still holds the audio back, so the output is muted
//! rather than turned down.
//!
//! The tap and the aggregate device belong to this process: if the app quits
//! or crashes, the audio goes straight to the output again.

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use objc2::msg_send;
use objc2::rc::{Allocated, Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject, Bool};

use super::{
    AUDIO_OBJECT_SYSTEM_OBJECT, AUDIO_OBJECT_UNKNOWN, AudioObjectGetPropertyData, AudioObjectId,
    AudioObjectPropertySelector, OsStatus, core_audio_error, global_address, size_of_val_u32,
};

const AUDIO_HARDWARE_PROPERTY_TRANSLATE_PID_TO_PROCESS_OBJECT: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"id2p");

const AUDIO_TAP_PROPERTY_FORMAT: AudioObjectPropertySelector = u32::from_be_bytes(*b"tfmt");

const AUDIO_FORMAT_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");

const AUDIO_FORMAT_FLAG_IS_FLOAT: u32 = 1;

const AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED: u32 = 1 << 5;

/// `CATapMutedWhenTapped`: the tapped audio is held back from the output only
/// while the tap is read, so it comes back as soon as playback stops.
const TAP_MUTED_WHEN_TAPPED: isize = 2;

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
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

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
fn create_tap(api: &TapApi, device_uid: &str) -> Result<Tap, String> {
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
            let _: () = msg_send![&*description, setMuteBehavior: TAP_MUTED_WHEN_TAPPED];
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

/// Shared with the IO thread.
struct Playback {
    /// Gain the IO thread moves toward.
    target: AtomicU32,
    /// Gain of the last sample played. Only the IO thread changes it.
    current: AtomicU32,
    /// Share of the remaining distance to the target covered per sample.
    smoothing: f32,
}

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

    let target = f32::from_bits(playback.target.load(Ordering::Relaxed));
    let mut gain = f32::from_bits(playback.current.load(Ordering::Relaxed));
    for (tap_frame, output_frame) in tap_samples
        .chunks_exact(tap_channels)
        .zip(output_samples.chunks_exact_mut(output_channels))
    {
        gain += (target - gain) * playback.smoothing;
        for (sample, tapped) in output_frame.iter_mut().zip(tap_frame) {
            *sample = tapped * gain;
        }
    }
    playback.current.store(gain.to_bits(), Ordering::Relaxed);
    0
}

/// Plays other apps' audio on an output at a volume from 0 to 1, for as long
/// as it exists.
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
    /// Blocks until playback runs. The first time, that includes the System
    /// Audio Recording permission prompt.
    pub(super) fn start(device_uid: &str, volume: f32) -> Result<Self, String> {
        let api = tap_api().ok_or("Output taps need macOS 14.2 or later")?;
        let tap = create_tap(api, device_uid)?;
        // From here on, Drop releases whatever was created.
        let mut output_tap = OutputTap {
            api,
            tap: tap.id,
            aggregate_device: AUDIO_OBJECT_UNKNOWN,
            io_proc: std::ptr::null_mut(),
            playback: Box::new(Playback {
                // Playback starts at full volume, where the output already is.
                target: AtomicU32::new(1.0f32.to_bits()),
                current: AtomicU32::new(1.0f32.to_bits()),
                smoothing: 0.0,
            }),
        };

        let format = tap_format(tap.id)?;
        if format.format_id != AUDIO_FORMAT_LINEAR_PCM
            || format.format_flags & AUDIO_FORMAT_FLAG_IS_FLOAT == 0
            || format.format_flags & AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED != 0
            || format.bits_per_channel != 32
        {
            return Err("The output tap delivers an unsupported format".to_string());
        }
        output_tap.playback.smoothing =
            (1.0 - (-1.0 / (GAIN_SMOOTHING_SECONDS * format.sample_rate.max(1.0))).exp()) as f32;
        output_tap.set_volume(volume);

        output_tap.aggregate_device = create_aggregate_device(device_uid, &tap.uid)?;
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
    /// in amplitude. Muffling then turns an output down about as far as it
    /// does one with a volume control.
    pub(super) fn set_volume(&self, volume: f32) {
        let gain = volume.clamp(0.0, 1.0).powi(3);
        self.playback
            .target
            .store(gain.to_bits(), Ordering::Relaxed);
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
