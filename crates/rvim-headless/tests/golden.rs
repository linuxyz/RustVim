use rvim_editor::Editor;
use rvim_headless::HeadlessBackend;
use rvim_ui_api::{Size, UiBackend};

#[test]
fn headless_frame_matches_snapshot() {
    let path = std::env::temp_dir().join(format!("rvim-golden-{}.txt", std::process::id()));
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    let mut editor = Editor::new(Size::new(8, 40));
    editor.open_file(&path).unwrap();
    editor.execute_ex("set laststatus=2");
    let frame = editor.render();
    let mut backend = HeadlessBackend::new(Size::new(8, 40));
    backend.init().unwrap();
    backend.present(&frame).unwrap();
    let snapshot = backend.snapshot();
    assert!(snapshot.contains("hello"), "{snapshot}");
    assert!(snapshot.contains("world"), "{snapshot}");
    assert!(snapshot.contains("NORMAL"), "{snapshot}");
    assert!(frame.cursor.is_some());
    let _ = std::fs::remove_file(path);
}

fn compare_golden(name: &str, actual: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let path = dir.join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, actual).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("missing golden {}: {error}", path.display());
    });
    assert_eq!(actual, expected, "golden mismatch: {}", path.display());
}

/// Give the buffer a fixed, platform-independent name.
///
/// Golden snapshots include the statusline, which renders the buffer name, and
/// the backing file lives in the machine-specific temporary directory. Rename
/// the buffer so snapshots do not depend on `TMPDIR` or the host path.
fn stabilise_buffer_name(editor: &mut Editor, name: &str) {
    editor.execute_ex(&format!("file {name}"));
}

fn render_case(rows: u16, cols: u16, setup: impl FnOnce(&mut Editor)) -> String {
    render_case_content(rows, cols, b"hello\nworld\n", setup)
}

fn render_case_content(
    rows: u16,
    cols: u16,
    content: &[u8],
    setup: impl FnOnce(&mut Editor),
) -> String {
    let mut hash = 0u64;
    for byte in content {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as u64);
    }
    let path = std::env::temp_dir().join(format!("rvim-golden-{rows}x{cols}-{hash}.txt"));
    std::fs::write(&path, content).unwrap();
    let mut editor = Editor::new(Size::new(rows, cols));
    editor.swap_enabled = false;
    editor.open_file(&path).unwrap();
    stabilise_buffer_name(&mut editor, &format!("rvim-golden-{rows}x{cols}.txt"));
    editor.execute_ex("set laststatus=2");
    setup(&mut editor);
    let frame = editor.render();
    let mut backend = HeadlessBackend::new(Size::new(rows, cols));
    backend.init().unwrap();
    backend.present(&frame).unwrap();
    let snapshot = backend.snapshot();
    let _ = std::fs::remove_file(path);
    snapshot
}

#[test]
fn golden_basic_80x24() {
    let snapshot = render_case(24, 80, |_| {});
    compare_golden("basic_80x24.txt", &snapshot);
}

#[test]
fn golden_wide_120x40_with_number_and_wrap() {
    let snapshot = render_case(40, 120, |editor| {
        editor.execute_ex("set number wrap");
        editor.execute_ex("normal! gg");
    });
    compare_golden("wide_120x40.txt", &snapshot);
}

#[test]
fn golden_cursor_80x24() {
    let frame = {
        let path =
            std::env::temp_dir().join(format!("rvim-golden-cursor-{}.txt", std::process::id()));
        std::fs::write(&path, b"hello\nworld\n").unwrap();
        let mut editor = Editor::new(Size::new(24, 80));
        editor.swap_enabled = false;
        editor.open_file(&path).unwrap();
        stabilise_buffer_name(&mut editor, "rvim-golden-cursor.txt");
        editor.execute_ex("set laststatus=2");
        editor.execute_ex("normal! 2G0l");
        editor.render()
    };
    let mut backend = HeadlessBackend::new(Size::new(24, 80));
    backend.init().unwrap();
    backend.present(&frame).unwrap();
    compare_golden("cursor_80x24.txt", &backend.snapshot_with_cursor());
}

#[test]
fn golden_wide_characters_80x24() {
    let snapshot = render_case_content(24, 80, "你好世界\n".as_bytes(), |_| {});
    compare_golden("wide_chars_80x24.txt", &snapshot);
}

#[test]
fn golden_message_popup_80x24() {
    let snapshot = render_case_content(24, 80, b"hello\n", |editor| {
        editor.execute_ex("echowindow 'hello popup'");
    });
    compare_golden("popup_80x24.txt", &snapshot);
}

#[test]
fn golden_attribute_palette_80x24() {
    let path = std::env::temp_dir().join("rvim-golden-attrs.txt");
    std::fs::write(&path, b"TODO normal\n").unwrap();
    let mut editor = Editor::new(Size::new(24, 80));
    editor.swap_enabled = false;
    editor.open_file(&path).unwrap();
    stabilise_buffer_name(&mut editor, "rvim-golden-attrs.txt");
    editor.execute_ex("syntax match Todo /TODO/");
    editor.execute_ex("highlight Todo ctermfg=red cterm=bold");
    editor.execute_ex("set laststatus=2");
    let frame = editor.render();
    let mut backend = HeadlessBackend::new(Size::new(24, 80));
    backend.init().unwrap();
    backend.present(&frame).unwrap();
    compare_golden("attrs_80x24.txt", &backend.snapshot_with_attrs());
    let _ = std::fs::remove_file(path);
}
