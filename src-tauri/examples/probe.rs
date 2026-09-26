//! Ruční sonda na macOS: hlavní vlákno pumpuje run loop (jako v appce) a
//! každou sekundu vypíše aplikaci v popředí a nečinnost.
//! `cargo run --example probe` a mezitím přepínej aplikace.

#[cfg(target_os = "macos")]
fn main() {
    use std::ffi::c_void;
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, ret: u8) -> i32;
    }
    for _ in 0..8 {
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0, 0) };
        println!(
            "app={:?} idle={}s",
            homeofficetracker_lib::platform::frontmost_app(),
            homeofficetracker_lib::platform::idle_seconds()
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    for _ in 0..8 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        println!(
            "app={:?} idle={}s",
            homeofficetracker_lib::platform::frontmost_app(),
            homeofficetracker_lib::platform::idle_seconds()
        );
    }
}
