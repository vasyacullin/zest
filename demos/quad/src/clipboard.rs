//! Plain-text clipboard boundary. The OS backend is opened lazily on a user
//! command and retained for Linux clipboard ownership. Tests/smoke use memory.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClipboardAction {
    Copy,
    Cut,
    Paste,
}

#[derive(Debug)]
pub(super) struct ClipboardError(pub String);
impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ClipboardError {}

pub(super) trait TextClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError>;
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

#[derive(Default)]
pub(super) struct SystemClipboard {
    inner: Option<arboard::Clipboard>,
}
impl SystemClipboard {
    fn backend(&mut self) -> Result<&mut arboard::Clipboard, ClipboardError> {
        if self.inner.is_none() {
            self.inner =
                Some(arboard::Clipboard::new().map_err(|error| {
                    ClipboardError(format!("clipboard initialization: {error}"))
                })?);
        }
        Ok(self.inner.as_mut().expect("clipboard initialized above"))
    }
}
impl TextClipboard for SystemClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError> {
        self.backend()?
            .get_text()
            .map_err(|error| ClipboardError(format!("clipboard read: {error}")))
    }
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.backend()?
            .set_text(text)
            .map_err(|error| ClipboardError(format!("clipboard write: {error}")))
    }
}

pub(super) const MAX_PASTE_BYTES: usize = 64 * 1024;

/// Normalize platform line endings at the UI boundary; the core buffer itself
/// preserves its input. Keep tabs and Unicode sequences, reject other controls.
pub(super) fn paste_text(text: &str) -> Result<String, ClipboardError> {
    if text.len() > MAX_PASTE_BYTES {
        return Err(ClipboardError("paste exceeds the 64 KiB text limit".into()));
    }
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ClipboardError(
            "paste contains unsupported control characters".into(),
        ));
    }
    Ok(text)
}

/// Deterministic provider shared by unit tests and the ordinary GPU smoke path.
#[derive(Default)]
pub(super) struct MemoryClipboard {
    pub text: Option<String>,
    pub fail_read: bool,
    pub fail_write: bool,
    pub reads: usize,
    pub writes: usize,
}
impl TextClipboard for MemoryClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError> {
        self.reads += 1;
        if self.fail_read {
            return Err(ClipboardError("injected clipboard read failure".into()));
        }
        self.text
            .clone()
            .ok_or_else(|| ClipboardError("clipboard contains no plain text".into()))
    }
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.writes += 1;
        if self.fail_write {
            return Err(ClipboardError("injected clipboard write failure".into()));
        }
        self.text = Some(text.to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paste_normalizes_line_endings_and_preserves_unicode_and_tabs() {
        assert_eq!(
            paste_text("Привет\r\nالعربية\re\u{301}\t👩🏽‍💻").unwrap(),
            "Привет\nالعربية\ne\u{301}\t👩🏽‍💻"
        );
        assert!(paste_text("").unwrap().is_empty());
        assert!(paste_text(&"x".repeat(MAX_PASTE_BYTES)).is_ok());
        assert!(paste_text(&"x".repeat(MAX_PASTE_BYTES + 1)).is_err());
        for text in ["a\0b", "a\u{7}b", "a\u{7f}b"] {
            assert!(paste_text(text).is_err());
        }
    }
}
