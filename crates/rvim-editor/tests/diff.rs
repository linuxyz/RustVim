//! Diff subsystem tests (DSP-11/12 core).

use std::path::PathBuf;

use rvim_editor::Editor;
use rvim_ui_api::{Color, Size};

fn temp_path(name: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "rvim-diff-{}-{}-{unique}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

fn eval(ed: &mut Editor, expression: &str) -> String {
    ed.eval_viml(expression)
        .unwrap_or_else(|error| panic!("{expression}: {error}"))
        .echo_string()
}

fn rows(ed: &mut Editor, count: u16) -> Vec<String> {
    let window = ed.view.current_window().unwrap().id;
    let rect = ed
        .view
        .layout_rects(0, 0)
        .into_iter()
        .find(|rect| rect.window == window)
        .expect("current window rect");
    let frame = ed.render();
    (0..count)
        .map(|offset| {
            frame
                .base()
                .row_text(rect.text.row + offset)
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Open file A in the current window and file B in a vertical split, then make
/// both windows diff windows.
fn diff_editor(a: &str, b: &str) -> (Editor, PathBuf, PathBuf) {
    let path_a = temp_path("a.txt");
    let path_b = temp_path("b.txt");
    std::fs::write(&path_a, a.as_bytes()).unwrap();
    std::fs::write(&path_b, b.as_bytes()).unwrap();
    let mut ed = Editor::new(Size::new(14, 60));
    ed.open_file(&path_a).unwrap();
    ed.messages.clear();
    ed.execute_ex(&format!("split {}", path_b.display()));
    ed.execute_ex("windo diffthis");
    (ed, path_a, path_b)
}

fn buffer_lines(ed: &Editor, buffer_name: &str) -> Vec<String> {
    let entry = ed
        .buffers
        .iter()
        .find(|(_, entry)| entry.short_name().ends_with(buffer_name))
        .map(|(_, entry)| entry)
        .expect("buffer");
    (1..=entry.text.line_count())
        .map(|lnum| String::from_utf8_lossy(entry.text.line(lnum).unwrap_or(b"")).into_owned())
        .collect()
}

fn focus(ed: &mut Editor, name: &str) {
    let id = ed
        .view
        .windows
        .iter()
        .find(|(_, window)| {
            ed.buffers
                .get(window.buffer)
                .is_some_and(|entry| entry.short_name().ends_with(name))
        })
        .map(|(id, _)| id)
        .expect("window for buffer");
    ed.view.set_current_window(id);
    ed.current_buffer = ed.view.current_window().unwrap().buffer;
}

#[test]
fn diff_blocks_filler_and_highlight() {
    let (mut ed, a, b) = diff_editor("a\nc\nd\n", "a\nb\nc\nd\n");
    // The A window is the current one after :windo.
    focus(&mut ed, "-a.txt");
    assert_eq!(eval(&mut ed, "diff_filler(2)"), "1");
    let lines = rows(&mut ed, 4);
    assert_eq!(lines[0], "a");
    assert!(lines[1].starts_with('-'), "{lines:?}");
    assert_eq!(lines[2], "c");
    // The added line in B is highlighted DiffAdd.
    focus(&mut ed, "-b.txt");
    let frame = ed.render();
    let cell = frame.base().cell(1, 0).unwrap();
    assert_eq!(frame.base().attr(cell).bg, Color::Indexed(2));
    assert_eq!(
        eval(&mut ed, "diff_hlID(2, 1)"),
        eval(&mut ed, "hlID('DiffAdd')")
    );
    focus(&mut ed, "-a.txt");
    assert_eq!(eval(&mut ed, "diff_hlID(2, 1)"), "0");
    assert_eq!(eval(&mut ed, "has('diff')"), "1");
    assert_eq!(eval(&mut ed, "exists('*diff_filler')"), "1");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn diff_do_and_dp_transfer_lines() {
    let (mut ed, a, b) = diff_editor("one\ntwo\nthree\n", "one\nTWO\nthree\n");
    focus(&mut ed, "-a.txt");
    ed.execute_ex("2");
    assert_eq!(
        eval(&mut ed, "diff_hlID(2, 1)"),
        eval(&mut ed, "hlID('DiffChange')")
    );
    // do: obtain the other buffer's version of the block.
    ed.execute_ex("normal! do");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(buffer_lines(&ed, "a.txt"), vec!["one", "TWO", "three"]);
    ed.execute_ex("undo");
    assert_eq!(buffer_lines(&ed, "a.txt"), vec!["one", "two", "three"]);
    // dp: put our lines into the other buffer.
    ed.execute_ex("normal! dp");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(buffer_lines(&ed, "b.txt"), vec!["one", "two", "three"]);
    // The buffers are identical now: no diff blocks or fillers remain.
    assert_eq!(eval(&mut ed, "diff_hlID(2, 1)"), "0");
    assert_eq!(eval(&mut ed, "diff_filler(2)"), "0");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn diff_get_and_put_ex_commands() {
    let (mut ed, a, b) = diff_editor("aa\nbb\ncc\n", "aa\nXX\ncc\nYY\n");
    focus(&mut ed, "-a.txt");
    ed.execute_ex("2");
    ed.execute_ex("diffget");
    assert_eq!(buffer_lines(&ed, "a.txt"), vec!["aa", "XX", "cc"]);
    ed.execute_ex("undo");
    ed.execute_ex("diffput");
    assert_eq!(buffer_lines(&ed, "b.txt"), vec!["aa", "bb", "cc", "YY"]);
    // The trailing added line in B is still a change for the A window.
    assert_eq!(eval(&mut ed, "diff_filler(3)"), "1");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn diff_jump_motions() {
    let (mut ed, a, b) = diff_editor(
        "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\n",
        "l1\nX2\nl3\nl4\nX5\nl6\nl7\nl8\nX9\n",
    );
    focus(&mut ed, "-a.txt");
    ed.execute_ex("1");
    ed.execute_ex("normal! ]c");
    assert_eq!(ed.cursor().lnum, 2);
    ed.execute_ex("normal! ]c");
    assert_eq!(ed.cursor().lnum, 5);
    ed.execute_ex("normal! [c");
    assert_eq!(ed.cursor().lnum, 2);
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn diff_fold_method() {
    let (mut ed, a, b) = diff_editor("l1\nl2\nl3\nl4\n", "l1\nX2\nX3\nl4\n");
    focus(&mut ed, "-a.txt");
    ed.execute_ex("set foldmethod=diff foldlevel=0");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "2");
    assert_eq!(eval(&mut ed, "foldlevel(2)"), "1");
    assert_eq!(eval(&mut ed, "foldlevel(1)"), "0");
    assert_eq!(eval(&mut ed, "foldlevel(4)"), "0");
    ed.execute_ex("set foldlevel=1");
    assert_eq!(eval(&mut ed, "foldclosed(2)"), "-1");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn diffoff_and_diffsplit() {
    let (mut ed, a, b) = diff_editor("one\ntwo\n", "one\nTWO\n");
    focus(&mut ed, "-a.txt");
    assert_eq!(
        eval(&mut ed, "diff_hlID(2, 1)"),
        eval(&mut ed, "hlID('DiffChange')")
    );
    ed.execute_ex("diffoff");
    assert_eq!(eval(&mut ed, "diff_hlID(2, 1)"), "0");
    assert_eq!(eval(&mut ed, "diff_filler(2)"), "0");
    // :diffsplit opens a split and turns diff mode on for both windows.
    let path = temp_path("other.txt");
    std::fs::write(&path, b"x\ny\n").unwrap();
    ed.execute_ex(&format!("diffsplit {}", path.display()));
    assert_eq!(eval(&mut ed, "&diff"), "1");
    focus(&mut ed, "-a.txt");
    assert_eq!(eval(&mut ed, "&diff"), "1");
    ed.execute_ex("diffoff!");
    focus(&mut ed, "-b.txt");
    assert_eq!(eval(&mut ed, "&diff"), "0");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
    let _ = std::fs::remove_file(path);
}

#[test]
fn diffopt_icase_and_iwhite() {
    let (mut ed, a, b) = diff_editor("Hello\nworld\n", "hello\nworld\n");
    focus(&mut ed, "-a.txt");
    assert_eq!(
        eval(&mut ed, "diff_hlID(1, 1)"),
        eval(&mut ed, "hlID('DiffChange')")
    );
    ed.execute_ex("set diffopt=internal,icase");
    assert_eq!(eval(&mut ed, "diff_hlID(1, 1)"), "0");
    ed.execute_ex("set diffopt=internal,iwhite");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}
