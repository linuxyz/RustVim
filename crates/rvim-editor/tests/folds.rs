//! Folding subsystem tests.

use std::path::PathBuf;

use rvim_editor::Editor;
use rvim_ui_api::{ActionKind, Key, KeyEvent, MouseButton, MouseEvent, MouseKind, Size, UiEvent};

fn temp_path(name: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "rvim-fold-{}-{}-{unique}-{name}",
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

fn key_char(ed: &mut Editor, ch: char) {
    ed.handle_key(KeyEvent::new(Key::Char(ch)));
}

fn eval(ed: &mut Editor, expression: &str) -> String {
    ed.eval_viml(expression)
        .unwrap_or_else(|error| panic!("{expression}: {error}"))
        .echo_string()
}

fn row_text(ed: &mut Editor, row: u16) -> String {
    let frame = ed.render();
    frame.base().row_text(row)
}

#[test]
fn indent_folds_compute_levels_and_render() {
    let (mut ed, path) = editor_with(
        "zero
    one
        two
    oneb
zero2
",
    );
    ed.execute_ex("set foldmethod=indent shiftwidth=4");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("set foldlevel=0");
    assert_eq!(eval(&mut ed, "foldlevel(1)"), "0");
    assert_eq!(eval(&mut ed, "foldlevel(2)"), "1");
    assert_eq!(eval(&mut ed, "foldlevel(3)"), "2");
    assert_eq!(eval(&mut ed, "foldlevel(4)"), "1");
    assert_eq!(eval(&mut ed, "foldlevel(5)"), "0");
    // The level-2 fold on line 3 is one line long and thus "small"; the outer
    // level-1 fold still closes and hides lines 2..4.
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "2");
    assert_eq!(eval(&mut ed, "foldclosedend(3)"), "4");
    assert_eq!(eval(&mut ed, "foldtextresult(2)"), "+--  3 lines: one");
    assert_eq!(eval(&mut ed, "foldtextresult(3)"), "+--  2 lines: two");
    assert!(row_text(&mut ed, 0).starts_with("zero"));
    assert!(
        row_text(&mut ed, 1).starts_with("+--  3 lines: one"),
        "{:?}",
        row_text(&mut ed, 1)
    );
    assert!(row_text(&mut ed, 2).starts_with("zero2"));
    ed.execute_ex("set foldlevel=1");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "-1");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "-1");
    let rows: Vec<String> = (0..5).map(|row| row_text(&mut ed, row)).collect();
    assert!(rows[1].starts_with("    one"), "{rows:?}");
    assert!(rows[2].starts_with("        two"), "{rows:?}");
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_folds_and_foldtext() {
    let (mut ed, path) = editor_with(
        "a {{{1
b
c }}}
d
",
    );
    ed.execute_ex("set foldmethod=marker foldlevel=0");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(eval(&mut ed, "foldlevel(1)"), "1");
    assert_eq!(eval(&mut ed, "foldclosed(1)"), "1");
    assert_eq!(eval(&mut ed, "foldclosedend(1)"), "3");
    assert_eq!(eval(&mut ed, "foldtextresult(1)"), "+--  3 lines: a ");
    assert_eq!(eval(&mut ed, "foldtextresult(2)"), "+--  2 lines: b");
    assert!(row_text(&mut ed, 0).starts_with("+--  3 lines: a "));
    // A custom 'foldtext' expression sees the v:fold* variables.
    ed.execute_ex("set foldtext=v:foldstart.'-'.v:foldend.'-'.v:foldlevel");
    assert_eq!(eval(&mut ed, "foldtextresult(1)"), "1-3-1");
    let _ = std::fs::remove_file(path);
}

#[test]
fn manual_folds_and_fold_commands() {
    let (mut ed, path) = editor_with(
        "l1
l2
l3
l4
l5
l6
",
    );
    ed.execute_ex("set foldmethod=manual");
    ed.execute_ex("set foldlevel=0");
    ed.execute_ex("2,4fold");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "2");
    assert_eq!(eval(&mut ed, "foldclosedend(3)"), "4");
    ed.execute_ex("3foldopen");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "-1");
    ed.execute_ex("3foldclose");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "2");
    // zo/zc/za through the z prefix keys.
    ed.execute_ex("normal! 3Gzo");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "-1");
    ed.execute_ex("normal! zc");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "2");
    ed.execute_ex("normal! za");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "-1");
    ed.execute_ex("normal! za");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "2");
    // zd removes the manual fold.
    ed.execute_ex("normal! zd");
    assert_eq!(eval(&mut ed, "foldclosed(3)"), "-1");
    assert_eq!(eval(&mut ed, "foldlevel(3)"), "0");
    let _ = std::fs::remove_file(path);
}

#[test]
fn fold_level_commands_and_foldenable() {
    let (mut ed, path) = editor_with(
        "zero
    one
    two
zero2
",
    );
    ed.execute_ex("set foldmethod=indent shiftwidth=4");
    ed.execute_ex("normal! zM");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "2");
    ed.execute_ex("normal! zR");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "-1");
    // zR sets 'foldlevel' to the deepest nesting (1 for this file).
    assert_eq!(eval(&mut ed, "&foldlevel"), "1");
    ed.execute_ex("normal! zm");
    assert_eq!(eval(&mut ed, "&foldlevel"), "0");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "2");
    ed.execute_ex("normal! zr");
    assert_eq!(eval(&mut ed, "&foldlevel"), "1");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "-1");
    // foldenable=off makes every fold appear open; foldclosed reports -1.
    ed.execute_ex("normal! zM");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "2");
    ed.execute_ex("set nofoldenable");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "-1");
    assert_eq!(eval(&mut ed, "foldlevel(2)"), "1");
    ed.execute_ex("set foldenable");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "2");
    assert_eq!(eval(&mut ed, "has('folding')"), "1");
    assert_eq!(eval(&mut ed, "exists('*foldclosed')"), "1");
    let _ = std::fs::remove_file(path);
}

