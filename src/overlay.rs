use std::sync::mpsc;

/// Messages sent from the background (voice) thread to the overlay UI thread.
pub enum OverlayMsg {
    /// Recording started -- show the overlay.
    Show,
    /// Update the displayed transcript text.
    UpdateText(String),
    /// Recording stopped -- hide the overlay.
    Hide,
    /// Shut down the overlay (app should quit).
    Quit,
}

/// State for the overlay, consumed by the platform-specific run loop.
pub struct OverlayState {
    pub receiver: mpsc::Receiver<OverlayMsg>,
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::run_overlay;
