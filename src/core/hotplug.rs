//! macOS `IOKit` arrival/removal notifications for `SteelSeries` `HID` devices.
//!
//! The callback only signals "something changed"; the application re-runs its
//! PID-filtered HID scan before updating the connection status. A VID match
//! alone would also match other `SteelSeries` products and every `HID` interface
//! of the mouse.

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::mpsc::{Receiver, Sender};
    use std::thread;
    use std::time::Duration;

    use core_foundation_sys::base::CFRelease;
    use core_foundation_sys::dictionary::{CFDictionarySetValue, CFMutableDictionaryRef};
    use core_foundation_sys::number::{CFNumberCreate, kCFNumberSInt32Type};
    use core_foundation_sys::runloop::{
        CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRun, kCFRunLoopDefaultMode,
    };
    use io_kit_sys::hid::keys::{kIOHIDDeviceKey, kIOHIDVendorIDKey};
    use io_kit_sys::keys::{kIOFirstMatchNotification, kIOTerminatedNotification};
    use io_kit_sys::types::io_iterator_t;
    use io_kit_sys::{
        CFSTR, IOIteratorNext, IONotificationPortCreate, IONotificationPortDestroy,
        IONotificationPortGetRunLoopSource, IONotificationPortRef, IOObjectRelease,
        IOServiceAddMatchingNotification, IOServiceMatching, kIOMasterPortDefault,
    };

    use crate::core::device::{HidResponse, STEELSERIES_VID};

    /// One physical connect/disconnect produces one notification per matching HID
    /// interface. Waiting this long after the last one collapses the burst.
    const HOTPLUG_DEBOUNCE: Duration = Duration::from_millis(150);

    /// Internal signal from the `IOKit` callback to the coalescer thread.
    struct HotplugSignal;

    /// Resolves the modern default port, falling back to the legacy constant on older macOS.
    fn iokit_port() -> u32 {
        // SAFETY: the symbol name is a valid NUL-terminated C string and RTLD_DEFAULT
        // searches every image loaded into the process.
        let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"kIOMainPortDefault".as_ptr()) };
        if symbol.is_null() {
            // SAFETY: io-kit-sys exports this constant on all supported macOS versions.
            unsafe { kIOMasterPortDefault }
        } else {
            // SAFETY: when present, the symbol is IOKit's mach_port_t variable.
            unsafe { *symbol.cast::<u32>() }
        }
    }

    /// Spawns the thread that owns the `IOKit` notification port and run loop.
    pub fn watch(responses: Sender<HidResponse>) {
        thread::Builder::new()
            .name("aerox5-hotplug".to_owned())
            .spawn(move || run(responses))
            .expect("spawn IOKit hotplug watcher thread");
    }

    /// Owns every C object the notifications need; dropped when the run loop exits.
    ///
    /// The boxed sender is the callback's handle to the coalescer thread; dropping
    /// the watcher closes that channel and lets the coalescer thread finish.
    struct Watcher {
        port: IONotificationPortRef,
        matched: io_iterator_t,
        terminated: io_iterator_t,
        refcon: *mut Sender<HotplugSignal>,
    }

    /// Builds a fresh matching dictionary: `IOServiceAddMatchingNotification`
    /// consumes one reference per registration, so it cannot be reused.
    fn matching_dictionary(vid: u16) -> Option<CFMutableDictionaryRef> {
        // SAFETY: kIOHIDDeviceKey is a NUL-terminated constant from io-kit-sys;
        // a successful call returns a dictionary this thread owns.
        let matching = unsafe { IOServiceMatching(kIOHIDDeviceKey) };
        if matching.is_null() {
            return None;
        }

        let vid = i32::from(vid);
        // SAFETY: `vid` stays alive for the call and CFNumberCreate copies its value.
        let number = unsafe {
            CFNumberCreate(
                ptr::null(),
                kCFNumberSInt32Type,
                (&raw const vid).cast::<c_void>(),
            )
        };
        if number.is_null() {
            unsafe {
                // SAFETY: `matching` was created above and is released exactly once here.
                CFRelease(matching.cast::<c_void>());
            }
            return None;
        }
        // SAFETY: kIOHIDVendorIDKey is a NUL-terminated constant from io-kit-sys.
        let key = unsafe { CFSTR(kIOHIDVendorIDKey) };
        unsafe {
            // SAFETY: matching/number/key are valid; the dictionary retains key and value.
            CFDictionarySetValue(matching, key.cast::<c_void>(), number.cast::<c_void>());
        }
        unsafe {
            // SAFETY: the dictionary retains `number`, so our reference can be released.
            CFRelease(number.cast::<c_void>());
        }
        Some(matching)
    }

    impl Watcher {
        /// Registers both notifications, returning `None` if `IOKit` setup fails.
        fn new(signals: Sender<HotplugSignal>) -> Option<Self> {
            let mut watcher = Self {
                port: ptr::null_mut(),
                matched: 0,
                terminated: 0,
                refcon: Box::into_raw(Box::new(signals)),
            };

            // SAFETY: `iokit_port()` returns the default IOKit mach port; the
            // returned notification port is owned by `watcher` from here on.
            let port = unsafe { IONotificationPortCreate(iokit_port()) };
            if port.is_null() {
                drop(watcher);
                return None;
            }
            watcher.port = port;

            // SAFETY: `port` is non-null and owned by `watcher`.
            let source = unsafe { IONotificationPortGetRunLoopSource(port) };
            if source.is_null() {
                drop(watcher);
                return None;
            }
            // SAFETY: CFRunLoopGetCurrent creates this thread's run loop.
            let loop_ref = unsafe { CFRunLoopGetCurrent() };
            unsafe {
                // SAFETY: source and loop_ref are valid; the source lives until the port is destroyed.
                CFRunLoopAddSource(loop_ref, source, kCFRunLoopDefaultMode);
            }

            // SAFETY: the dictionary is valid and consumed by the registration;
            // `refcon` points to a sender alive until `watcher` is dropped.
            let Some(matching) = matching_dictionary(STEELSERIES_VID) else {
                drop(watcher);
                return None;
            };
            let status = unsafe {
                // SAFETY: all arguments are valid for the duration of the call.
                IOServiceAddMatchingNotification(
                    port,
                    kIOFirstMatchNotification,
                    matching,
                    on_device_change,
                    watcher.refcon.cast::<c_void>(),
                    &raw mut watcher.matched,
                )
            };
            if status != 0 {
                // IOKit consumed `matching` even when registration fails.
                drop(watcher);
                return None;
            }
            // SAFETY: the iterator was just returned by a successful registration.
            unsafe { drain(watcher.matched) };

            // SAFETY: a fresh dictionary is required because the first one was consumed.
            let Some(matching) = matching_dictionary(STEELSERIES_VID) else {
                drop(watcher);
                return None;
            };
            let status = unsafe {
                // SAFETY: all arguments are valid for the duration of the call.
                IOServiceAddMatchingNotification(
                    port,
                    kIOTerminatedNotification,
                    matching,
                    on_device_change,
                    watcher.refcon.cast::<c_void>(),
                    &raw mut watcher.terminated,
                )
            };
            if status != 0 {
                // IOKit consumed `matching` even when registration fails.
                drop(watcher);
                return None;
            }
            // SAFETY: the iterator was just returned by a successful registration.
            unsafe { drain(watcher.terminated) };

            Some(watcher)
        }
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            // The refcon is freed last so no callback can observe a dangling sender.
            if self.matched != 0 {
                unsafe {
                    // SAFETY: `matched` was validated when the registration succeeded.
                    IOObjectRelease(self.matched);
                }
            }
            if self.terminated != 0 {
                unsafe {
                    // SAFETY: `terminated` was validated when the registration succeeded.
                    IOObjectRelease(self.terminated);
                }
            }
            if !self.port.is_null() {
                unsafe {
                    // SAFETY: `port` was created by IONotificationPortCreate and is owned here.
                    IONotificationPortDestroy(self.port);
                }
            }
            if !self.refcon.is_null() {
                unsafe {
                    // SAFETY: `refcon` is the boxed sender created in `Watcher::new`, freed once.
                    drop(Box::from_raw(self.refcon));
                }
            }
        }
    }

    /// Empties an iterator so future notifications are delivered.
    ///
    /// # Safety
    /// `iterator` must be a live `io_iterator_t` that has not been released yet.
    unsafe fn drain(iterator: io_iterator_t) {
        loop {
            // SAFETY: the caller guarantees a live iterator.
            let service = unsafe { IOIteratorNext(iterator) };
            if service == 0 {
                break;
            }
            // SAFETY: `service` came from IOIteratorNext and is owned by us.
            unsafe { IOObjectRelease(service) };
        }
    }

    /// Re-arms the notification and hands the event to the coalescer thread.
    ///
    /// # Safety
    /// `refcon` must point to the `Sender<HotplugSignal>` boxed by `Watcher::new`,
    /// and `iterator` must be the live iterator supplied by `IOKit` for this event.
    unsafe extern "C" fn on_device_change(refcon: *mut c_void, iterator: io_iterator_t) {
        if iterator != 0 {
            // SAFETY: IOKit requires draining this iterator to arm the next event.
            unsafe { drain(iterator) };
        }

        if refcon.is_null() {
            return;
        }
        // SAFETY: the sender is kept alive by `Watcher` for as long as the
        // notification registrations exist; `send` returns an error instead of
        // panicking when the coalescer has gone away.
        let sender = unsafe { &*refcon.cast::<Sender<HotplugSignal>>() };
        let _ = sender.send(HotplugSignal);
    }

    /// Forwards one rescan request per burst of `IOKit` notifications.
    ///
    /// The first signal starts the wait and each further signal restarts it, so
    /// a burst from one physical device is reported only once it has subsided.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the coalescer thread must own both channel ends"
    )]
    fn coalesce(
        signals: Receiver<HotplugSignal>,
        responses: Sender<HidResponse>,
        debounce: Duration,
    ) {
        while signals.recv().is_ok() {
            // Drain trailing signals, restarting the wait after each one.
            while signals.recv_timeout(debounce).is_ok() {}
            if responses.send(HidResponse::Hotplug).is_err() {
                break;
            }
        }
    }

    fn run(responses: Sender<HidResponse>) {
        let (signal_tx, signal_rx) = std::sync::mpsc::channel();
        let coalescer = thread::Builder::new()
            .name("aerox5-hotplug-coalescer".to_owned())
            .spawn(move || coalesce(signal_rx, responses, HOTPLUG_DEBOUNCE))
            .expect("spawn IOKit hotplug coalescer thread");

        let Some(watcher) = Watcher::new(signal_tx) else {
            let _ = coalescer.join();
            return;
        };
        // SAFETY: the run loop source was added in `Watcher::new`; the watcher
        // stays alive until the loop is stopped (at process exit).
        unsafe { CFRunLoopRun() };
        drop(watcher);
        let _ = coalescer.join();
    }

    #[cfg(test)]
    mod tests {
        use std::sync::mpsc::channel;

        use core_foundation_sys::dictionary::CFDictionaryGetValue;
        use core_foundation_sys::number::CFNumberGetValue;
        use io_kit_sys::IOServiceGetMatchingServices;

        use super::*;

        /// Builds a fresh iterator over generic `IOKit` services. Unlike a HID-VID match this is
        /// non-empty on every macOS machine, so the callback test needs no attached hardware.
        fn fresh_iterator() -> io_iterator_t {
            // SAFETY: "IOService" is a NUL-terminated class name; a successful call returns an
            // iterator owned by the caller.
            let matching = unsafe { IOServiceMatching(c"IOService".as_ptr()) };
            assert!(
                !matching.is_null(),
                "IOServiceMatching must return a dictionary"
            );
            let mut iterator = 0;
            // SAFETY: `matching` is a valid dictionary consumed by the lookup.
            let status =
                unsafe { IOServiceGetMatchingServices(iokit_port(), matching, &raw mut iterator) };
            assert_eq!(status, 0, "IOKit must enumerate generic IOService objects");
            iterator
        }

        #[test]
        fn matching_dictionary_contains_the_requested_vendor_id() {
            let matching = matching_dictionary(STEELSERIES_VID).expect("matching dictionary");
            // SAFETY: kIOHIDVendorIDKey is a NUL-terminated constant from io-kit-sys.
            let key = unsafe { CFSTR(kIOHIDVendorIDKey) };
            // SAFETY: `matching` and `key` are valid for the duration of the lookup.
            let value = unsafe { CFDictionaryGetValue(matching, key.cast::<c_void>()) };
            assert!(
                !value.is_null(),
                "the matching dictionary must contain the vendor ID key"
            );
            let mut decoded = 0i32;
            // SAFETY: `value` is the CFNumber inserted by `matching_dictionary`.
            let is_number = unsafe {
                CFNumberGetValue(
                    value.cast(),
                    kCFNumberSInt32Type,
                    (&raw mut decoded).cast::<c_void>(),
                )
            };
            assert!(is_number, "the vendor ID must be stored as a CFNumber");
            assert_eq!(decoded, i32::from(STEELSERIES_VID));
            unsafe {
                // SAFETY: `matching` is the +1 reference from `matching_dictionary`.
                CFRelease(matching.cast::<c_void>());
            }
        }

        #[test]
        fn matching_dictionary_is_accepted_by_iokit() {
            let matching = matching_dictionary(STEELSERIES_VID).expect("matching dictionary");
            let mut iterator = 0;
            // SAFETY: `matching` is a valid dictionary consumed by the lookup.
            let status =
                unsafe { IOServiceGetMatchingServices(iokit_port(), matching, &raw mut iterator) };
            assert_eq!(status, 0, "IOKit must accept the VID matching dictionary");
            assert_ne!(iterator, 0, "IOKit must return an iterator handle");
            // SAFETY: `iterator` was returned by IOServiceGetMatchingServices.
            unsafe { IOObjectRelease(iterator) };
        }

        #[test]
        fn callback_drains_iterator_and_signals_one_hotplug() {
            // Prove this iterator class really yields objects on this machine.
            let probe = fresh_iterator();
            // SAFETY: `probe` is a live iterator returned above.
            let first = unsafe { IOIteratorNext(probe) };
            assert_ne!(first, 0, "IOKit must expose at least one service");
            // SAFETY: `first` came from IOIteratorNext and is ours to release.
            unsafe { IOObjectRelease(first) };
            // SAFETY: `probe` was returned by IOServiceGetMatchingServices.
            unsafe { IOObjectRelease(probe) };

            let iterator = fresh_iterator();
            let (signal_tx, signal_rx) = channel();
            let (response_tx, response_rx) = channel();
            let coalescer = thread::Builder::new()
                .name("aerox5_hotplug_callback_test".to_owned())
                .spawn(move || coalesce(signal_rx, response_tx, Duration::from_millis(30)))
                .expect("spawn test coalescer");
            let signal_tx = Box::into_raw(Box::new(signal_tx));
            // SAFETY: `signal_tx` is boxed for the call and `iterator` is live.
            unsafe { on_device_change(signal_tx.cast::<c_void>(), iterator) };

            // SAFETY: `iterator` is still owned by this test.
            let next = unsafe { IOIteratorNext(iterator) };
            assert_eq!(
                next, 0,
                "the callback must empty its iterator to re-arm the notification"
            );
            unsafe {
                // SAFETY: `iterator` was returned by IOServiceGetMatchingServices.
                IOObjectRelease(iterator);
            }
            assert!(matches!(
                response_rx.recv_timeout(Duration::from_secs(2)),
                Ok(HidResponse::Hotplug)
            ));
            unsafe {
                // SAFETY: the box was handed to the callback by raw pointer and is freed once.
                drop(Box::from_raw(signal_tx));
            }
            let _ = coalescer.join();
        }

        #[test]
        fn coalescer_collapses_a_burst_into_one_hotplug() {
            let (signal_tx, signal_rx) = channel();
            let (response_tx, response_rx) = channel();
            let debounce = Duration::from_millis(30);
            let coalescer = thread::Builder::new()
                .name("aerox5_hotplug_burst_test".to_owned())
                .spawn(move || coalesce(signal_rx, response_tx, debounce))
                .expect("spawn test coalescer");

            for _ in 0..5 {
                signal_tx
                    .send(HotplugSignal)
                    .expect("the coalescer must still be listening");
            }
            assert!(matches!(
                response_rx.recv_timeout(Duration::from_secs(2)),
                Ok(HidResponse::Hotplug)
            ));
            assert!(
                matches!(
                    response_rx.recv_timeout(debounce * 3),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ),
                "one burst must produce exactly one hotplug event"
            );

            // A separate burst after the window must still be forwarded.
            signal_tx
                .send(HotplugSignal)
                .expect("the coalescer must still be listening");
            assert!(matches!(
                response_rx.recv_timeout(Duration::from_secs(2)),
                Ok(HidResponse::Hotplug)
            ));

            drop(signal_tx);
            let _ = coalescer.join();
        }

        #[test]
        fn registers_both_notifications_for_steelseries_hid_devices() {
            let (sender, _receiver) = channel();
            let watcher = Watcher::new(sender);
            assert!(
                watcher.is_some(),
                "IOKit must accept the first-match and terminated registrations"
            );
            drop(watcher);
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::watch;

#[cfg(not(target_os = "macos"))]
/// Platform without IOKit: connection changes are detected by manual Refresh only.
pub fn watch(_responses: std::sync::mpsc::Sender<crate::core::device::HidResponse>) {}
