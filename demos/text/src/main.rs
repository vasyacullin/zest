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
    println!("History: {:?}", buffer.history_stats());
    println!(
        "Text model smoke passed: graphemes, directional selection, IME commit/cancel and undo/redo."
    );
    Ok(())
}
