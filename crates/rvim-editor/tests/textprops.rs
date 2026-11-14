//! Text properties (DSP-05) tests.

use std::path::PathBuf;

use rvim_editor::Editor;
use rvim_ui_api::{Color, Size};

fn temp_path(name: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "rvim-prop-{}-{}-{unique}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

fn editor_with(content: &str) -> (Editor, PathBuf) {
    let path = temp_path("buf.txt");
    std::fs::write(&path, content.as_bytes()).unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.messages.clear();
    (ed, path)
}

fn eval(ed: &mut Editor, expression: &str) -> String {
    ed.eval_viml(expression)
        .unwrap_or_else(|error| panic!("{expression}: {error}"))
        .echo_string()
}

fn rows(ed: &mut Editor, count: u16) -> Vec<String> {
    let frame = ed.render();
    (0..count)
        .map(|row| frame.base().row_text(row).trim_end().to_string())
        .collect()
}

#[test]
fn prop_type_lifecycle() {
    let (mut ed, path) = editor_with("abc\n");
    eval(
        &mut ed,
        "prop_type_add('tp', {'highlight': 'Comment', 'priority': 3})",
    );
    assert_eq!(eval(&mut ed, "len(prop_type_list())"), "1");
    assert_eq!(eval(&mut ed, "prop_type_get('tp').highlight"), "Comment");
    assert_eq!(eval(&mut ed, "prop_type_get('tp').priority"), "3");
    assert_eq!(eval(&mut ed, "prop_type_get('missing')"), "{}");
    // Defining the same type twice is E969.
    ed.eval_viml("prop_type_add('tp', {'highlight': 'Comment'})")
        .unwrap_err();
    eval(&mut ed, "prop_type_change('tp', {'priority': 7})");
    assert_eq!(eval(&mut ed, "prop_type_get('tp').priority"), "7");
    eval(&mut ed, "prop_type_delete('tp')");
    assert_eq!(eval(&mut ed, "len(prop_type_list())"), "0");
    // Deleting a missing type is not an error.
    eval(&mut ed, "prop_type_delete('tp')");
    // Unknown highlight groups are rejected (E970).
    ed.eval_viml("prop_type_add('bad', {'highlight': 'NoSuchGroup'})")
        .unwrap_err();
    assert_eq!(eval(&mut ed, "has('textprop')"), "1");
    assert_eq!(eval(&mut ed, "exists('*prop_add')"), "1");
    let _ = std::fs::remove_file(path);
}

#[test]
fn prop_add_and_list_fields() {
    let (mut ed, path) = editor_with("abcdef\nsecond line\n");
    eval(&mut ed, "prop_type_add('tp', {'highlight': 'Comment'})");
    assert_eq!(
        eval(&mut ed, "prop_add(1, 2, {'type': 'tp', 'length': 3})"),
        "0"
    );
    assert_eq!(
        eval(
            &mut ed,
            "prop_add(1, 5, {'type': 'tp', 'id': 9, 'length': 2})"
        ),
        "9"
    );
    let list = eval(&mut ed, "prop_list(1)");
    assert!(list.contains("'col': 2"), "{list}");
    assert!(list.contains("'length': 3"), "{list}");
    assert!(list.contains("'id': 9"), "{list}");
    assert!(list.contains("'type': 'tp'"), "{list}");
    // Multi-line property: end_col is exclusive and one-based.
    assert_eq!(
        eval(
            &mut ed,
            "prop_add(1, 3, {'type': 'tp', 'end_lnum': 2, 'end_col': 4})"
        ),
        "0"
    );
    let all = eval(&mut ed, "len(prop_list(1, {'end_lnum': -1}))");
    assert_eq!(all, "3");
    let _ = std::fs::remove_file(path);
}

#[test]
fn prop_add_list_places_multiple() {
    let (mut ed, path) = editor_with("abcdefghij\n");
    eval(&mut ed, "prop_type_add('tp', {'highlight': 'Comment'})");
    eval(
        &mut ed,
        "prop_add_list({'type': 'tp', 'id': 4}, [[1, 1, 1, 3], [1, 5, 1, 7], [1, 9, 1, 10]])",
    );
    let list = eval(&mut ed, "prop_list(1)");
    assert!(list.contains("'col': 1"), "{list}");
    assert!(list.contains("'col': 5"), "{list}");
    assert!(list.contains("'col': 9"), "{list}");
    let _ = std::fs::remove_file(path);
}

#[test]
fn prop_highlight_priority_and_combine() {
    let (mut ed, path) = editor_with("abcdef\n");
    eval(
        &mut ed,
        "prop_type_add('low', {'highlight': 'Comment', 'priority': 1})",
    );
    eval(
        &mut ed,
        "prop_type_add('high', {'highlight': 'Keyword', 'priority': 5})",
    );
    eval(&mut ed, "prop_add(1, 1, {'type': 'low', 'length': 2})");
    // Low priority highlight wins until a higher-priority property covers it.
    let frame = ed.render();
    let cell = frame.base().cell(0, 0).unwrap();
    assert_eq!(frame.base().attr(cell).fg, Color::Indexed(2));
    eval(&mut ed, "prop_add(1, 1, {'type': 'high', 'length': 1})");
    let frame = ed.render();
    let cell = frame.base().cell(0, 0).unwrap();
    assert_eq!(frame.base().attr(cell).fg, Color::Indexed(4));
    // The second character is still covered by the lower priority property.
    let cell = frame.base().cell(0, 1).unwrap();
    assert_eq!(frame.base().attr(cell).fg, Color::Indexed(2));
    // combine=FALSE replaces the lower highlight with the property highlight;
    // combine is accepted and stored either way.
    eval(&mut ed, "prop_type_change('high', {'combine': 0})");
    assert_eq!(eval(&mut ed, "prop_type_get('high').combine"), "0");
    let _ = std::fs::remove_file(path);
}

#[test]
fn prop_virtual_text_renders() {
    let (mut ed, path) = editor_with("abcdef\n");
    eval(&mut ed, "prop_type_add('tp', {'highlight': 'Comment'})");
    eval(&mut ed, "prop_add(1, 2, {'type': 'tp', 'text': '>>'})");
    assert_eq!(rows(&mut ed, 1)[0], "a>>bcdef");
    // Column zero with "after" appends to the end of the line.
    eval(
        &mut ed,
        "prop_add(1, 0, {'type': 'tp', 'text': ' [after]', 'text_align': 'after'})",
    );
    assert_eq!(rows(&mut ed, 1)[0], "a>>bcdef [after]");
    // The text property itself is reported by prop_list().
    let list = eval(&mut ed, "prop_list(1)");
    assert!(list.contains("'text': '>>'"), "{list}");
    let _ = std::fs::remove_file(path);
}

#[test]
fn prop_remove_clear_and_find() {
    let (mut ed, path) = editor_with("abcdef\nsecond\n");
    eval(&mut ed, "prop_type_add('a', {'highlight': 'Comment'})");
    eval(&mut ed, "prop_type_add('b', {'highlight': 'Keyword'})");
    eval(
        &mut ed,
        "prop_add(1, 1, {'type': 'a', 'id': 1, 'length': 1})",
    );
    eval(
        &mut ed,
        "prop_add(1, 3, {'type': 'a', 'id': 2, 'length': 1})",
    );
    eval(
        &mut ed,
        "prop_add(1, 5, {'type': 'a', 'id': 3, 'length': 1})",
    );
    // Remove only the first match by default.
    assert_eq!(eval(&mut ed, "prop_remove({'type': 'a'}, 1)"), "1");
    assert_eq!(eval(&mut ed, "len(prop_list(1))"), "2");
    assert_eq!(eval(&mut ed, "prop_remove({'type': 'a', 'all': 1})"), "2");
    assert_eq!(eval(&mut ed, "len(prop_list(1))"), "0");
    // id/type both matching.
    eval(
        &mut ed,
        "prop_add(1, 2, {'type': 'a', 'id': 7, 'length': 1})",
    );
    eval(
        &mut ed,
        "prop_add(1, 4, {'type': 'b', 'id': 7, 'length': 1})",
    );
    assert_eq!(
        eval(&mut ed, "prop_remove({'id': 7, 'type': 'a', 'both': 1})"),
        "1"
    );
    // prop_find finds the remaining property.
    let found = eval(&mut ed, "prop_find({'id': 7})");
    assert!(found.contains("'col': 4"), "{found}");
    assert!(found.contains("'lnum': 1"), "{found}");
    eval(&mut ed, "prop_clear(1)");
    assert_eq!(eval(&mut ed, "len(prop_list(1))"), "0");
    let _ = std::fs::remove_file(path);
}

#[test]
fn props_follow_edits_and_undo() {
    let (mut ed, path) = editor_with("one\ntwo\nthree\n");
    eval(&mut ed, "prop_type_add('tp', {'highlight': 'Comment'})");
    eval(&mut ed, "prop_add(2, 1, {'type': 'tp', 'length': 2})");
    assert_eq!(eval(&mut ed, "prop_list(2)[0].col"), "1");
    // Inserting a line above moves the property with its line.
    ed.execute_ex("normal! ggOnew<Esc>");
    assert_eq!(eval(&mut ed, "len(prop_list(2))"), "0");
    assert_eq!(eval(&mut ed, "len(prop_list(3))"), "1");
    // Undo keeps the property anchored to the same text.
    ed.execute_ex("undo");
    assert_eq!(eval(&mut ed, "len(prop_list(2))"), "1");
    // Deleting the anchored line removes the property.
    ed.execute_ex("2delete");
    assert_eq!(eval(&mut ed, "len(prop_list(1, {'end_lnum': -1}))"), "0");
    let _ = std::fs::remove_file(path);
}

#[test]
fn props_on_other_buffers() {
    let (mut ed, path) = editor_with("first\n");
    let path2 = temp_path("second.txt");
    std::fs::write(&path2, b"second\n").unwrap();
    ed.execute_ex(&format!("edit {}", path2.display()));
    let bufnr = ed.current_buffer_id();
    eval(&mut ed, "prop_type_add('tp', {'highlight': 'Comment'})");
    eval(
        &mut ed,
        &format!("prop_add(1, 1, {{'type': 'tp', 'length': 1, 'bufnr': {bufnr}}})"),
    );
    assert_eq!(
        eval(&mut ed, &format!("len(prop_list(1, {{'bufnr': {bufnr}}}))")),
        "1"
    );
    ed.execute_ex(&format!("edit {}", path.display()));
    assert_eq!(
        eval(&mut ed, &format!("len(prop_list(1, {{'bufnr': {bufnr}}}))")),
        "1"
    );
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path2);
}
