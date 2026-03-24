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

/// Visual style for the floating overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayStyle {
    /// Dark vibrancy with electric blue left-edge stripe (default).
    Bifrost,
    /// Dark HUD with amber glowing dot indicator.
    Stormforge,
    /// Minimal black terminal aesthetic with blinking cursor.
    Uru,
}

impl OverlayStyle {
    /// Parse a style name (case-insensitive). Returns None for unknown names.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "bifrost" => Some(Self::Bifrost),
            "stormforge" => Some(Self::Stormforge),
            "uru" => Some(Self::Uru),
            _ => None,
        }
    }
}

/// State for the overlay, consumed by the platform-specific run loop.
pub struct OverlayState {
    pub receiver: mpsc::Receiver<OverlayMsg>,
    pub style: OverlayStyle,
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::run_overlay;
