use super::*;

#[test]
fn clipboard_shortcuts_use_physical_keys_and_require_real_focused_input_without_preedit() {
    let mut input = TextInput::new("Привет");
    input.focus(true);
    input.buffer.select_all().unwrap();
    input.modifiers = command_modifier();
    for (key, code, action) in [
        ("с", KeyCode::KeyC, ClipboardAction::Copy),
        ("ч", KeyCode::KeyX, ClipboardAction::Cut),
        ("м", KeyCode::KeyV, ClipboardAction::Paste),
    ] {
        let result = input
            .key_physical(
                &character(key),
                PhysicalKey::Code(code),
                Some(key),
                true,
                false,
            )
            .unwrap();
        assert_eq!(result.clipboard, Some(action));
        assert!(!result.changed);
        assert_eq!(input.buffer.text(), "Привет");
        assert!(
            input
                .key_physical(
                    &character(key),
                    PhysicalKey::Code(code),
                    Some(key),
                    false,
                    false
                )
                .unwrap()
                .clipboard
                .is_none()
        );
        assert!(
            input
                .key_physical(
                    &character(key),
                    PhysicalKey::Code(code),
                    Some(key),
                    true,
                    true
                )
                .unwrap()
                .clipboard
                .is_none()
        );
    }
    input.ime(&Ime::Enabled).unwrap();
    input.ime(&Ime::Preedit("draft".into(), None)).unwrap();
    assert!(
        input
            .key(&character("v"), Some("v"), true, false)
            .unwrap()
            .clipboard
            .is_none()
    );
    assert_eq!(input.buffer.display_text(), "draft");
    input.focus(false);
    input.modifiers = command_modifier();
    assert!(
        input
            .key(&character("x"), Some("x"), true, false)
            .unwrap()
            .clipboard
            .is_none()
    );
    input.focus(true);
    input.modifiers = ModifiersState::CONTROL | ModifiersState::ALT;
    let result = input
        .key_physical(
            &character("©"),
            PhysicalKey::Code(KeyCode::KeyC),
            Some("©"),
            true,
            false,
        )
        .unwrap();
    assert!(result.clipboard.is_none());
    assert_eq!(input.buffer.text(), "©");
}

#[test]
fn layout_navigation_requests_preserve_text_and_handle_document_shortcuts() {
    let mut input = TextInput::new("one\ntwo");
    input.focus(true);
    for (key, movement) in [
        (NamedKey::Home, Navigation::LineStart),
        (NamedKey::End, Navigation::LineEnd),
        (NamedKey::ArrowUp, Navigation::Up),
        (NamedKey::ArrowDown, Navigation::Down),
    ] {
        let result = input
            .key(&named(key), Some("incidental"), true, false)
            .unwrap();
        assert_eq!(result.navigation, Some(movement));
        assert!(!result.changed);
    }
    input.modifiers = command_modifier() | ModifiersState::SHIFT;
    assert_eq!(
        input
            .key(&named(NamedKey::Home), None, true, false)
            .unwrap()
            .navigation,
        Some(Navigation::DocumentStart)
    );
    assert_eq!(
        input
            .key(&named(NamedKey::End), None, true, false)
            .unwrap()
            .navigation,
        Some(Navigation::DocumentEnd)
    );
    input.modifiers = ModifiersState::ALT;
    assert_eq!(
        input
            .key(&named(NamedKey::ArrowUp), Some("incidental"), true, false)
            .unwrap(),
        KeyResult::default()
    );
    assert_eq!(input.buffer.text(), "one\ntwo");
    assert_eq!(input.buffer.history_stats().undo_edits, 0);
}

#[test]
fn command_shortcuts_follow_physical_positions_on_cyrillic_layouts() {
    let mut input = TextInput::new("base");
    input.focus(true);
    input.buffer.insert("!").unwrap();
    input.modifiers = command_modifier();
    input
        .key_physical(
            &character("я"),
            PhysicalKey::Code(KeyCode::KeyZ),
            Some("я"),
            true,
            false,
        )
        .unwrap();
    assert_eq!(input.buffer.text(), "base");
    input
        .key_physical(
            &character("н"),
            PhysicalKey::Code(KeyCode::KeyY),
            Some("н"),
            true,
            false,
        )
        .unwrap();
    assert_eq!(input.buffer.text(), "base!");
    input
        .key_physical(
            &character("ф"),
            PhysicalKey::Code(KeyCode::KeyA),
            Some("ф"),
            true,
            false,
        )
        .unwrap();
    assert_eq!(input.buffer.selected_text(), "base!");
    input.modifiers = ModifiersState::CONTROL | ModifiersState::ALT;
    input
        .key_physical(
            &character("@"),
            PhysicalKey::Code(KeyCode::KeyA),
            Some("@"),
            true,
            false,
        )
        .unwrap();
    assert_eq!(input.buffer.text(), "@");
}
fn named(key: NamedKey) -> Key {
    Key::Named(key)
}
fn character(key: &str) -> Key {
    Key::Character(key.into())
}
fn command_modifier() -> ModifiersState {
    if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        ModifiersState::CONTROL
    }
}

