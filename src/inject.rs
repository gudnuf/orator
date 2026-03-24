use anyhow::{Context, Result};
use enigo::{Enigo, Keyboard, Settings};

pub struct TextInjector {
    enigo: Enigo,
}

impl TextInjector {
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default())
            .context("Failed to create Enigo instance. On macOS, grant Accessibility permission in System Settings > Privacy & Security > Accessibility.")?;
        Ok(Self { enigo })
    }

    pub fn type_text(&mut self, text: &str) -> Result<()> {
        self.enigo
            .text(text)
            .context("Failed to inject text into active window")?;
        Ok(())
    }
}
