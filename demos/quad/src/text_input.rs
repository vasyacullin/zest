//! Winit-to-model binding kept in the demo until the platform crate exists.
use winit::{
    event::Ime,
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey},
};
use zest::text::{EditError, Movement, Selection, TextBuffer};

#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct KeyResult {
    pub changed: bool,
    pub close: bool,
    pub reset_ime: bool,
}

#[derive(Clone)]
pub(super) struct TextInput {
    pub buffer: TextBuffer,
    pub focused: bool,
    pub ime_enabled: bool,
    pub modifiers: ModifiersState,
}
impl TextInput {
    pub fn new(text: &str) -> Self {
        Self {
            buffer: TextBuffer::new(text),
            focused: false,
            ime_enabled: false,
            modifiers: ModifiersState::empty(),
        }
    }
    pub fn focus(&mut self, focused: bool) -> bool {
        let changed = self.focused != focused;
        self.focused = focused;
        if !focused {
            self.ime_enabled = false;
            self.modifiers = ModifiersState::empty();
            return self.buffer.cancel_preedit() || changed;
        }
        changed
    }
    pub fn cancel_composition(&mut self) -> bool {
        self.ime_enabled = false;
        self.buffer.cancel_preedit()
    }
    pub fn ime(&mut self, ime: &Ime) -> Result<bool, EditError> {
        if !self.focused {
            return Ok(false);
        }
        match ime {
            Ime::Enabled => {
                self.ime_enabled = true;
                Ok(true)
            }
            Ime::Disabled => {
                self.ime_enabled = false;
                Ok(self.buffer.cancel_preedit())
            }
            Ime::Preedit(text, cursor) if self.ime_enabled => self
                .buffer
                .set_preedit(text, cursor.map(|(a, b)| Selection::new(a, b))),
            Ime::Commit(text) if self.ime_enabled => Ok(self.buffer.commit_preedit(text)),
            _ => Ok(false),
        }
    }
    // The small argument set also lets smoke tests replay the exact binding
    // used for real events, without fabricating a platform-private KeyEvent.
    pub fn key_physical(
        &mut self,
        key: &Key,
        physical: PhysicalKey,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, EditError> {
        let primary = if cfg!(target_os = "macos") {
            self.modifiers.super_key()
        } else {
            self.modifiers.control_key()
        };
        let shortcut = if primary && !self.modifiers.alt_key() {
            match physical {
                PhysicalKey::Code(KeyCode::KeyA) => Some(Key::Character("a".into())),
                PhysicalKey::Code(KeyCode::KeyZ) => Some(Key::Character("z".into())),
                PhysicalKey::Code(KeyCode::KeyY) => Some(Key::Character("y".into())),
                _ => None,
            }
        } else {
            None
        };
        self.key(shortcut.as_ref().unwrap_or(key), text, pressed, synthetic)
    }
    pub fn key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, EditError> {
        let mut result = KeyResult::default();
        if !pressed || synthetic {
            return Ok(result);
        }
        if key == &Key::Named(NamedKey::Escape) {
            if self.focused && self.buffer.preedit().is_some() {
                result.changed = self.cancel_composition();
                result.reset_ime = true;
            } else {
                result.close = true;
            }
            return Ok(result);
        }
        if !self.focused || self.buffer.preedit().is_some() {
            return Ok(result);
        }
        let primary = if cfg!(target_os = "macos") {
            self.modifiers.super_key()
        } else {
            self.modifiers.control_key()
        };
        let shortcut = primary && !self.modifiers.alt_key();
        if shortcut {
            result.changed = match key.as_ref() {
                Key::Character(c) if c.eq_ignore_ascii_case("a") => self.buffer.select_all()?,
                Key::Character(c) if c.eq_ignore_ascii_case("z") => {
                    if self.modifiers.shift_key() {
                        self.buffer.redo()?
                    } else {
                        self.buffer.undo()?
                    }
                }
                Key::Character(c) if c.eq_ignore_ascii_case("y") => self.buffer.redo()?,
                _ => false,
            };
            return Ok(result);
        }
        // Ctrl+Alt text is allowed for AltGr layouts. Cmd/Ctrl shortcuts and
        // synthetic focus events must not insert their incidental text payload.
        if self.modifiers.super_key() || (self.modifiers.control_key() && !self.modifiers.alt_key())
        {
            return Ok(result);
        }
        result.changed = match key {
            Key::Named(NamedKey::Backspace) => self.buffer.backspace()?,
            Key::Named(NamedKey::Delete) => self.buffer.delete_forward()?,
            Key::Named(NamedKey::ArrowLeft) => self
                .buffer
                .move_cursor(Movement::PreviousGrapheme, self.modifiers.shift_key())?,
            Key::Named(NamedKey::ArrowRight) => self
                .buffer
                .move_cursor(Movement::NextGrapheme, self.modifiers.shift_key())?,
            Key::Named(NamedKey::Home) => self
                .buffer
                .move_cursor(Movement::DocumentStart, self.modifiers.shift_key())?,
            Key::Named(NamedKey::End) => self
                .buffer
                .move_cursor(Movement::DocumentEnd, self.modifiers.shift_key())?,
            Key::Named(NamedKey::Enter) => self.buffer.insert("\n")?,
            _ => {
                match text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control)) {
                    Some(text) => self.buffer.insert(text)?,
                    None => false,
                }
            }
        };
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