#[test]
fn typing_shortcuts_graphemes_and_synthetic_events_use_the_real_binding() {
    let mut input = TextInput::new("e\u{301}👩🏽‍💻");
    assert!(
        !input
            .key(&character("x"), Some("x"), true, false)
            .unwrap()
            .changed
    );
    input.focus(true);
    assert!(
        !input
            .key(&character("x"), Some("x"), true, true)
            .unwrap()
            .changed
    );
    assert!(
        !input
            .key(&character("x"), Some("x"), false, false)
            .unwrap()
            .changed
    );
    input
        .key(&named(NamedKey::Backspace), Some("\u{8}"), true, false)
        .unwrap();
    assert_eq!(input.buffer.text(), "e\u{301}");
    input.modifiers = command_modifier();
    input.key(&character("a"), Some("a"), true, false).unwrap();
    input.modifiers = ModifiersState::empty();
    input.key(&character("Ж"), Some("Ж"), true, false).unwrap();
    assert_eq!(input.buffer.text(), "Ж");
    input.modifiers = command_modifier();
    input.key(&character("z"), Some("z"), true, false).unwrap();
    assert_eq!(input.buffer.text(), "e\u{301}");
    input.modifiers |= ModifiersState::SHIFT;
    input.key(&character("z"), None, true, false).unwrap();
    assert_eq!(input.buffer.text(), "Ж");
    input.modifiers = ModifiersState::SHIFT;
    let movement = input
        .key(&named(NamedKey::ArrowLeft), None, true, false)
        .unwrap();
    assert_eq!(movement.navigation, Some(Navigation::Left));
    assert!(input.buffer.selection().is_caret()); // the layout owner applies navigation
}

#[test]
fn ime_clear_before_commit_is_one_edit_and_cancel_or_blur_never_commits() {
    let mut input = TextInput::new("OLD");
    input.focus(true);
    input.buffer.select_all().unwrap();
    input.ime(&Ime::Commit("ignored".into())).unwrap();
    assert_eq!(input.buffer.text(), "OLD");
    input.ime(&Ime::Enabled).unwrap();
    input
        .ime(&Ime::Preedit("にほん".into(), Some((0, 9))))
        .unwrap();
    assert_eq!(input.buffer.display_text(), "にほん");
    input
        .key(&named(NamedKey::Backspace), None, true, false)
        .unwrap();
    assert_eq!(input.buffer.text(), "OLD");
    input.ime(&Ime::Preedit("".into(), None)).unwrap();
    input.ime(&Ime::Commit("日本".into())).unwrap();
    assert_eq!(input.buffer.text(), "日本");
    assert_eq!(input.buffer.history_stats().undo_edits, 1);
    input.ime(&Ime::Preedit("cancel".into(), None)).unwrap();
    let result = input
        .key(&named(NamedKey::Escape), None, true, false)
        .unwrap();
    assert!(result.changed && result.reset_ime && !result.close);
    input.ime(&Ime::Commit("late".into())).unwrap();
    assert_eq!(input.buffer.text(), "日本");
    input.ime(&Ime::Enabled).unwrap();
    input.ime(&Ime::Preedit("blur".into(), None)).unwrap();
    input.modifiers = command_modifier();
    input.focus(false);
    input.ime(&Ime::Commit("late".into())).unwrap();
    assert_eq!(input.buffer.text(), "日本");
    assert!(input.buffer.preedit().is_none());
    assert!(input.modifiers.is_empty());
    input.focus(true);
    assert!(
        input
            .key(&named(NamedKey::Escape), None, true, false)
            .unwrap()
            .close
    );
}

#[test]
fn text_payload_preserves_altgr_option_and_filters_controls() {
    let mut input = TextInput::new("");
    input.focus(true);
    input.modifiers = ModifiersState::CONTROL | ModifiersState::ALT;
    input.key(&character("@"), Some("@"), true, false).unwrap();
    input.modifiers = ModifiersState::ALT;
    input.key(&character("é"), Some("é"), true, false).unwrap();
    input.modifiers = ModifiersState::empty();
    input
        .key(&character("x"), Some("\u{7f}"), true, false)
        .unwrap();
    input
        .key(&named(NamedKey::Enter), Some("\r"), true, false)
        .unwrap();
    assert_eq!(input.buffer.text(), "@é\n");
}

#[test]
fn malformed_ime_offsets_are_atomic_and_disabled_clears_preedit() {
    let mut input = TextInput::new("x");
    input.focus(true);
    input.ime(&Ime::Enabled).unwrap();
    input
        .ime(&Ime::Preedit("e\u{301}".into(), Some((1, 1))))
        .unwrap();
    assert!(input.ime(&Ime::Preedit("日".into(), Some((1, 1)))).is_err());
    assert_eq!(input.buffer.display_text(), "xe\u{301}");
    input.ime(&Ime::Disabled).unwrap();
    assert_eq!(input.buffer.display_text(), "x");
    input.ime(&Ime::Preedit("late".into(), None)).unwrap();
    assert!(input.buffer.preedit().is_none());
}
