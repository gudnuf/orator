use rdev::{listen, Event, EventType, Key};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Spawn a global hotkey listener in a background thread.
///
/// The listener sets `recording` to `true` on Right Alt (Right Option on macOS)
/// press and back to `false` on release. rdev::listen() blocks, so it must run
/// in its own thread.
pub fn start_hotkey_listener(recording: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let callback = move |event: Event| match event.event_type {
            EventType::KeyPress(Key::AltGr) => {
                recording.store(true, Ordering::Relaxed);
            }
            EventType::KeyRelease(Key::AltGr) => {
                recording.store(false, Ordering::Relaxed);
            }
            _ => {}
        };

        if let Err(e) = listen(callback) {
            eprintln!("Hotkey listener error: {:?}", e);
        }
    });
}
