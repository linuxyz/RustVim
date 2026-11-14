//! Signs subsystem tests.

use std::path::PathBuf;

use rvim_editor::Editor;
use rvim_ui_api::{Color, Size};

fn temp_path(name: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "rvim-sign-{}-{}-{unique}-{name}",
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
fn sign_define_place_and_render() {
    let (mut ed, path) = editor_with("l1\nl2\nl3\n");
    ed.execute_ex("sign define mark text=>> texthl=ErrorMsg");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("sign place 3 line=2 name=mark");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(eval(&mut ed, "len(sign_getdefined())"), "1");
    assert_eq!(eval(&mut ed, "len(sign_getplaced()[0].signs)"), "1");
    assert_eq!(eval(&mut ed, "sign_getplaced()[0].signs[0].lnum"), "2");
    assert_eq!(eval(&mut ed, "sign_getplaced()[0].signs[0].id"), "3");
    let lines = rows(&mut ed, 3);
    assert_eq!(lines[0], "  l1");
    assert_eq!(lines[1], ">>l2");
    assert_eq!(lines[2], "  l3");
    // "no" hides the column, "yes" always shows it.
    ed.execute_ex("set signcolumn=no");
    assert_eq!(rows(&mut ed, 1)[0], "l1");
    ed.execute_ex("set signcolumn=yes");
    assert_eq!(rows(&mut ed, 1)[0], "  l1");
    ed.execute_ex("sign unplace 3");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(eval(&mut ed, "len(sign_getplaced()[0].signs)"), "0");
    // "auto" reserves nothing once the last sign is gone.
    ed.execute_ex("set signcolumn=auto");
    assert_eq!(rows(&mut ed, 1)[0], "l1");
    let _ = std::fs::remove_file(path);
}

#[test]
fn sign_viml_functions_roundtrip() {
    let (mut ed, path) = editor_with("a\nb\nc\n");
    assert_eq!(eval(&mut ed, "sign_define('alpha', {'text': 'aa'})"), "0");
    assert_eq!(eval(&mut ed, "sign_getdefined()[0].name"), "alpha");
    assert_eq!(eval(&mut ed, "sign_getdefined()[0].text"), "aa");
    assert_eq!(
        eval(&mut ed, "sign_place(5, 'grp', 'alpha', 0, {'lnum': 2})"),
        "5"
    );
    assert_eq!(
        eval(&mut ed, "len(sign_getplaced(0, {'group': 'grp'})[0].signs)"),
        "1"
    );
    assert_eq!(eval(&mut ed, "sign_getplaced()[0].signs[0].id"), "5");
    assert_eq!(eval(&mut ed, "sign_getplaced()[0].signs[0].group"), "grp");
    assert_eq!(eval(&mut ed, "sign_jump(5, 'grp', 0)"), "2");
    assert_eq!(ed.cursor().lnum, 2);
    assert_eq!(
        eval(&mut ed, "sign_unplace('grp', {'bufnr': 0, 'id': 5})"),
        "0"
    );
    assert_eq!(eval(&mut ed, "len(sign_getplaced()[0].signs)"), "0");
    assert_eq!(eval(&mut ed, "sign_undefine('alpha')"), "0");
    assert_eq!(eval(&mut ed, "len(sign_getdefined())"), "0");
    // Lists of placements are accepted too.
    ed.execute_ex("sign define beta text=bb");
    assert_eq!(
        eval(
            &mut ed,
            "sign_placelist([{'name': 'beta', 'lnum': 1, 'buffer': 0, 'id': 7}])"
        ),
        "[7]"
    );
    assert_eq!(
        eval(&mut ed, "sign_unplacelist([{'buffer': 0, 'id': 7}])"),
        "[0]"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn sign_priority_and_groups() {
    let (mut ed, path) = editor_with("a\nb\nc\n");
    ed.execute_ex("sign define low text=aa");
    ed.execute_ex("sign define high text=bb");
    ed.execute_ex("sign place 1 line=2 name=low priority=5");
    ed.execute_ex("sign place 2 line=2 name=high priority=20");
    // The highest priority sign is displayed; the line text follows it.
    assert!(rows(&mut ed, 2)[1].starts_with("bbb"));
    // A group only hides that group's sign.
    ed.execute_ex("sign unplace 2 group=other");
    assert_eq!(rows(&mut ed, 2)[1].get(0..3), Some("bbb"));
    ed.execute_ex("sign place 3 group=other line=2 name=high priority=1");
    assert_eq!(rows(&mut ed, 2)[1].get(0..3), Some("bbb"));
    ed.execute_ex("sign unplace 2");
    assert!(rows(&mut ed, 2)[1].starts_with("aab"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn sign_in_number_column_and_highlights() {
    let (mut ed, path) = editor_with("a\nb\nc\n");
    ed.execute_ex("set number signcolumn=number");
    ed.execute_ex("sign define mark text=S> texthl=WarningMsg linehl=ErrorMsg");
    ed.execute_ex("sign place 1 line=2 name=mark");
    let frame = ed.render();
    let row = frame.base().row_text(1);
    assert!(row.contains("S>"), "{row:?}");
    assert!(row.contains('b'), "{row:?}");
    // linehl highlights the whole line: the text cell carries the ErrorMsg fg.
    let cell = frame.base().cell(1, row.find('b').unwrap() as u16).unwrap();
    let attr = frame.base().attr(cell);
    assert_eq!(attr.fg, Color::Indexed(9));
    // Without a number column, "number" behaves like "auto".
    ed.execute_ex("set nonumber");
    assert!(rows(&mut ed, 2)[1].starts_with("S>b"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn signs_follow_edits_and_undo() {
    let (mut ed, path) = editor_with("a\nb\nc\n");
    ed.execute_ex("sign define mark text=>>");
    ed.execute_ex("sign place 4 line=2 name=mark");
    assert_eq!(eval(&mut ed, "sign_getplaced()[0].signs[0].lnum"), "2");
    // Inserting a line above moves the sign down with its line.
    ed.execute_ex("normal! ggOnew<Esc>");
    assert_eq!(eval(&mut ed, "line('$')"), "4");
    assert_eq!(eval(&mut ed, "sign_getplaced()[0].signs[0].lnum"), "3");
    // Undo restores the line order and the sign follows.
    ed.execute_ex("undo");
    assert_eq!(eval(&mut ed, "sign_getplaced()[0].signs[0].lnum"), "2");
    // Deleting the sign's line removes the sign, like Vim.
    ed.execute_ex("2delete");
    assert_eq!(eval(&mut ed, "len(sign_getplaced()[0].signs)"), "0");
    let _ = std::fs::remove_file(path);
}

#[test]
fn sign_jump_and_list() {
    let (mut ed, path) = editor_with("a\nb\nc\nd\n");
    ed.execute_ex("sign define mark text=>>");
    ed.execute_ex("sign place 9 line=4 name=mark");
    ed.execute_ex("sign list");
    let message = ed.messages.last.clone().unwrap_or_default();
    assert!(message.contains("sign mark"), "{message:?}");
    ed.execute_ex("1");
    ed.execute_ex("sign jump 9");
    assert_eq!(ed.cursor().lnum, 4);
    assert_eq!(eval(&mut ed, "has('signs')"), "1");
    assert_eq!(eval(&mut ed, "exists('*sign_place')"), "1");
    let _ = std::fs::remove_file(path);
}
