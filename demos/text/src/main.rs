use zest::text::{Movement, Selection, TextBuffer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = TextBuffer::new("Привет, 👩🏽‍💻!");
    buffer.move_cursor(Movement::PreviousGrapheme, false)?;
    buffer.backspace()?;
    assert_eq!(buffer.text(), "Привет, !");
    buffer.undo()?;
    assert_eq!(buffer.text(), "Привет, 👩🏽‍💻!");
    println!("Grapheme deletion/undo: {}", buffer.text());

    buffer.select_all()?;
    let selection = buffer.selection();
    buffer.set_preedit("にほん", Some(Selection::new(0, "にほん".len())))?;
    assert_eq!(buffer.text(), "Привет, 👩🏽‍💻!");
    println!("Transient composition: {}", buffer.display_text());
    buffer.commit_preedit("日本語");
    assert_eq!(buffer.text(), "日本語");
    buffer.undo()?;
    assert_eq!(buffer.text(), "Привет, 👩🏽‍💻!");
    assert_eq!(buffer.selection(), selection);
    buffer.redo()?;
    assert_eq!(buffer.text(), "日本語");
    println!("Committed composition/redo: {}", buffer.text());

    buffer.move_cursor(Movement::DocumentEnd, false)?;
    buffer.insert(" e\u{301} 🇯🇵 שלום")?;
    let committed = buffer.text().to_owned();
    buffer.set_preedit("cancelled", None)?;
    buffer.cancel_preedit();
    assert_eq!(buffer.display_text(), committed);
    println!("Mixed Unicode document: {}", buffer.text());
    buffer.move_cursor(Movement::PreviousWord, true)?;
    assert_eq!(buffer.selected_text(), "שלום");
    let selected = buffer.selection();
    buffer.delete_previous_word()?;
    assert_eq!(buffer.text(), "日本語 e\u{301} 🇯🇵 ");
    buffer.undo()?;
    assert_eq!(buffer.text(), committed);
    assert_eq!(buffer.selection(), selected);
    buffer.move_cursor(Movement::DocumentStart, false)?;
    buffer.delete_next_word()?;
    assert_eq!(buffer.text(), "本語 e\u{301} 🇯🇵 שלום");
    buffer.undo()?;
    assert_eq!(buffer.text(), committed);
    println!("History: {:?}", buffer.history_stats());
    println!(
        "Text model smoke passed: graphemes, Unicode word motion/deletion, directional selection, IME commit/cancel and undo/redo."
    );
    Ok(())
}
