import pathlib
p = pathlib.Path("crates/rvim-headless/tests/golden.rs")
s = p.read_text()
old = """fn render_case(rows: u16, cols: u16, setup: impl FnOnce(&mut Editor)) -> String {
    let path = std::env::temp_dir().join(format!("rvim-golden-{rows}x{cols}.txt"));
    std::fs::write(&path, b"hello\\nworld\\n").unwrap();
    let mut editor = Editor::new(Size::new(rows, cols));
    editor.swap_enabled = false;
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
}"""
new = """fn render_case(rows: u16, cols: u16, setup: impl FnOnce(&mut Editor)) -> String {
    render_case_content(rows, cols, b"hello\\nworld\\n", setup)
}

fn render_case_content(
    rows: u16,
    cols: u16,
    content: &[u8],
    setup: impl FnOnce(&mut Editor),
) -> String {
    let path = std::env::temp_dir().join(format!("rvim-golden-{rows}x{cols}.txt"));
    std::fs::write(&path, content).unwrap();
    let mut editor = Editor::new(Size::new(rows, cols));
    editor.swap_enabled = false;
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
}"""
assert old in s
s = s.replace(old, new, 1)
# append tests before final? Add at end
s += '''

#[test]
fn golden_wide_characters_80x24() {
    let snapshot = render_case_content(24, 80, "你好世界\\n".as_bytes(), |_| {});
    compare_golden("wide_chars_80x24.txt", &snapshot);
}

#[test]
fn golden_message_popup_80x24() {
    let snapshot = render_case_content(24, 80, b"hello\\n", |editor| {
        editor.execute_ex("echowindow 'hello popup'");
    });
    compare_golden("popup_80x24.txt", &snapshot);
}
'''
p.write_text(s)
print("patched")