#[test]
fn cursor_moves_over_closed_folds() {
    let (mut ed, path) = editor_with(
        "zero
    one
    two
zero2
",
    );
    ed.execute_ex("set foldmethod=indent shiftwidth=4 foldlevel=0");
    // Lines 2..3 are a closed fold.
    ed.execute_ex("1");
    key_char(&mut ed, 'j');
    assert_eq!(ed.cursor().lnum, 2, "j enters the fold at its start");
    key_char(&mut ed, 'j');
    assert_eq!(ed.cursor().lnum, 4, "j leaves the closed fold");
    key_char(&mut ed, 'k');
    assert_eq!(ed.cursor().lnum, 2, "k enters the fold at its start");
    // Jumping to a hidden line lands on the fold start.
    ed.execute_ex("3");
    assert_eq!(ed.cursor().lnum, 2);
    key_char(&mut ed, 'G');
    assert_eq!(ed.cursor().lnum, 4);
    let _ = std::fs::remove_file(path);
}

#[test]
fn fold_jump_motions() {
    let (mut ed, path) = editor_with(
        "a
b
c
d
e
f
",
    );
    ed.execute_ex("set foldmethod=manual foldlevel=0");
    ed.execute_ex("1,2fold");
    ed.execute_ex("5,6fold");
    ed.execute_ex("1");
    ed.execute_ex("normal! ]z");
    assert_eq!(ed.cursor().lnum, 5);
    ed.execute_ex("normal! [z");
    assert_eq!(ed.cursor().lnum, 1);
    ed.execute_ex("normal! zj");
    assert_eq!(ed.cursor().lnum, 5);
    ed.execute_ex("normal! zk");
    assert_eq!(ed.cursor().lnum, 1);
    let _ = std::fs::remove_file(path);
}

#[test]
fn foldcolumn_shows_levels() {
    let (mut ed, path) = editor_with(
        "zero
    one
        two
    oneb
zero2
",
    );
    ed.execute_ex("set foldmethod=indent shiftwidth=4 foldcolumn=2");
    ed.execute_ex("set foldlevel=0");
    let row = row_text(&mut ed, 1);
    assert!(row.starts_with("+ "), "{row:?}");
    ed.execute_ex("set foldlevel=1");
    let rows: Vec<String> = (0..5).map(|row| row_text(&mut ed, row)).collect();
    assert!(rows[0].starts_with("  "), "{rows:?}");
    assert!(rows[1].starts_with("- "), "{rows:?}");
    assert!(rows[2].starts_with("- "), "{rows:?}");
    assert!(rows[3].starts_with("| "), "{rows:?}");
    assert!(rows[4].starts_with("  "), "{rows:?}");
    // The fold column is clickable: it carries a Fold hit region and clicking
    // it toggles the fold.
    ed.execute_ex("set foldlevel=0");
    let frame = ed.render();
    assert!(frame
        .hit_regions
        .iter()
        .any(|region| matches!(region.action.kind, ActionKind::Fold) && region.rect.cols == 2));
    let region = frame
        .hit_regions
        .iter()
        .find(|region| matches!(region.action.kind, ActionKind::Fold) && region.rect.row == 1)
        .cloned()
        .expect("fold hit region on the fold line");
    ed.handle_event(UiEvent::Mouse(MouseEvent {
        row: region.rect.row,
        col: region.rect.col,
        kind: MouseKind::Press,
        button: MouseButton::Left,
        mods: Default::default(),
    }));
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "-1");
    let _ = std::fs::remove_file(path);
}

#[test]
fn fold_marker_creation_and_folddo() {
    let (mut ed, path) = editor_with(
        "a
b
",
    );
    ed.execute_ex("set foldmethod=marker");
    ed.execute_ex("set foldlevel=0");
    ed.execute_ex("1,2fold");
    assert_eq!(eval(&mut ed, "foldclosed(1)"), "1");
    let line1 = ed
        .current_buffer()
        .and_then(|buffer| buffer.text.line(1))
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .unwrap();
    assert!(line1.contains("{{{"), "{line1:?}");
    // :folddoopen runs a command only on lines that are not in a closed fold.
    let (mut ed, path2) = editor_with(
        "a
    b
    c
d
",
    );
    ed.execute_ex("set foldmethod=indent shiftwidth=4 foldlevel=0");
    ed.execute_ex("folddoopen s/./X/");
    let lines: Vec<String> = (1..=4)
        .map(|lnum| {
            ed.current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| String::from_utf8_lossy(line).into_owned())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(lines[0], "X");
    assert_eq!(lines[3], "X");
    assert_eq!(lines[1], "    b");
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path2);
}

#[test]
fn fold_with_other_method_reports_e350() {
    let (mut ed, path) = editor_with(
        "a
b
c
",
    );
    ed.execute_ex("set foldmethod=indent");
    ed.execute_ex("1,2fold");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
    let message = ed.messages.last.clone().unwrap_or_default();
    assert!(message.contains("E350"), "{message:?}");
    let _ = std::fs::remove_file(path);
}
