//! P1 completion gaps: command-line window details, interactive tag selection
//! and :psearch.

use std::path::PathBuf;

use rvim_editor::{Editor, Mode};
use rvim_ui_api::{Key, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, Size, UiEvent};

fn temp_path(name: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "rvim-p1-{}-{}-{unique}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

fn temp_dir(name: &str) -> PathBuf {
    let path = temp_path(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn key_char(ed: &mut Editor, ch: char) {
    ed.handle_key(KeyEvent::new(Key::Char(ch)));
}

fn press(ed: &mut Editor, key: Key) {
    ed.handle_key(KeyEvent::new(key));
}

fn buffer_name(ed: &Editor, buffer: rvim_types::BufferId) -> Option<PathBuf> {
    ed.buffers
        .get(buffer)
        .and_then(|entry| entry.text.meta.name.clone())
}

#[test]
fn command_line_window_has_type_column_and_insertmode() {
    let mut ed = Editor::new(Size::new(16, 50));
    ed.eval_viml("histadd('cmd', 'echo 1')").unwrap();
    ed.eval_viml("histadd('cmd', 'echo 2')").unwrap();
    key_char(&mut ed, 'q');
    key_char(&mut ed, ':');
    assert!(ed.cmdwin.is_some());
    let window = ed.view.current_window().expect("command-line window");
    assert_eq!(window.cmdwin_type, Some(':'));
    let frame = ed.render();
    let rows: Vec<String> = (0..3)
        .map(|row| frame.base().row_text(row).trim_end().to_string())
        .collect();
    assert!(rows[0].starts_with(":echo 1"), "{rows:?}");
    assert!(rows[1].starts_with(":echo 2"), "{rows:?}");
    // Closing the command-line window restores the previous buffer.
    ed.execute_ex("close");
    assert!(ed.cmdwin.is_none());
    assert!(!ed.messages.error, "{:?}", ed.messages.last);

    // CTRL-C continues on the command line with the text under the cursor.
    let mut ed = Editor::new(Size::new(16, 50));
    ed.eval_viml("histadd('cmd', 'echo 7')").unwrap();
    key_char(&mut ed, 'q');
    key_char(&mut ed, ':');
    ed.handle_key(KeyEvent::with_mods(Key::Char('c'), Mods::CTRL));
    assert!(ed.cmdwin.is_none());
    assert!(matches!(ed.mode, Mode::Command(_)), "{:?}", ed.mode);
    assert_eq!(ed.cmdline.text, "echo 7");

    // 'insertmode' starts the command-line window in Insert mode.
    let mut ed = Editor::new(Size::new(16, 50));
    ed.execute_ex("set insertmode");
    key_char(&mut ed, 'q');
    key_char(&mut ed, ':');
    assert!(matches!(ed.mode, Mode::Insert(_)), "{:?}", ed.mode);
    ed.execute_ex("close");
}

fn tag_editor() -> (Editor, PathBuf) {
    let root = temp_dir("tags-root");
    let one = root.join("one.txt");
    let two = root.join("two.txt");
    std::fs::write(&one, b"target one\n").unwrap();
    std::fs::write(&two, b"target two\n").unwrap();
    let tags = root.join("tags");
    std::fs::write(
        &tags,
        format!(
            "target\t{}\t1\ntarget\t{}\t1\n",
            one.display(),
            two.display()
        ),
    )
    .unwrap();
    std::fs::write(root.join("main.txt"), b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(14, 60));
    ed.open_file(&root.join("main.txt")).unwrap();
    ed.tag_index = rvim_editor::tags::TagIndex::load(&tags);
    (ed, root)
}

#[test]
fn interactive_tselect_and_ptselect() {
    let (mut ed, root) = tag_editor();
    let main = ed.current_buffer;
    ed.execute_ex("tselect target");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    // No jump happened yet: the prompt is waiting for a number.
    assert_eq!(ed.current_buffer, main);
    key_char(&mut ed, '2');
    press(&mut ed, Key::Enter);
    assert_eq!(
        buffer_name(&ed, ed.current_buffer).map(|path| path.file_name().unwrap().to_owned()),
        Some("two.txt".into())
    );
    // :ptselect shows the picked tag in the preview window only.
    ed.execute_ex(&format!("edit {}", root.join("main.txt").display()));
    let main = ed.current_buffer;
    ed.execute_ex("ptselect target");
    assert!(ed.preview_window.is_none());
    key_char(&mut ed, '1');
    press(&mut ed, Key::Enter);
    assert_eq!(ed.current_buffer, main);
    let preview = ed.preview_window.expect("preview window");
    let preview_buffer = ed.view.windows.get(&preview).unwrap().buffer;
    assert_eq!(
        buffer_name(&ed, preview_buffer).map(|path| path.file_name().unwrap().to_owned()),
        Some("one.txt".into())
    );
    // Escape cancels the prompt.
    ed.execute_ex("tselect target");
    press(&mut ed, Key::Esc);
    assert!(ed.tag_select_preview.is_none());
}

#[test]
fn find_searches_the_path_option() {
    let root = temp_dir("find-root");
    let target = root.join("found.txt");
    std::fs::write(&target, b"content\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&root.join("start.txt")).unwrap();
    ed.options
        .set_global(
            "path",
            rvim_editor::OptionValue::String(rvim_types::VimString::from(
                root.display().to_string(),
            )),
        )
        .unwrap();
    ed.execute_ex("find found.txt");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(buffer_name(&ed, ed.current_buffer), Some(target.clone()));
    ed.execute_ex("find missing.txt");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
    let message = ed.messages.last.clone().unwrap_or_default();
    assert!(message.contains("E345"), "{message:?}");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(root.join("start.txt"));
}

#[test]
fn psearch_finds_match_in_included_file() {
    let root = temp_dir("psearch-root");
    let main = root.join("main.c");
    let header = root.join("lib.h");
    std::fs::write(&main, b"#include \"lib.h\"\nint main(void) { return 0; }\n").unwrap();
    std::fs::write(&header, b"int popen(const char *, const char *);\n").unwrap();
    let mut ed = Editor::new(Size::new(14, 60));
    ed.open_file(&main).unwrap();
    ed.options
        .set_global(
            "include",
            rvim_editor::OptionValue::String(rvim_types::VimString::from("^\\s*#\\s*include")),
        )
        .unwrap();
    ed.execute_ex("psearch popen");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    let preview = ed.preview_window.expect("preview window");
    let preview_buffer = ed.view.windows.get(&preview).unwrap().buffer;
    assert_eq!(buffer_name(&ed, preview_buffer), Some(header.clone()));
    assert_eq!(ed.cursor().lnum, 1);
    // The non-bang form leaves the editing window on the original file.
    assert_eq!(buffer_name(&ed, ed.current_buffer), Some(main.clone()));
    // Listing all matches reports the file and line.
    ed.execute_ex("psearch! popen");
    let message = ed.messages.last.clone().unwrap_or_default();
    assert!(message.contains("lib.h"), "{message:?}");
    assert!(message.contains("popen"), "{message:?}");
    // A missing identifier is an error.
    ed.execute_ex("psearch no_such_identifier");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
    let _ = std::fs::remove_file(&main);
    let _ = std::fs::remove_file(&header);
}

#[test]
fn plugins_load_in_runtimepath_order_with_after_last() {
    let root = temp_dir("plugin-root");
    std::fs::create_dir_all(root.join("plugin")).unwrap();
    std::fs::create_dir_all(root.join("after/plugin")).unwrap();
    std::fs::write(
        root.join("plugin/a.vim"),
        b"if !exists('g:plugin_order') | let g:plugin_order = [] | endif\ncall add(g:plugin_order, 'a')\n",
    )
    .unwrap();
    std::fs::write(
        root.join("plugin/z.vim"),
        b"call add(g:plugin_order, 'z')\n",
    )
    .unwrap();
    std::fs::write(
        root.join("after/plugin/b.vim"),
        b"call add(g:plugin_order, 'after')\n",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.execute_ex(&format!(
        "set runtimepath={},{}",
        root.display(),
        root.join("after").display()
    ));
    ed.load_plugins().unwrap();
    assert_eq!(
        ed.eval_viml("string(g:plugin_order)")
            .unwrap()
            .echo_string(),
        "['a', 'z', 'after']"
    );
    // Loading twice does not source the same plugin again.
    ed.load_plugins().unwrap();
    assert_eq!(
        ed.eval_viml("string(g:plugin_order)")
            .unwrap()
            .echo_string(),
        "['a', 'z', 'after']"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn filetype_scripts_load_in_vim_order() {
    let root = temp_dir("ftplugin-root");
    for dir in ["ftplugin", "indent", "after/ftplugin", "after/indent"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(
        root.join("ftplugin/demo.vim"),
        b"if !exists('g:ft_order') | let g:ft_order = [] | endif\ncall add(g:ft_order, 'base')\n",
    )
    .unwrap();
    std::fs::write(
        root.join("ftplugin/demo_extra.vim"),
        b"call add(g:ft_order, 'extra')\n",
    )
    .unwrap();
    std::fs::write(
        root.join("indent/demo.vim"),
        b"call add(g:ft_order, 'indent')\n",
    )
    .unwrap();
    std::fs::write(
        root.join("after/ftplugin/demo.vim"),
        b"call add(g:ft_order, 'after')\n",
    )
    .unwrap();
    let file = root.join("file.demo");
    std::fs::write(&file, b"demo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.execute_ex(&format!(
        "set runtimepath={},{}",
        root.display(),
        root.join("after").display()
    ));
    ed.filetype_plugin = true;
    ed.filetype_indent = true;
    ed.open_file(&file).unwrap();
    ed.execute_ex("set filetype=demo");
    assert_eq!(
        ed.eval_viml("string(g:ft_order)").unwrap().echo_string(),
        "['base', 'extra', 'after', 'indent']"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn message_popup_click_dismisses_it() {
    let mut ed = Editor::new(Size::new(16, 50));
    ed.execute_ex("echowindow \"hello popup\"");
    let frame = ed.render();
    let region = frame
        .hit_regions
        .iter()
        .find(|region| matches!(region.action.kind, rvim_ui_api::ActionKind::PopupItem))
        .cloned()
        .expect("popup hit region");
    ed.handle_event(UiEvent::Mouse(MouseEvent {
        row: region.rect.row,
        col: region.rect.col,
        kind: MouseKind::Press,
        button: MouseButton::Left,
        mods: Default::default(),
    }));
    assert!(ed.messages.last.is_none());
}

#[test]
fn nested_autoload_script_is_loaded() {
    let root = temp_dir("autoload-root");
    std::fs::create_dir_all(root.join("autoload/foo")).unwrap();
    std::fs::write(
        root.join("autoload/foo/bar.vim"),
        b"function! foo#bar#Baz()\n  return 42\nendfunction\n",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.execute_ex(&format!("set runtimepath={}", root.display()));
    let value = ed.eval_viml("foo#bar#Baz()").unwrap().echo_string();
    assert_eq!(value, "42");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn known_but_unfired_events_can_be_defined() {
    let mut ed = Editor::new(Size::new(10, 50));
    // Runtime plugins (e.g. zipPlugin.vim) define autocmds for these events.
    ed.execute_ex("autocmd BufReadCmd * echo 1");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("autocmd BufWriteCmd * echo 2");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("autocmd FileReadCmd * echo 3");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("autocmd FileWriteCmd * echo 4");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    // Truly unknown events are still rejected.
    ed.execute_ex("autocmd NoSuchEvent * echo 5");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
}
