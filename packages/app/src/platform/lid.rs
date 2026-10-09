//! Whether the lid of a Mac laptop is closed. Closing it disconnects the
//! built-in microphone in hardware (Macs with a T2 chip or Apple silicon):
//! macOS still lists the microphone, and keeps it as the default input, but
//! it records silence. The lid state is the root power domain's
//! `AppleClamshellState`, which only Macs with a lid have.

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFAllocatorRef, CFTypeRef, kCFAllocatorDefault};
use core_foundation_sys::dictionary::CFMutableDictionaryRef;
use core_foundation_sys::string::CFStringRef;
use dispatch2::{DispatchQueue, DispatchRetained};

type IoObject = u32;
type KernReturn = i32;
type IoNotificationPortRef = *mut c_void;
type IoServiceInterestCallback = unsafe extern "C" fn(
    refcon: *mut c_void,
    service: IoObject,
    message_type: u32,
    message_argument: *mut c_void,
);

const IO_MAIN_PORT_DEFAULT: u32 = 0;

/// `kIOPMMessageClamshellStateChange`, which the root power domain sends
/// whenever the lid opens or closes.
const CLAMSHELL_STATE_CHANGE: u32 = 0xE003_4100;

/// `kClamshellStateBit` in that message's argument: set when the lid is
/// closed.
const CLAMSHELL_CLOSED_BIT: usize = 1;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;

    /// Takes over the reference to `matching`.
    fn IOServiceGetMatchingService(main_port: u32, matching: CFMutableDictionaryRef) -> IoObject;

    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: CFStringRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> CFTypeRef;

    fn IONotificationPortCreate(main_port: u32) -> IoNotificationPortRef;

    fn IONotificationPortSetDispatchQueue(notify: IoNotificationPortRef, queue: *mut c_void);

    fn IOServiceAddInterestNotification(
        notify_port: IoNotificationPortRef,
        service: IoObject,
        interest_type: *const c_char,
        callback: IoServiceInterestCallback,
        ref_con: *mut c_void,
        notification: *mut IoObject,
    ) -> KernReturn;
}

static ON_CHANGE: OnceLock<fn(bool)> = OnceLock::new();

/// The root power domain, which holds the lid state. Kept for the life of
/// the app.
fn root_domain() -> Option<IoObject> {
    static ROOT_DOMAIN: OnceLock<IoObject> = OnceLock::new();

    let root = *ROOT_DOMAIN.get_or_init(|| unsafe {
        IOServiceGetMatchingService(
            IO_MAIN_PORT_DEFAULT,
            IOServiceMatching(c"IOPMrootDomain".as_ptr()),
        )
    });
    (root != 0).then_some(root)
}

/// Whether the lid is closed, or `None` on a Mac without a lid.
pub fn closed() -> Option<bool> {
    let root = root_domain()?;
    let key = CFString::from_static_string("AppleClamshellState");
    let value = unsafe {
        IORegistryEntryCreateCFProperty(root, key.as_concrete_TypeRef(), kCFAllocatorDefault, 0)
    };
    if value.is_null() {
        return None;
    }

    let value = unsafe { CFType::wrap_under_create_rule(value) };
    value.downcast::<CFBoolean>().map(bool::from)
}

/// Calls `on_change` with whether the lid is now closed each time it opens
/// or closes, on a queue of its own. Only the first call watches.
pub fn watch(on_change: fn(bool)) {
    if ON_CHANGE.set(on_change).is_err() {
        return;
    }
    let Some(root) = root_domain() else {
        log::warn!(target: "lid", "lid_watch_failed error=no root power domain");
        return;
    };

    unsafe {
        let port = IONotificationPortCreate(IO_MAIN_PORT_DEFAULT);
        if port.is_null() {
            log::warn!(target: "lid", "lid_watch_failed error=no notification port");
            return;
        }
        let queue = DispatchQueue::new("io.voxfusion.lid", None);
        IONotificationPortSetDispatchQueue(port, DispatchRetained::as_ptr(&queue).as_ptr().cast());
        // The port and its queue last as long as the app.
        std::mem::forget(queue);

        let mut notification: IoObject = 0;
        let status = IOServiceAddInterestNotification(
            port,
            root,
            c"IOGeneralInterest".as_ptr(),
            on_power_message,
            std::ptr::null_mut(),
            &mut notification,
        );
        if status != 0 {
            log::warn!(target: "lid", "lid_watch_failed status={status}");
        }
    }
}

unsafe extern "C" fn on_power_message(
    _refcon: *mut c_void,
    _service: IoObject,
    message_type: u32,
    message_argument: *mut c_void,
) {
    if message_type != CLAMSHELL_STATE_CHANGE {
        return;
    }
    let closed = message_argument as usize & CLAMSHELL_CLOSED_BIT != 0;
    if let Some(on_change) = ON_CHANGE.get() {
        on_change(closed);
    }
}
