import pathlib
p = pathlib.Path("crates/rvim-headless/tests/golden.rs")
s = p.read_text()
s += '''

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

fn render_case(rows: u16, cols: u16, setup: impl FnOnce(&mut Editor)) -> String {
    let path = std::env::temp_dir().join(format!(
        "rvim-golden-{}-{}.txt",
        std::process::id(),
        rows * 1000 + cols
    ));
    std::fs::write(&path, b"hello\\nworld\\n").unwrap();
    let mut editor = Editor::new(Size::new(rows, cols));
    editor.open_file(&path).unwrap();
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
    let snapshot = render_case(24, 80, |editor| {
        editor.execute_ex("normal! 2G0l");
    });
    let frame = {
        let path = std::env::temp_dir().join(format!("rvim-golden-cursor-{}.txt", std::process::id()));
        std::fs::write(&path, b"hello\\nworld\\n").unwrap();
        let mut editor = Editor::new(Size::new(24, 80));
        editor.open_file(&path).unwrap();
        editor.execute_ex("set laststatus=2");
        editor.execute_ex("normal! 2G0l");
        editor.render()
    };
    let mut backend = HeadlessBackend::new(Size::new(24, 80));
    backend.init().unwrap();
    backend.present(&frame).unwrap();
    compare_golden("cursor_80x24.txt", &backend.snapshot_with_cursor());
    let _ = snapshot;
}
'''
p.write_text(s)
print("patched")
