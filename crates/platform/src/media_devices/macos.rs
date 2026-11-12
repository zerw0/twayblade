use objc2_foundation::{NSNotificationCenter, NSString};
use std::{
    ffi::{c_char, c_void},
    sync::OnceLock,
};
use tokio::sync::watch;
#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}
fn address(selector: &[u8; 4]) -> Address {
    Address {
        selector: u32::from_be_bytes(*selector),
        scope: u32::from_be_bytes(*b"glob"),
        element: 0,
    }
}
type Listener = unsafe extern "C" fn(u32, u32, *const Address, *mut c_void) -> i32;
#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectAddPropertyListener(
        object: u32,
        address: *const Address,
        listener: Listener,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringGetCString(
        string: *const c_void,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> bool;
    fn CFRelease(value: *const c_void);
}
fn channel() -> &'static watch::Sender<u64> {
    static CHANNEL: OnceLock<watch::Sender<u64>> = OnceLock::new();
    CHANNEL.get_or_init(|| watch::channel(0).0)
}
unsafe extern "C" fn changed(
    _object: u32,
    _count: u32,
    _addresses: *const Address,
    _data: *mut c_void,
) -> i32 {
    channel().send_modify(|n| *n = n.wrapping_add(1));
    0
}
pub fn changes() -> watch::Receiver<u64> {
    static START: OnceLock<()> = OnceLock::new();
    START.get_or_init(|| {
        std::thread::spawn(|| {
            // System object ID and selectors from AudioHardware.h. Callback uses
            // no borrowed data; registrations last for the process lifetime.
            for selector in [b"dev#", b"dIn ", b"dOut"] {
                unsafe {
                    AudioObjectAddPropertyListener(
                        1,
                        &address(selector),
                        changed,
                        std::ptr::null_mut(),
                    );
                }
            }
            let center = NSNotificationCenter::defaultCenter();
            let callback = block2::RcBlock::new(
                move |_notification: std::ptr::NonNull<objc2_foundation::NSNotification>| {
                    channel().send_modify(|n| *n = n.wrapping_add(1));
                },
            );
            let mut observers = Vec::new();
            for name in [
                "AVCaptureDeviceWasConnectedNotification",
                "AVCaptureDeviceWasDisconnectedNotification",
            ] {
                observers.push(unsafe {
                    center.addObserverForName_object_queue_usingBlock(
                        Some(&NSString::from_str(name)),
                        None,
                        None,
                        &callback,
                    )
                });
            }
            // Retain observers and callback for the lifetime of the native listeners.
            loop {
                std::thread::park();
            }
        });
    });
    channel().subscribe()
}
pub fn default_audio(input: bool) -> Option<String> {
    let mut device = 0u32;
    let mut size = 4u32;
    // SAFETY: CoreAudio writes exactly the sized scalar/CFStringRef requested.
    // The property returns an owned CFString which is released after conversion.
    unsafe {
        if AudioObjectGetPropertyData(
            1,
            &address(if input { b"dIn " } else { b"dOut" }),
            0,
            std::ptr::null(),
            &mut size,
            (&mut device as *mut u32).cast(),
        ) != 0
        {
            return None;
        }
        let mut uid: *const c_void = std::ptr::null();
        size = std::mem::size_of_val(&uid) as u32;
        if AudioObjectGetPropertyData(
            device,
            &address(b"uid "),
            0,
            std::ptr::null(),
            &mut size,
            (&mut uid as *mut *const c_void).cast(),
        ) != 0
            || uid.is_null()
        {
            return None;
        }
        let mut buffer = [0u8; 4096];
        let success = CFStringGetCString(
            uid,
            buffer.as_mut_ptr().cast(),
            buffer.len() as isize,
            0x08000100,
        );
        CFRelease(uid);
        if !success {
            return None;
        }
        let end = buffer.iter().position(|b| *b == 0)?;
        String::from_utf8(buffer[..end].to_vec()).ok()
    }
}
