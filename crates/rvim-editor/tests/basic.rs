use std::path::PathBuf;

use rvim_editor::Editor;
use rvim_types::BufPos;
use rvim_ui_api::{Key, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, Size, UiEvent};

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rvim-test-{}-{}", std::process::id(), unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn unique() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn key_char(ed: &mut Editor, ch: char) {
    ed.handle_key(KeyEvent::new(Key::Char(ch)));
}

fn type_text(ed: &mut Editor, text: &str) {
    for ch in text.chars() {
        key_char(ed, ch);
    }
}

#[test]
fn opens_edits_writes_and_quits() {
    let path = temp_path("edit.txt");
    std::fs::write(&path, b"alpha\nbeta\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    assert_eq!(ed.line(1), Some(b"alpha".as_slice()));
    key_char(&mut ed, 'j');
    key_char(&mut ed, '$');
    key_char(&mut ed, 'x');
    assert_eq!(ed.line(2), Some(b"bet".as_slice()));
    ed.execute_ex("write");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(std::fs::read(&path).unwrap(), b"alpha\nbet\n");
    ed.execute_ex("quit");
    assert!(ed.should_quit);
}

#[test]
fn insert_mode_and_undo() {
    let path = temp_path("insert.txt");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'i');
    type_text(&mut ed, "X");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"Xone".as_slice()));
    key_char(&mut ed, 'u');
    assert_eq!(ed.line(1), Some(b"one".as_slice()));
    ed.handle_key(KeyEvent::with_mods(Key::Char('r'), Mods::CTRL));
    assert_eq!(ed.line(1), Some(b"Xone".as_slice()));
}

#[test]
fn normal_delete_undo_redo() {
    let path = temp_path("delete2.txt");
    std::fs::write(&path, b"first\nsecond\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'd');
    key_char(&mut ed, 'd');
    assert_eq!(ed.line(1), Some(b"second".as_slice()));
    key_char(&mut ed, 'u');
    assert_eq!(ed.line(1), Some(b"first".as_slice()));
    ed.handle_key(KeyEvent::with_mods(Key::Char('r'), Mods::CTRL));
    assert_eq!(ed.line(1), Some(b"second".as_slice()));
}

#[test]
fn options_update_view() {
    let mut ed = Editor::new(Size::new(12, 50));
    key_char(&mut ed, 'i');
    type_text(&mut ed, "X");
    ed.handle_key(KeyEvent::new(Key::Esc));
    ed.execute_ex("set number");
    assert!(!ed.messages.error);
    let frame = ed.render();
    let first = frame.base().row_text(0);
    assert!(first.starts_with("   1 X"), "{first:?}");
    ed.execute_ex("set nonumber");
    let frame = ed.render();
    for row in 0..frame.size.rows {
        eprintln!("row{row}={:?}", frame.base().row_text(row));
    }
    eprintln!("showcmd={:?}", ed.showcmd_text());
    assert!(frame.base().row_text(0).starts_with('X'));
}

#[test]
fn command_line_round_trip() {
    let path = temp_path("cmd.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, ':');
    type_text(&mut ed, "set tabstop=4");
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(
        ed.options.buffer_number(ed.current_buffer_id(), "tabstop"),
        4
    );
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
}

#[test]
fn replace_mode_overwrites_characters() {
    let path = temp_path("replace_mode.txt");
    std::fs::write(&path, b"abcdef\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'R');
    type_text(&mut ed, "XY");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"XYcdef".as_slice()));
}

#[test]
fn dot_repeats_delete_and_insert_changes() {
    let path = temp_path("dot.txt");
    std::fs::write(&path, b"one\ntwo\nthree\nfour\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'd');
    key_char(&mut ed, 'd');
    assert_eq!(ed.line(1), Some(b"two".as_slice()));
    key_char(&mut ed, '.');
    assert_eq!(ed.line(1), Some(b"three".as_slice()));
    ed.execute_ex(":1");
    key_char(&mut ed, 'i');
    type_text(&mut ed, "X");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"Xthree".as_slice()));
    ed.execute_ex(":2");
    key_char(&mut ed, '.');
    assert_eq!(ed.line(2), Some(b"Xfour".as_slice()));
}

#[test]
fn visual_block_delete_removes_columns() {
    let path = temp_path("block.txt");
    std::fs::write(&path, b"abcdef\nghijkl\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.handle_key(KeyEvent::with_mods(Key::Char('v'), Mods::CTRL));
    key_char(&mut ed, 'l');
    key_char(&mut ed, 'l');
    key_char(&mut ed, 'j');
    key_char(&mut ed, 'd');
    assert_eq!(ed.line(1), Some(b"def".as_slice()));
    assert_eq!(ed.line(2), Some(b"jkl".as_slice()));
}

#[test]
fn ex_silent_and_tab_modifiers() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.messages.info("before");
    ed.execute_ex("silent echo 'hidden'");
    assert_eq!(ed.messages.last.as_deref(), Some("before"));
    ed.execute_ex("silent! UnknownCommand");
    assert!(!ed.messages.error);
    assert_eq!(ed.view.tab_count(), 1);
    ed.execute_ex("tab set number");
    assert_eq!(ed.view.tab_count(), 2);
    let tabs = ed.view.tab_ids();
    ed.execute_ex("tablast");
    assert_eq!(ed.view.current_tab, tabs[1]);
    ed.execute_ex("tabfirst");
    assert_eq!(ed.view.current_tab, tabs[0]);
}

#[test]
fn shell_filter_and_shell_error() {
    let path = temp_path("filter.txt");
    std::fs::write(&path, b"b\na\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("1,2!sort");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.line(1), Some(b"a".as_slice()));
    assert_eq!(ed.line(2), Some(b"b".as_slice()));
    ed.execute_ex("!exit 3");
    assert_eq!(ed.eval_viml("v:shell_error").unwrap().echo_string(), "3");
}

#[test]
fn redir_captures_messages_into_variable() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("redir => g:captured");
    ed.execute_ex("echo 'hello'");
    ed.execute_ex("redir END");
    let captured = ed.eval_viml("g:captured").unwrap().echo_string();
    assert!(captured.contains("hello"), "{captured}");
}

#[test]
fn global_local_options_keep_both_values() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("set statusline=global");
    assert_eq!(ed.options.get_string("statusline"), "global");
    assert_eq!(
        ed.options
            .window_string(ed.current_window_id(), "statusline"),
        "global"
    );
    ed.execute_ex("setlocal statusline=local");
    assert_eq!(ed.options.get_string("statusline"), "global");
    assert_eq!(
        ed.options
            .window_string(ed.current_window_id(), "statusline"),
        "local"
    );
    ed.execute_ex("setglobal statusline=other");
    assert_eq!(ed.options.get_string("statusline"), "other");
    assert_eq!(
        ed.options
            .window_string(ed.current_window_id(), "statusline"),
        "local"
    );
}

#[test]
fn setfiletype_commands_update_buffer_filetype() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("setf rust");
    assert_eq!(
        ed.options.buffer_string(ed.current_buffer_id(), "filetype"),
        "rust"
    );
    ed.execute_ex("set filetype=");
    ed.execute_ex("setfiletype python");
    assert_eq!(
        ed.options.buffer_string(ed.current_buffer_id(), "filetype"),
        "python"
    );
    ed.execute_ex("setfiletype ignored");
    assert_eq!(
        ed.options.buffer_string(ed.current_buffer_id(), "filetype"),
        "python"
    );
}

#[test]
fn packloadall_and_packadd_source_plugins() {
    let root = temp_path("packroot");
    let start = root.join("pack/vendor/start/startpkg/plugin");
    std::fs::create_dir_all(&start).unwrap();
    std::fs::write(start.join("start.vim"), b"let g:start_pkg = 1\n").unwrap();
    let opt = root.join("pack/vendor/opt/optpkg/plugin");
    std::fs::create_dir_all(&opt).unwrap();
    std::fs::write(opt.join("opt.vim"), b"let g:opt_pkg = 1\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex(&format!("set packpath={}", root.display()));
    ed.execute_ex("packloadall");
    assert_eq!(ed.eval_viml("g:start_pkg").unwrap().echo_string(), "1");
    ed.execute_ex("packadd optpkg");
    assert_eq!(ed.eval_viml("g:opt_pkg").unwrap().echo_string(), "1");
}

#[test]
fn tabline_click_regions_dispatch_callbacks() {
    let mut ed = Editor::new(Size::new(12, 50));
    ed.source_viml(
        b"function! TabClick(info)\n  let g:tab_clicked = a:info.area\n  return 0\nendfunction\n",
        "tabclick.vim",
    )
    .unwrap();
    ed.execute_ex("set showtabline=2");
    ed.execute_ex("set tabline=%[TabClick]TAB%[]");
    let frame = ed.render();
    let click = frame
        .clicks
        .iter()
        .find(|click| click.function == "TabClick")
        .expect("tabline click region")
        .clone();
    let mouse = MouseEvent {
        row: click.rect.row,
        col: click.rect.col,
        kind: MouseKind::Press,
        button: MouseButton::Left,
        mods: Mods::empty(),
    };
    ed.handle_event(UiEvent::Mouse(mouse));
    assert_eq!(
        ed.eval_viml("g:tab_clicked").unwrap().echo_string(),
        "tabline"
    );
}

#[test]
fn insert_keyword_completion_cycles_candidates() {
    let path = temp_path("inscomp.txt");
    std::fs::write(&path, b"\nalpha beta\nalphabet\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'i');
    type_text(&mut ed, "al");
    ed.handle_key(KeyEvent::with_mods(Key::Char('n'), Mods::CTRL));
    assert_eq!(ed.line(1), Some(b"alpha".as_slice()));
    ed.handle_key(KeyEvent::with_mods(Key::Char('n'), Mods::CTRL));
    assert_eq!(ed.line(1), Some(b"alphabet".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn indent_and_shiftwidth_builtins() {
    let path = temp_path("indentfn.txt");
    std::fs::write(&path, b"    x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_eq!(ed.eval_viml("indent(1)").unwrap().echo_string(), "4");
    ed.execute_ex("set shiftwidth=3");
    assert_eq!(ed.eval_viml("shiftwidth()").unwrap().echo_string(), "3");
}

#[test]
fn normal_command_applies_to_range() {
    let path = temp_path("normalrange.txt");
    std::fs::write(&path, b"a\nb\nc\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("1,3normal I#");
    assert_eq!(ed.line(1), Some(b"#a".as_slice()));
    assert_eq!(ed.line(2), Some(b"#b".as_slice()));
    assert_eq!(ed.line(3), Some(b"#c".as_slice()));
}

#[test]
fn dragging_window_border_resizes_split() {
    use rvim_ui_api::ActionKind;
    let mut ed = Editor::new(Size::new(20, 60));
    ed.execute_ex("vsplit");
    let frame = ed.render();
    let border = frame
        .hit_regions
        .iter()
        .find(|region| region.action.kind == ActionKind::WindowBorder)
        .expect("window border region")
        .rect;
    let before: Vec<u16> = ed
        .view
        .windows
        .values()
        .map(|window| window.width)
        .collect();
    let press = MouseEvent {
        row: border.row + 1,
        col: border.col,
        kind: MouseKind::Press,
        button: MouseButton::Left,
        mods: Mods::empty(),
    };
    ed.handle_event(UiEvent::Mouse(press));
    let drag = MouseEvent {
        row: border.row + 1,
        col: border.col + 3,
        kind: MouseKind::Drag,
        button: MouseButton::Left,
        mods: Mods::empty(),
    };
    ed.handle_event(UiEvent::Mouse(drag));
    ed.handle_event(UiEvent::Mouse(MouseEvent {
        row: border.row + 1,
        col: border.col + 3,
        kind: MouseKind::Release,
        button: MouseButton::Left,
        mods: Mods::empty(),
    }));
    let _ = ed.render();
    let after: Vec<u16> = ed
        .view
        .windows
        .values()
        .map(|window| window.width)
        .collect();
    assert_ne!(before, after, "border drag should change split widths");
}

#[test]
fn history_builtins_round_trip() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("call histadd(':', 'one')");
    ed.execute_ex("call histadd(':', 'two')");
    assert_eq!(
        ed.eval_viml("histget(':', -1)").unwrap().echo_string(),
        "two"
    );
    assert_eq!(
        ed.eval_viml("histget(':', 1)").unwrap().echo_string(),
        "one"
    );
    assert!(
        ed.eval_viml("histnr(':')")
            .unwrap()
            .echo_string()
            .parse::<i64>()
            .unwrap()
            >= 2
    );
    ed.execute_ex("call histdel(':', -1)");
    assert_eq!(
        ed.eval_viml("histget(':', -1)").unwrap().echo_string(),
        "one"
    );
    ed.execute_ex("history");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
}

#[test]
fn completefunc_provider_supplies_insert_candidates() {
    let path = temp_path("cfu.txt");
    std::fs::write(&path, b"\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.source_viml(
        b"function! MyComplete(findstart, base)\n  if a:findstart\n    return col('.') - 3\n  endif\n  return ['foobar', 'foobaz']\nendfunction\nset completefunc=MyComplete\n",
        "cfu.vim",
    )
    .unwrap();
    key_char(&mut ed, 'i');
    type_text(&mut ed, "fo");
    ed.handle_key(KeyEvent::with_mods(Key::Char('x'), Mods::CTRL));
    ed.handle_key(KeyEvent::new(Key::Char('u')));
    assert_eq!(ed.line(1), Some(b"foobar".as_slice()));
    ed.handle_key(KeyEvent::with_mods(Key::Char('n'), Mods::CTRL));
    assert_eq!(ed.line(1), Some(b"foobaz".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn filter_modifier_suppresses_nonmatching_messages() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("filter /keep/ echo 'keep this'");
    assert_eq!(ed.messages.last.as_deref(), Some("keep this"));
    ed.messages.clear();
    ed.execute_ex("filter /drop/ echo 'keep this'");
    assert_eq!(ed.messages.last, None);
}

#[test]
fn mkview_and_loadview_restore_cursor() {
    let path = temp_path("viewfile.txt");
    std::fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let view = temp_path("view.vim");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.set_cursor(BufPos::new(2, 2));
    ed.execute_ex(&format!("mkview {}", view.display()));
    ed.set_cursor(BufPos::new(1, 0));
    ed.execute_ex(&format!("loadview {}", view.display()));
    assert_eq!(ed.cursor(), BufPos::new(2, 2));
}

#[test]
fn mksession_writes_sourceable_session() {
    let path = temp_path("session_file.txt");
    std::fs::write(&path, b"session\n").unwrap();
    let session = temp_path("Session.vim");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex(&format!("mksession {}", session.display()));
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("edit "), "{text}");
    ed.execute_ex("enew");
    ed.execute_ex(&format!("source {}", session.display()));
    assert_eq!(
        ed.current_buffer().and_then(|buffer| buffer.name()),
        Some(path.as_path())
    );
}

#[test]
fn normal_command_honors_mappings_unless_bang() {
    let path = temp_path("normalmap.txt");
    std::fs::write(&path, b"a\nb\nc\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("nnoremap x dd");
    ed.execute_ex("normal x");
    assert_eq!(ed.line(1), Some(b"b".as_slice()));
    ed.execute_ex("edit!");
    ed.execute_ex("normal! x");
    assert_eq!(ed.line(1), Some(b"".as_slice()));
}

#[test]
fn buffer_lifecycle_add_load_delete_wipe() {
    let a = temp_path("buf-a.txt");
    let b = temp_path("buf-b.txt");
    std::fs::write(&a, b"aaa\n").unwrap();
    std::fs::write(&b, b"bbb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("badd {}", b.display()));
    assert_eq!(ed.buffers.len(), 2);
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex(&format!("buffer {}", b.display()));
    assert_eq!(ed.line(1), Some(b"bbb".as_slice()));
    ed.execute_ex("bdelete");
    assert_eq!(ed.buffers.len(), 2);
    ed.execute_ex(&format!("bwipeout {}", b.display()));
    assert_eq!(ed.buffers.len(), 1);
}

#[test]
fn cindent_indents_after_control_statement() {
    let path = temp_path("cindent.txt");
    std::fs::write(&path, b"if (x)\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent shiftwidth=4 expandtab");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"    ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn bufhidden_wipes_or_unloads_abandoned_buffer() {
    let a = temp_path("hidden-a.txt");
    let b = temp_path("hidden-b.txt");
    std::fs::write(&a, b"aaa\n").unwrap();
    std::fs::write(&b, b"bbb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex("set bufhidden=wipe");
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex(&format!("buffer {}", b.display()));
    assert_eq!(ed.buffers.len(), 1);

    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex("set bufhidden=unload");
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex(&format!("buffer {}", b.display()));
    assert_eq!(ed.buffers.len(), 2);
    ed.execute_ex("ls");
    assert!(ed
        .messages
        .last
        .as_deref()
        .unwrap_or("")
        .contains("hidden-a.txt"));
}

#[test]
fn mapping_attributes_nowait_silent_and_unique() {
    let path = temp_path("mapattr.txt");
    std::fs::write(&path, b"abc\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("nnoremap <nowait> a l");
    ed.execute_ex("nnoremap ab h");
    key_char(&mut ed, 'a');
    assert_eq!(ed.cursor().col, 1);

    ed.execute_ex("nnoremap <silent> s :echo 'hidden'<CR>");
    ed.messages.info("before");
    key_char(&mut ed, 's');
    assert_eq!(ed.messages.last, None);

    ed.execute_ex("nnoremap <unique> q l");
    ed.execute_ex("nnoremap <unique> q l");
    assert!(ed.messages.error);
}

#[test]
fn buffer_local_mapping_does_not_leak_to_other_buffers() {
    let a = temp_path("mapbuf-a.txt");
    let b = temp_path("mapbuf-b.txt");
    std::fs::write(&a, b"abc\n").unwrap();
    std::fs::write(&b, b"abc\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex("nnoremap <buffer> x l");
    key_char(&mut ed, 'x');
    assert_eq!(ed.cursor().col, 1);
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex(&format!("buffer {}", b.display()));
    key_char(&mut ed, 'x');
    assert_eq!(ed.line(1), Some(b"bc".as_slice()));
}

#[test]
fn expr_mapping_evaluates_rhs() {
    let path = temp_path("exprmap.txt");
    std::fs::write(&path, b"abc\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex(r#"inoremap <expr> x "ZZ""#);
    key_char(&mut ed, 'i');
    key_char(&mut ed, 'x');
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"ZZabc".as_slice()));
}

#[test]
fn showbreak_prefixes_wrapped_lines() {
    let path = temp_path("showbreak.txt");
    std::fs::write(&path, b"one two three four five six seven eight nine\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 20));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set wrap linebreak showbreak=>> breakat= ");
    let frame = ed.render();
    let continuation = (0..frame.size.rows)
        .map(|row| frame.base().row_text(row))
        .find(|row| row.contains(">>"))
        .expect("wrapped continuation row");
    assert!(continuation.starts_with(">>"), "{continuation}");
}

#[test]
fn shell_bang_expands_file_modifiers() {
    let path = temp_path("shell-expand.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("!echo %:t");
    let message = ed.messages.last.clone().unwrap_or_default();
    assert!(message.contains("shell-expand.txt"), "{message}");
}

#[test]
fn multi_row_statusline_uses_newline_items() {
    let path = temp_path("multistatus.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set laststatus=2 statusline=%a%@%l statuslineopt=maxheight:2");
    let frame = ed.render();
    let rows: Vec<String> = (0..frame.size.rows)
        .map(|row| frame.base().row_text(row))
        .collect();
    let normal = rows.iter().position(|row| row.contains("NORMAL"));
    let line = rows.iter().position(|row| row.trim() == "1");
    assert!(normal.is_some(), "{rows:?}");
    assert!(line.is_some(), "{rows:?}");
    assert_ne!(normal, line);
}

#[test]
fn display_line_motions_gj_gk_g0_gdollar() {
    let path = temp_path("displaymotion.txt");
    std::fs::write(&path, b"one two three four five six seven\nsecond\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 20));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set wrap");
    key_char(&mut ed, 'g');
    key_char(&mut ed, 'j');
    assert_eq!(ed.cursor().lnum, 1);
    assert!(ed.cursor().col > 0);
    key_char(&mut ed, 'g');
    key_char(&mut ed, 'k');
    assert_eq!(ed.cursor().col, 0);
    key_char(&mut ed, 'g');
    key_char(&mut ed, '$');
    assert!(ed.cursor().col > 0);
    key_char(&mut ed, 'g');
    key_char(&mut ed, '0');
    assert_eq!(ed.cursor().col, 0);
}

#[test]
fn cindent_reindents_closing_brace_on_insert() {
    let path = temp_path("cinkeys.txt");
    std::fs::write(&path, b"\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'i');
    type_text(&mut ed, "if (x) {");
    ed.handle_key(KeyEvent::new(Key::Enter));
    key_char(&mut ed, '}');
    assert_eq!(ed.line(2), Some(b"}".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn global_multi_row_statusline_uses_bottom_rows() {
    let path = temp_path("globalstatus.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set laststatus=3 statusline=%a%@%l statuslineopt=maxheight:2");
    let frame = ed.render();
    let rows: Vec<String> = (0..frame.size.rows)
        .map(|row| frame.base().row_text(row))
        .collect();
    let normal = rows.iter().position(|row| row.contains("NORMAL"));
    let line = rows.iter().position(|row| row.trim() == "1");
    assert!(normal.is_some(), "{rows:?}");
    assert!(line.is_some(), "{rows:?}");
    assert_ne!(normal, line);
    assert!(normal.unwrap() < line.unwrap());
}

#[test]
fn lisp_indent_tracks_parentheses() {
    let path = temp_path("lispindent.txt");
    std::fs::write(&path, b"(foo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set lisp shiftwidth=2 expandtab");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"  ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn wildmenumode_reports_completion_popup() {
    let mut ed = Editor::new(Size::new(12, 40));
    assert_eq!(ed.eval_viml("wildmenumode()").unwrap().echo_string(), "0");
    key_char(&mut ed, ':');
    type_text(&mut ed, "e");
    ed.handle_key(KeyEvent::new(Key::Tab));
    assert_eq!(ed.eval_viml("wildmenumode()").unwrap().echo_string(), "1");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.eval_viml("wildmenumode()").unwrap().echo_string(), "0");
}

#[test]
fn smoothscroll_scrolls_by_display_chunk() {
    let path = temp_path("smoothscroll.txt");
    std::fs::write(&path, b"one two three four five six seven eight nine ten\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 20));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set wrap smoothscroll");
    ed.handle_key(KeyEvent::with_mods(Key::Char('e'), Mods::CTRL));
    assert!(ed.view.current_window().unwrap().viewport.skipcol > 0);
    let frame = ed.render();
    assert!(!frame.base().row_text(0).starts_with("one"));
    ed.handle_key(KeyEvent::with_mods(Key::Char('y'), Mods::CTRL));
    assert_eq!(ed.view.current_window().unwrap().viewport.skipcol, 0);
}

#[test]
fn cinoptions_scales_indent() {
    let path = temp_path("cinoptions.txt");
    std::fs::write(&path, b"\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=>2s");
    key_char(&mut ed, 'i');
    type_text(&mut ed, "if (x)");
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"        ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn smoothscroll_cursor_maps_to_skipped_chunk() {
    let path = temp_path("smoothcursor.txt");
    std::fs::write(&path, b"one two three four five six seven eight nine ten\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 20));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set wrap smoothscroll");
    ed.set_cursor(BufPos::new(1, 20));
    ed.view.current_window_mut().unwrap().viewport.skipcol = 20;
    let frame = ed.render();
    let cursor = frame.cursor.expect("cursor");
    assert_eq!(cursor.row, 0);
    assert_eq!(cursor.col, 0);
}

#[test]
fn viminfo_roundtrips_registers_history_and_marks() {
    use rvim_edit::{RegisterContent, RegisterKind, RegisterName};

    let path = temp_path("viminfo-data.txt");
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.open_file(&path).unwrap();
    ed.command_history.push_back("set number".to_string());
    ed.search_history.push_back("needle".to_string());
    ed.registers.set(
        RegisterName::Small('a'),
        RegisterContent::new(
            vec![rvim_types::VimString::from(b"hello".to_vec())],
            RegisterKind::Characterwise,
        ),
    );
    ed.current_buffer_mut()
        .unwrap()
        .marks
        .insert('A', BufPos::new(1, 2));
    let info = temp_path("state.viminfo");
    ed.execute_ex(&format!("wviminfo {}", info.display()));
    assert!(info.exists());

    let mut other = Editor::new(Size::new(12, 40));
    other.swap_enabled = false;
    other.open_file(&path).unwrap();
    other.execute_ex(&format!("rviminfo {}", info.display()));
    assert!(other
        .registers
        .get(RegisterName::Small('a'))
        .is_some_and(|content| content.text().as_bytes() == b"hello"));
    assert!(other
        .command_history
        .iter()
        .any(|item| item == "set number"));
    assert!(other.search_history.iter().any(|item| item == "needle"));
    assert!(other
        .oldfiles
        .iter()
        .any(|item| item == &path.display().to_string()));
    assert_eq!(
        other.current_buffer().unwrap().marks.get(&'A'),
        Some(&BufPos::new(1, 2))
    );
    other.execute_ex("oldfiles");
    assert!(other
        .messages
        .last
        .as_deref()
        .is_some_and(|message| message.contains("viminfo-data.txt")));
}

#[test]
fn default_viminfo_helpers_use_viminfofile() {
    use rvim_edit::{RegisterContent, RegisterKind, RegisterName};

    let path = temp_path("auto-viminfo.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("auto-state.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.open_file(&path).unwrap();
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.registers.set(
        RegisterName::Small('b'),
        RegisterContent::new(
            vec![rvim_types::VimString::from(b"auto".to_vec())],
            RegisterKind::Linewise,
        ),
    );
    ed.write_default_viminfo();
    assert!(info.exists());

    let mut other = Editor::new(Size::new(12, 40));
    other.swap_enabled = false;
    other.open_file(&path).unwrap();
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert!(other
        .registers
        .get(RegisterName::Small('b'))
        .is_some_and(|content| content.text().as_bytes() == b"auto"));
}

#[test]
fn persistent_undo_restores_previous_session() {
    let path = temp_path("persistent-undo.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let undo_dir = temp_path("undo-dir");
    std::fs::create_dir_all(&undo_dir).unwrap();

    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex("set undofile");
    ed.execute_ex(&format!("set undodir={}", undo_dir.display()));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'A');
    type_text(&mut ed, " one");
    ed.handle_key(KeyEvent::new(Key::Esc));
    key_char(&mut ed, 'A');
    type_text(&mut ed, " two");
    ed.handle_key(KeyEvent::new(Key::Esc));
    ed.execute_ex("write");
    ed.shutdown_swaps();

    let mut other = Editor::new(Size::new(12, 40));
    other.swap_enabled = false;
    other.execute_ex("set undofile");
    other.execute_ex(&format!("set undodir={}", undo_dir.display()));
    other.open_file(&path).unwrap();
    other.execute_ex("normal! u");
    assert_eq!(other.line(1), Some(b"hello one".as_slice()));
    other.execute_ex("normal! u");
    assert_eq!(other.line(1), Some(b"hello".as_slice()));
}

#[test]
fn viminfo_write_merges_existing_entries() {
    use rvim_edit::{RegisterContent, RegisterKind, RegisterName};

    let path = temp_path("viminfo-merge.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("merged.viminfo");

    let mut first = Editor::new(Size::new(12, 40));
    first.swap_enabled = false;
    first.execute_ex(&format!("set viminfofile={}", info.display()));
    first.open_file(&path).unwrap();
    first.command_history.push_back("one".to_string());
    first.registers.set(
        RegisterName::Small('a'),
        RegisterContent::new(
            vec![rvim_types::VimString::from(b"alpha".to_vec())],
            RegisterKind::Characterwise,
        ),
    );
    first.write_default_viminfo();

    let mut second = Editor::new(Size::new(12, 40));
    second.swap_enabled = false;
    second.execute_ex(&format!("set viminfofile={}", info.display()));
    second.open_file(&path).unwrap();
    second.command_history.push_back("two".to_string());
    second.registers.set(
        RegisterName::Small('b'),
        RegisterContent::new(
            vec![rvim_types::VimString::from(b"beta".to_vec())],
            RegisterKind::Characterwise,
        ),
    );
    second.write_default_viminfo();

    let mut third = Editor::new(Size::new(12, 40));
    third.swap_enabled = false;
    third.execute_ex(&format!("set viminfofile={}", info.display()));
    third.open_file(&path).unwrap();
    third.read_default_viminfo();
    assert!(third.command_history.iter().any(|item| item == "one"));
    assert!(third.command_history.iter().any(|item| item == "two"));
    assert!(third
        .registers
        .get(RegisterName::Small('a'))
        .is_some_and(|content| content.text().as_bytes() == b"alpha"));
    assert!(third
        .registers
        .get(RegisterName::Small('b'))
        .is_some_and(|content| content.text().as_bytes() == b"beta"));
}

#[test]
fn hidden_option_allows_abandoning_modified_buffer() {
    let path = temp_path("hidden-one.txt");
    let other = temp_path("hidden-two.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    std::fs::write(&other, b"world\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'A');
    type_text(&mut ed, " changed");
    ed.handle_key(KeyEvent::new(Key::Esc));
    ed.execute_ex(&format!("edit {}", other.display()));
    assert!(ed
        .messages
        .last
        .as_deref()
        .is_some_and(|message| message.contains("E37") || message.contains("No write")));

    ed.execute_ex("set hidden");
    ed.execute_ex(&format!("edit {}", other.display()));
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(other)
    );
}

#[test]
fn buffer_switch_respects_hidden_and_bufhidden() {
    let path = temp_path("switch-one.txt");
    let other = temp_path("switch-two.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    std::fs::write(&other, b"world\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'A');
    type_text(&mut ed, " changed");
    ed.handle_key(KeyEvent::new(Key::Esc));
    ed.execute_ex(&format!("badd {}", other.display()));
    ed.execute_ex(&format!("buffer {}", other.display()));
    assert!(ed
        .messages
        .last
        .as_deref()
        .is_some_and(|message| message.contains("E37") || message.contains("No write")));
    ed.execute_ex("set hidden");
    ed.execute_ex(&format!("buffer {}", other.display()));
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(other)
    );
}

#[test]
fn cinoptions_brace_column_zero_shift() {
    let path = temp_path("cino-caret.txt");
    std::fs::write(&path, b"{\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=^-2");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"  ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn cindent_case_body_is_shifted() {
    let path = temp_path("cino-case.txt");
    std::fs::write(&path, b"case 1:\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"    ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn cindent_reindents_case_label_on_colon() {
    let path = temp_path("cino-case-label.txt");
    std::fs::write(&path, b"switch (x) {\n\n}\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    ed.set_cursor(BufPos::new(2, 0));
    key_char(&mut ed, 'i');
    type_text(&mut ed, "case 1:");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(2), Some(b"    case 1:".as_slice()));
}

#[test]
fn cindent_places_opening_braces() {
    let path = temp_path("cino-open-brace.txt");
    std::fs::write(&path, b"if (x)\n\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions={2");
    ed.set_cursor(BufPos::new(2, 0));
    key_char(&mut ed, 'i');
    type_text(&mut ed, "{");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(2), Some(b"  {".as_slice()));

    let function = temp_path("cino-function-brace.txt");
    std::fs::write(&function, b"\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&function).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=f4");
    ed.set_cursor(BufPos::new(1, 0));
    key_char(&mut ed, 'i');
    type_text(&mut ed, "{");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"    {".as_slice()));
}

#[test]
fn cindent_places_jump_labels() {
    let path = temp_path("cino-label.txt");
    std::fs::write(&path, b"       \n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=L2");
    key_char(&mut ed, 'A');
    type_text(&mut ed, "LABEL:");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert!(ed
        .line(1)
        .is_some_and(|line| line.starts_with(b"     LABEL:")));
}

#[test]
fn cindent_continuation_lines() {
    let path = temp_path("cino-continuation.txt");
    std::fs::write(&path, b"foo(a,\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"        ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));

    let custom = temp_path("cino-continuation-custom.txt");
    std::fs::write(&custom, b"foo(a,\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&custom).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=(2");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"  ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn cindent_indents_comment_bodies() {
    let path = temp_path("cino-comment.txt");
    std::fs::write(&path, b"/*\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"   ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));

    let custom = temp_path("cino-comment-custom.txt");
    std::fs::write(&custom, b"/*\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&custom).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=c5");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"     ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn cindent_aligns_or_forces_comment_text() {
    let path = temp_path("cino-comment-text.txt");
    std::fs::write(&path, b"/*  foo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"    ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));

    let forced = temp_path("cino-comment-forced.txt");
    std::fs::write(&forced, b"/*  foo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&forced).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=c5,C1");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"     ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn mouse_click_maps_wrapped_display_rows() {
    use rvim_ui_api::{MouseButton, MouseEvent, MouseKind};
    let path = temp_path("mouse-wrap.txt");
    std::fs::write(&path, b"one two three four five six seven eight nine ten\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 20));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set wrap");
    let _ = ed.render();
    ed.handle_event(UiEvent::Mouse(MouseEvent {
        row: 1,
        col: 0,
        kind: MouseKind::Press,
        button: MouseButton::Left,
        mods: Mods::empty(),
    }));
    assert_eq!(ed.cursor().lnum, 1);
    assert_eq!(ed.cursor().col, 20);
}

#[test]
fn mouse_wheel_scrolls_display_chunks() {
    let path = temp_path("wheel-wrap.txt");
    std::fs::write(
        &path,
        b"one two three four five six seven eight nine ten eleven twelve\n",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(10, 20));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set wrap smoothscroll");
    let _ = ed.render();
    ed.handle_event(UiEvent::Mouse(MouseEvent {
        row: 0,
        col: 0,
        kind: MouseKind::Wheel,
        button: MouseButton::WheelDown,
        mods: Mods::empty(),
    }));
    assert!(ed.view.current_window().unwrap().viewport.skipcol > 0);
}

#[test]
fn cindent_places_cpp_scope_labels() {
    let path = temp_path("cino-scope.txt");
    std::fs::write(&path, b"class C {\n\n};\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    ed.set_cursor(BufPos::new(2, 0));
    key_char(&mut ed, 'i');
    type_text(&mut ed, "public:");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(2), Some(b"    public:".as_slice()));
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(3), Some(b"        ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn cindent_adds_namespace_and_linkage_indent() {
    let path = temp_path("cino-namespace.txt");
    std::fs::write(&path, b"namespace foo {\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=N2");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"      ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));

    let linkage = temp_path("cino-linkage.txt");
    std::fs::write(&linkage, b"extern \"C\" {\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&linkage).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=E2");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"      ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn cindent_indents_initializer_lists() {
    let path = temp_path("cino-init-base.txt");
    std::fs::write(&path, b"class MyClass :\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"    ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));

    let ctor = temp_path("cino-init-ctor.txt");
    std::fs::write(&ctor, b"MyClass::MyClass() :\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&ctor).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=i2");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"  ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn set_termcap_reports_terminal_info() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("set termcap");
    let message = ed.messages.last.clone().unwrap_or_default();
    assert!(message.contains("Terminal capabilities"), "{message}");
    assert!(message.contains("terminal:"), "{message}");
    assert!(message.contains("size:"), "{message}");
}

#[test]
fn default_bufhidden_unloads_unmodified_buffer() {
    let a = temp_path("default-unload-a.txt");
    let b = temp_path("default-unload-b.txt");
    std::fs::write(&a, b"aaa\n").unwrap();
    std::fs::write(&b, b"bbb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex(&format!("buffer {}", b.display()));
    assert!(ed
        .buffers
        .iter()
        .any(
            |(_, buffer)| buffer.name_string().ends_with("default-unload-a.txt") && !buffer.loaded
        ));
    ed.execute_ex(&format!("buffer {}", a.display()));
    assert_eq!(ed.line(1), Some(b"aaa".as_slice()));
}

#[test]
fn tabmove_reorders_tab_pages() {
    let path = temp_path("tabmove.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("tabnew");
    ed.execute_ex("tabnew");
    assert_eq!(ed.view.tab_count(), 3);
    let current = ed.view.current_tab;
    ed.execute_ex("tabmove 0");
    assert_eq!(ed.view.tab_ids().first().copied(), Some(current));
    ed.execute_ex("tabmove +1");
    assert_eq!(ed.view.tab_ids().get(1).copied(), Some(ed.view.current_tab));
    ed.execute_ex("tabmove");
    assert_eq!(ed.view.tab_ids().last().copied(), Some(ed.view.current_tab));
}

#[test]
fn tab_page_variables_and_queries() {
    let path = temp_path("tabvars.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("tabnew");
    ed.execute_ex("tabnew");
    assert_eq!(ed.eval_viml("tabpagenr()").unwrap().echo_string(), "3");
    assert_eq!(ed.eval_viml("tabpagenr('$')").unwrap().echo_string(), "3");
    assert_eq!(ed.eval_viml("tabpagewinnr(1)").unwrap().echo_string(), "1");
    assert_eq!(
        ed.eval_viml("len(tabpagebuflist(1))")
            .unwrap()
            .echo_string(),
        "1"
    );
    ed.eval_viml("settabvar(1, 'marker', 42)").unwrap();
    assert_eq!(
        ed.eval_viml("gettabvar(1, 'marker')")
            .unwrap()
            .echo_string(),
        "42"
    );
    ed.execute_ex("let t:direct = 7");
    assert_eq!(
        ed.eval_viml("gettabvar(3, 'direct')")
            .unwrap()
            .echo_string(),
        "7"
    );
    ed.execute_ex("tabfirst");
    assert_eq!(ed.eval_viml("t:marker").unwrap().echo_string(), "42");
}

#[test]
fn tab_lifecycle_events_fire() {
    let path = temp_path("tabevent.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.source_viml(
        b"autocmd TabNew * let g:tab_new = 1\nautocmd TabClosed * let g:tab_closed = 1\n",
        "tabevent.vim",
    )
    .unwrap();
    ed.execute_ex("tabnew");
    assert_eq!(
        ed.eval_viml("get(g:, 'tab_new', 0)").unwrap().echo_string(),
        "1"
    );
    ed.execute_ex("tabclose");
    assert_eq!(
        ed.eval_viml("get(g:, 'tab_closed', 0)")
            .unwrap()
            .echo_string(),
        "1"
    );
}

#[test]
fn tabclose_bang_ignores_modified_buffer() {
    let path = temp_path("tabclose-bang.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("tabnew");
    key_char(&mut ed, 'i');
    type_text(&mut ed, "changed");
    ed.handle_key(KeyEvent::new(Key::Esc));
    ed.execute_ex("tabclose");
    assert_eq!(ed.view.tab_count(), 2);
    ed.execute_ex("tabclose!");
    assert_eq!(ed.view.tab_count(), 1);
}

#[test]
fn command_line_builtins_edit_pending_command() {
    let mut ed = Editor::new(Size::new(12, 40));
    key_char(&mut ed, ':');
    type_text(&mut ed, "echo");
    assert_eq!(ed.eval_viml("getcmdline()").unwrap().echo_string(), "echo");
    assert_eq!(ed.eval_viml("getcmdtype()").unwrap().echo_string(), ":");
    assert_eq!(ed.eval_viml("getcmdpos()").unwrap().echo_string(), "5");
    ed.eval_viml("setcmdpos(1)").unwrap();
    assert_eq!(ed.eval_viml("getcmdpos()").unwrap().echo_string(), "1");
    ed.eval_viml("setcmdline('echo hi')").unwrap();
    assert_eq!(
        ed.eval_viml("getcmdline()").unwrap().echo_string(),
        "echo hi"
    );
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn tabonly_checks_modified_buffers_unless_bang() {
    let path = temp_path("tabonly.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("tabnew");
    key_char(&mut ed, 'i');
    type_text(&mut ed, "changed");
    ed.handle_key(KeyEvent::new(Key::Esc));
    ed.execute_ex("tabprevious");
    ed.execute_ex("tabonly");
    assert_eq!(ed.view.tab_count(), 2);
    assert!(ed
        .messages
        .last
        .as_deref()
        .is_some_and(|message| message.contains("E445") || message.contains("changes")));
    ed.execute_ex("tabonly!");
    assert_eq!(ed.view.tab_count(), 1);
}

#[test]
fn showcmd_displays_pending_count_and_operator() {
    let path = temp_path("showcmd.txt");
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, '2');
    key_char(&mut ed, 'd');
    let frame = ed.render();
    assert!(
        frame.base().row_text(0).contains("2d")
            || (0..frame.size.rows).any(|row| frame.base().row_text(row).contains("2d"))
    );
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn mksession_restores_tabs_current_tab_and_cursor() {
    let a = temp_path("session-a.txt");
    let b = temp_path("session-b.txt");
    std::fs::write(&a, b"aaa\nsecond\n").unwrap();
    std::fs::write(&b, b"bbb\nline2\n").unwrap();
    let session = temp_path("session.vim");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("tabnew {}", b.display()));
    ed.set_cursor(BufPos::new(2, 2));
    ed.execute_ex(&format!("mksession {}", session.display()));

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("source {}", session.display()));
    assert_eq!(other.view.tab_count(), 2);
    assert_eq!(
        other
            .view
            .tab_ids()
            .iter()
            .position(|tab| *tab == other.view.current_tab),
        Some(1)
    );
    assert_eq!(
        other
            .current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(b.clone())
    );
    assert_eq!(other.cursor().lnum, 2);
    assert_eq!(other.cursor().col, 2);
}

#[test]
fn skipcol_can_start_mid_chunk() {
    let path = temp_path("partial-skip.txt");
    std::fs::write(&path, b"one two three four five six seven eight nine\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 20));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set wrap");
    ed.view.current_window_mut().unwrap().viewport.skipcol = 5;
    ed.set_cursor(BufPos::new(1, 5));
    ed.view.current_window_mut().unwrap().viewport.skipcol = 5;
    let frame = ed.render();
    assert!(frame.base().row_text(0).starts_with("wo "));
    let cursor = frame.cursor.expect("cursor");
    assert_eq!(cursor.col, 0);
}

#[test]
fn buffer_first_last_and_modified_navigation() {
    let a = temp_path("bnav-a.txt");
    let b = temp_path("bnav-b.txt");
    let c = temp_path("bnav-c.txt");
    for path in [&a, &b, &c] {
        std::fs::write(path, b"line\n").unwrap();
    }
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex(&format!("buffer {}", b.display()));
    ed.execute_ex(&format!("badd {}", c.display()));
    ed.execute_ex(&format!("bufload {}", c.display()));
    ed.execute_ex(&format!("buffer {}", c.display()));
    ed.execute_ex("bfirst");
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(a.clone())
    );
    ed.execute_ex("blast");
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(c.clone())
    );
    ed.execute_ex(&format!("buffer {}", b.display()));
    key_char(&mut ed, 'i');
    type_text(&mut ed, "changed");
    ed.handle_key(KeyEvent::new(Key::Esc));
    ed.execute_ex("set hidden");
    ed.execute_ex(&format!("buffer {}", a.display()));
    ed.execute_ex("bmodified");
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(b.clone())
    );
}

#[test]
fn list_do_commands_iterate_targets() {
    let a = temp_path("listdo-a.txt");
    let b = temp_path("listdo-b.txt");
    std::fs::write(&a, b"aaa\n").unwrap();
    std::fs::write(&b, b"bbb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex("bufdo call setline(1, 'changed')");
    assert_eq!(ed.line(1), Some(b"changed".as_slice()));
    ed.execute_ex(&format!("buffer {}", b.display()));
    assert_eq!(ed.line(1), Some(b"changed".as_slice()));

    ed.execute_ex(&format!("buffer {}", a.display()));
    ed.execute_ex("split");
    ed.execute_ex("windo let g:count = get(g:, 'count', 0) + 1");
    assert_eq!(ed.eval_viml("g:count").unwrap().echo_string(), "2");
}

#[test]
fn showmode_reports_insert_replace_and_visual() {
    let path = temp_path("showmode.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'i');
    let frame = ed.render();
    assert!((0..frame.size.rows).any(|row| frame.base().row_text(row).contains("-- INSERT --")));
    ed.handle_key(KeyEvent::new(Key::Esc));
    key_char(&mut ed, 'R');
    let frame = ed.render();
    assert!((0..frame.size.rows).any(|row| frame.base().row_text(row).contains("-- REPLACE --")));
    ed.handle_key(KeyEvent::new(Key::Esc));
    key_char(&mut ed, 'v');
    let frame = ed.render();
    assert!((0..frame.size.rows).any(|row| frame.base().row_text(row).contains("-- VISUAL --")));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn ruler_shows_line_and_column_without_statusline() {
    let path = temp_path("ruler.txt");
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.set_cursor(BufPos::new(1, 2));
    let frame = ed.render();
    assert!((0..frame.size.rows).any(|row| frame.base().row_text(row).trim_end().ends_with("1,3")));
}

#[test]
fn rulerformat_customizes_ruler_text() {
    let path = temp_path("rulerformat.txt");
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set rulerformat=%l:%c");
    ed.set_cursor(BufPos::new(2, 2));
    let frame = ed.render();
    assert!((0..frame.size.rows).any(|row| frame.base().row_text(row).trim_end().ends_with("2:3")));
}

#[test]
fn viminfo_register_timestamp_keeps_newer_external_value() {
    use rvim_edit::{RegisterContent, RegisterKind, RegisterName};

    let path = temp_path("viminfo-ts.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("timestamped.viminfo");
    std::fs::write(
        &info,
        b"# RVim viminfo\n\"a\tCHAR\t0\n\tfuture\n|R,9999999999\n",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.registers.set(
        RegisterName::Small('a'),
        RegisterContent::new(
            vec![rvim_types::VimString::from(b"current".to_vec())],
            RegisterKind::Characterwise,
        ),
    );
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert!(other
        .registers
        .get(RegisterName::Small('a'))
        .is_some_and(|content| content.text().as_bytes() == b"future"));
}

#[test]
fn cindent_aligns_break_with_case_label() {
    let path = temp_path("cino-break.txt");
    std::fs::write(&path, b"switch (x) {\ncase 1:\n    foo();\n        \n}\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=b1");
    ed.set_cursor(BufPos::new(4, 8));
    key_char(&mut ed, 'A');
    type_text(&mut ed, "break;");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(4), Some(b"break;".as_slice()));
}

#[test]
fn verbose_prefix_executes_command() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("verbose set number");
    assert!(ed
        .options
        .window_bool(ed.view.current_window().unwrap().id.raw(), "number"));
}

#[test]
fn cindent_aligns_preprocessor_at_column_zero() {
    let path = temp_path("cino-hash.txt");
    std::fs::write(&path, b"    \n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'A');
    type_text(&mut ed, "#define X");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"#define X".as_slice()));
}

#[test]
fn viminfo_mark_timestamp_keeps_newer_external_position() {
    let path = temp_path("viminfo-markts.txt");
    std::fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let info = temp_path("mark-ts.viminfo");
    let raw = format!(
        "# RVim viminfo\n'a  99  0  {}\n|M,9999999999\n",
        path.display()
    );
    std::fs::write(&info, raw).unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.current_buffer_mut()
        .unwrap()
        .marks
        .insert('a', BufPos::new(1, 0));
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.swap_enabled = false;
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.open_file(&path).unwrap();
    other.read_default_viminfo();
    assert_eq!(
        other.current_buffer().unwrap().marks.get(&'a'),
        Some(&BufPos::new(99, 0))
    );
}

#[test]
fn viminfo_history_timestamp_orders_newer_external_entry_last() {
    let path = temp_path("viminfo-histts.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("hist-ts.viminfo");
    std::fs::write(&info, b"# RVim viminfo\n:old\n|H,C,9999999999\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.command_history.push_back("current".to_string());
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert_eq!(
        other.command_history.back().map(String::as_str),
        Some("old")
    );
    assert!(other.command_history.iter().any(|item| item == "current"));
}

#[test]
fn viminfo_persists_simple_global_variables() {
    let path = temp_path("viminfo-globals.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("globals.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:number_value = 42");
    ed.execute_ex("let g:string_value = 'text'");
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert_eq!(
        other.eval_viml("g:number_value").unwrap().echo_string(),
        "42"
    );
    assert_eq!(
        other.eval_viml("g:string_value").unwrap().echo_string(),
        "text"
    );
}

#[test]
fn viminfo_restores_buffer_list() {
    let a = temp_path("viminfo-buflist-a.txt");
    let b = temp_path("viminfo-buflist-b.txt");
    std::fs::write(&a, b"aaa\n").unwrap();
    std::fs::write(&b, b"bbb\n").unwrap();
    let info = temp_path("buflist.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.swap_enabled = false;
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    let names: Vec<String> = other
        .buffers
        .iter()
        .map(|(_, buffer)| buffer.name_string())
        .collect();
    assert!(names
        .iter()
        .any(|name| name.ends_with("viminfo-buflist-a.txt")));
    assert!(names
        .iter()
        .any(|name| name.ends_with("viminfo-buflist-b.txt")));
}

#[test]
fn viminfo_persists_list_and_dict_globals() {
    let path = temp_path("viminfo-complex.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("complex.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:list_value = [1, 2, 3]");
    ed.execute_ex("let g:dict_value = {'a': 1, 'b': 'x'}");
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert_eq!(
        other
            .eval_viml("string(g:list_value)")
            .unwrap()
            .echo_string(),
        "[1, 2, 3]"
    );
    assert_eq!(
        other.eval_viml("g:dict_value.a").unwrap().echo_string(),
        "1"
    );
    assert_eq!(
        other.eval_viml("g:dict_value.b").unwrap().echo_string(),
        "x"
    );
}

#[test]
fn viminfo_persists_blob_and_special_globals() {
    let path = temp_path("viminfo-blob.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("blob.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:blob_value = 0z1234");
    ed.execute_ex("let g:null_value = v:null");
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert!(other
        .eval_viml("string(g:blob_value)")
        .unwrap()
        .echo_string()
        .contains("0z1234"));
    assert_eq!(
        other
            .eval_viml("g:null_value is v:null")
            .unwrap()
            .echo_string(),
        "1"
    );
}

#[test]
fn viminfo_persists_named_funcref_global() {
    let path = temp_path("viminfo-funcref.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("funcref.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:fn_value = function('len')");
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert_eq!(
        other
            .eval_viml("g:fn_value([1, 2, 3])")
            .unwrap()
            .echo_string(),
        "3"
    );
}

#[test]
fn viminfo_persists_partial_funcref_with_args() {
    let path = temp_path("viminfo-partial.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("partial.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:partial_value = function('len', [[1, 2, 3]])");
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert_eq!(
        other.eval_viml("g:partial_value()").unwrap().echo_string(),
        "3"
    );
}

#[test]
fn cindent_aligns_after_inline_case_statement() {
    let path = temp_path("cino-l.txt");
    std::fs::write(&path, b"    case 1: a = b;\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=l1");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"    ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn ctrl_l_clears_messages_and_redraws() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.messages.info("hello");
    ed.should_redraw = false;
    ed.handle_key(KeyEvent::with_mods(Key::Char('l'), Mods::CTRL));
    assert!(ed.messages.last.is_none());
    assert!(ed.should_redraw);
}

#[test]
fn suspend_command_requests_suspend() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("suspend");
    assert!(ed.suspend_requested);
    ed.suspend_requested = false;
    ed.execute_ex("stop");
    assert!(ed.suspend_requested);
}

#[test]
fn window_and_buffer_variable_apis() {
    let a = temp_path("winbuf-a.txt");
    let b = temp_path("winbuf-b.txt");
    std::fs::write(&a, b"aaa\n").unwrap();
    std::fs::write(&b, b"bbb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("badd {}", b.display()));
    ed.execute_ex(&format!("bufload {}", b.display()));
    ed.execute_ex("split");
    ed.eval_viml("setwinvar(1, 'marker', 5)").unwrap();
    assert_eq!(
        ed.eval_viml("getwinvar(1, 'marker')")
            .unwrap()
            .echo_string(),
        "5"
    );
    ed.eval_viml("setbufvar(2, 'marker', 7)").unwrap();
    assert_eq!(
        ed.eval_viml("getbufvar(2, 'marker')")
            .unwrap()
            .echo_string(),
        "7"
    );
    assert_eq!(
        ed.eval_viml("getbufvar(2, 'missing', 9)")
            .unwrap()
            .echo_string(),
        "9"
    );
}

#[test]
fn viminfo_persists_simple_lambda_global() {
    let path = temp_path("viminfo-lambda.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let info = temp_path("lambda.viminfo");
    let mut ed = Editor::new(Size::new(12, 40));
    ed.swap_enabled = false;
    ed.execute_ex(&format!("set viminfofile={}", info.display()));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:lambda_value = {x -> x + 1}");
    ed.write_default_viminfo();

    let mut other = Editor::new(Size::new(12, 40));
    other.execute_ex(&format!("set viminfofile={}", info.display()));
    other.read_default_viminfo();
    assert_eq!(
        other.eval_viml("g:lambda_value(2)").unwrap().echo_string(),
        "3"
    );
}

#[test]
fn termcap_options_can_be_set_and_queried() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("set t_Co=42");
    ed.execute_ex("set t_Co?");
    assert!(ed
        .messages
        .last
        .as_deref()
        .is_some_and(|message| message.contains("t_Co=42")));
    ed.execute_ex("set t_Co&");
    ed.execute_ex("set t_Co?");
    assert!(ed
        .messages
        .last
        .as_deref()
        .is_some_and(|message| message.contains("t_Co=")));
    ed.execute_ex("set t_cl?");
    let clear = ed.messages.last.clone().unwrap_or_default();
    assert!(clear.starts_with("t_cl="), "{clear}");
    assert!(clear.len() > "t_cl=".len(), "{clear}");
    ed.execute_ex("set t_AF?");
    let setaf = ed.messages.last.clone().unwrap_or_default();
    assert!(setaf.starts_with("t_AF="), "{setaf}");
    assert!(setaf.len() > "t_AF=".len(), "{setaf}");
}

#[test]
fn cindent_keeps_js_object_labels_with_cino_j() {
    let path = temp_path("cino-J.txt");
    std::fs::write(&path, b"var bar = {\n    \n}\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4 cinoptions=J1");
    ed.set_cursor(BufPos::new(2, 4));
    key_char(&mut ed, 'A');
    type_text(&mut ed, "foo:");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(2), Some(b"    foo:".as_slice()));
}

#[test]
fn jump_and_change_lists_are_queryable() {
    let path = temp_path("jumplist.txt");
    std::fs::write(&path, b"one\ntwo\nthree\nfour\nfive\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("normal! G");
    ed.execute_ex("normal! gg");
    assert_ne!(
        ed.eval_viml("len(getjumplist()[0])").unwrap().echo_string(),
        "0"
    );
    key_char(&mut ed, 'A');
    type_text(&mut ed, " changed");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_ne!(
        ed.eval_viml("len(getchangelist()[0])")
            .unwrap()
            .echo_string(),
        "0"
    );
}

#[test]
fn buffer_and_window_info_apis() {
    let path = temp_path("getinfo.txt");
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("split");
    assert_eq!(
        ed.eval_viml("len(getbufinfo())").unwrap().echo_string(),
        "1"
    );
    assert_eq!(
        ed.eval_viml("getbufinfo()[0].bufnr").unwrap().echo_string(),
        "1"
    );
    assert_eq!(
        ed.eval_viml("len(getwininfo())").unwrap().echo_string(),
        "2"
    );
    assert_eq!(
        ed.eval_viml("getwininfo()[0].tabnr").unwrap().echo_string(),
        "1"
    );
    assert_ne!(
        ed.eval_viml("getwininfo()[0].height")
            .unwrap()
            .echo_string(),
        "0"
    );
    ed.execute_ex("let b:local = 3");
    assert_eq!(
        ed.eval_viml("len(getbufinfo()[0].windows)")
            .unwrap()
            .echo_string(),
        "2"
    );
    assert_eq!(
        ed.eval_viml("getbufinfo()[0].variables.local")
            .unwrap()
            .echo_string(),
        "3"
    );
    assert_ne!(
        ed.eval_viml("getwininfo()[0].winrow")
            .unwrap()
            .echo_string(),
        "0"
    );
    assert_ne!(
        ed.eval_viml("getwininfo()[0].wincol")
            .unwrap()
            .echo_string(),
        "0"
    );
}

#[test]
fn cindent_indents_knr_parameters() {
    let path = temp_path("cino-p.txt");
    std::fs::write(&path, b"func(a, b)\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cindent expandtab shiftwidth=4");
    key_char(&mut ed, 'A');
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert_eq!(ed.line(2), Some(b"    ".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Esc));
}

#[test]
fn gettabinfo_reports_tab_pages() {
    let path = temp_path("gettabinfo.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("tabnew");
    ed.execute_ex("tabnew");
    assert_eq!(
        ed.eval_viml("len(gettabinfo())").unwrap().echo_string(),
        "3"
    );
    assert_eq!(
        ed.eval_viml("gettabinfo()[0].tabnr").unwrap().echo_string(),
        "1"
    );
    assert_eq!(
        ed.eval_viml("len(gettabinfo()[0].windows)")
            .unwrap()
            .echo_string(),
        "1"
    );
}

#[test]
fn hit_enter_prompt_consumes_one_key() {
    let path = temp_path("hitenter.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.messages.info("x".repeat(100));
    let frame = ed.render();
    assert!(ed.hit_enter);
    assert!((0..frame.size.rows).any(|row| frame.base().row_text(row).contains("Press ENTER")));
    ed.handle_event(UiEvent::Key(KeyEvent::new(Key::Char('a'))));
    assert!(!ed.hit_enter);
    assert_eq!(ed.line(1), Some(b"hello".as_slice()));
}

#[test]
fn feature_matrix_matches_enabled_capabilities() {
    let mut ed = Editor::new(Size::new(12, 40));
    assert_eq!(ed.eval_viml("has('viminfo')").unwrap().echo_string(), "1");
    assert_eq!(
        ed.eval_viml("has('persistent_undo')")
            .unwrap()
            .echo_string(),
        "1"
    );
    assert_eq!(ed.eval_viml("has('terminal')").unwrap().echo_string(), "0");
    assert_eq!(ed.eval_viml("has('gui')").unwrap().echo_string(), "0");
    assert_eq!(
        ed.eval_viml("exists('+undofile')").unwrap().echo_string(),
        "1"
    );
}

#[test]
fn grep_uses_grepprg_and_grepformat() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("set grepprg=/bin/echo\\ file:1:needle");
    ed.execute_ex("grep pattern");
    assert_eq!(ed.quickfix.entries.len(), 1);
    assert_eq!(ed.quickfix.entries[0].lnum, 1);
    assert!(ed.quickfix.entries[0].file.ends_with("file"));
}

#[test]
fn make_uses_makeprg_and_arguments() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("set makeprg=/bin/echo\\ file:3:oops");
    ed.execute_ex("make target");
    assert_eq!(ed.quickfix.entries.len(), 1);
    assert_eq!(ed.quickfix.entries[0].lnum, 3);
}

#[test]
fn grepadd_appends_to_quickfix() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("set grepprg=/bin/echo\\ file:1:needle");
    ed.execute_ex("grep first");
    assert_eq!(ed.quickfix.entries.len(), 1);
    ed.execute_ex("grepadd second");
    assert_eq!(ed.quickfix.entries.len(), 2);
}

#[test]
fn quickfix_stack_colder_and_cnewer() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("set grepprg=/bin/echo\\ file:1:needle");
    ed.execute_ex("grep first");
    ed.execute_ex("grep second");
    assert!(ed.quickfix.title.contains("second"));
    ed.execute_ex("colder");
    assert!(ed.quickfix.title.contains("first"));
    ed.execute_ex("cnewer");
    assert!(ed.quickfix.title.contains("second"));
}

#[test]
fn quickfix_list_apis_roundtrip() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.eval_viml(
        "setqflist([{'filename': 'a.txt', 'lnum': 2, 'col': 3, 'text': 'oops', 'type': 'E'}])",
    )
    .unwrap();
    assert_eq!(
        ed.eval_viml("getqflist()[0].lnum").unwrap().echo_string(),
        "2"
    );
    assert_eq!(
        ed.eval_viml("getqflist()[0].text").unwrap().echo_string(),
        "oops"
    );
    assert_eq!(ed.eval_viml("len(getqflist())").unwrap().echo_string(), "1");
}

#[test]
fn window_id_apis() {
    let path = temp_path("winid.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    let first = ed.eval_viml("win_getid()").unwrap().echo_string();
    ed.execute_ex("split");
    let second = ed.eval_viml("win_getid()").unwrap().echo_string();
    assert_ne!(first, second);
    assert_ne!(
        ed.eval_viml(&format!("win_id2win({first})"))
            .unwrap()
            .echo_string(),
        "0"
    );
    assert_eq!(
        ed.eval_viml(&format!("win_gotoid({first})"))
            .unwrap()
            .echo_string(),
        "1"
    );
    assert_eq!(ed.eval_viml("win_getid()").unwrap().echo_string(), first);
    assert_eq!(
        ed.eval_viml("len(win_findbuf(1))").unwrap().echo_string(),
        "2"
    );
    assert_eq!(
        ed.eval_viml("win_getid(1)").unwrap().echo_string(),
        ed.eval_viml("win_getid(1, 1)").unwrap().echo_string()
    );
    assert_ne!(
        ed.eval_viml("win_getid(1)").unwrap().echo_string(),
        ed.eval_viml("win_getid(2)").unwrap().echo_string()
    );
}

#[test]
fn switchbuf_split_opens_quickfix_in_split() {
    let path = temp_path("switchbuf.txt");
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set switchbuf=split");
    let script = format!(
        "setqflist([{{'filename': '{}', 'lnum': 2, 'col': 1}}])",
        path.display()
    );
    ed.eval_viml(&script).unwrap();
    ed.execute_ex("cnext");
    assert_eq!(ed.view.windows.len(), 2);
    assert_eq!(ed.cursor().lnum, 2);
}

#[test]
fn messages_clear_removes_history() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.messages.info("old entry");
    ed.execute_ex("messages clear");
    assert_eq!(ed.messages.history.len(), 1);
    assert_eq!(ed.messages.last.as_deref(), Some("Messages cleared"));
    assert!(!ed.messages.history.iter().any(|item| item == "old entry"));
}

#[test]
fn feedkeys_getchar_and_input_use_queued_keys() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.eval_viml("feedkeys('a<CR><C-x>')").unwrap();
    assert_eq!(ed.eval_viml("getchar()").unwrap().echo_string(), "a");
    assert_eq!(ed.eval_viml("getchar()").unwrap().echo_string(), "13");
    assert_eq!(ed.eval_viml("getchar()").unwrap().echo_string(), "24");
    ed.eval_viml("feedkeys('hi<CR>')").unwrap();
    assert_eq!(
        ed.eval_viml("input('prompt: ')").unwrap().echo_string(),
        "hi"
    );
    ed.eval_viml("feedkeys('hidden<CR>')").unwrap();
    assert_eq!(
        ed.eval_viml("inputsecret('password: ')")
            .unwrap()
            .echo_string(),
        "hidden"
    );
    ed.eval_viml("feedkeys('ab')").unwrap();
    ed.eval_viml("inputsave()").unwrap();
    ed.eval_viml("feedkeys('cd')").unwrap();
    ed.eval_viml("inputrestore()").unwrap();
    assert_eq!(ed.eval_viml("getchar()").unwrap().echo_string(), "a");
    assert_eq!(ed.eval_viml("getchar()").unwrap().echo_string(), "b");
    assert_eq!(ed.eval_viml("getchar()").unwrap().echo_string(), "0");
}

#[test]
fn inputlist_and_inputdialog_use_queued_keys() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.eval_viml("feedkeys('2<CR>')").unwrap();
    assert_eq!(
        ed.eval_viml("inputlist(['alpha', 'beta'])")
            .unwrap()
            .echo_string(),
        "2"
    );
    ed.eval_viml("feedkeys('yes<CR>')").unwrap();
    assert_eq!(
        ed.eval_viml("inputdialog('question: ', 'no')")
            .unwrap()
            .echo_string(),
        "yes"
    );
    ed.eval_viml("feedkeys('<CR>')").unwrap();
    assert_eq!(
        ed.eval_viml("inputdialog('question: ', 'no')")
            .unwrap()
            .echo_string(),
        "no"
    );
}

#[test]
fn confirm_uses_queued_choice() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.eval_viml("feedkeys('2<CR>')").unwrap();
    assert_eq!(
        ed.eval_viml("confirm('question', \"&Yes\\n&No\", 1)")
            .unwrap()
            .echo_string(),
        "2"
    );
    ed.eval_viml("feedkeys('<CR>')").unwrap();
    assert_eq!(
        ed.eval_viml("confirm('question', \"&Yes\\n&No\", 1)")
            .unwrap()
            .echo_string(),
        "1"
    );
}

#[test]
fn redraw_variants_request_redraw() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.should_redraw = false;
    ed.execute_ex("redrawstatus");
    assert!(ed.should_redraw);
    ed.should_redraw = false;
    ed.execute_ex("redrawtabline");
    assert!(ed.should_redraw);
}

#[test]
fn position_apis_getcurpos_getpos_setpos() {
    let path = temp_path("positions.txt");
    std::fs::write(&path, b"one\ntwo\nthree\nfour\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.set_cursor(BufPos::new(2, 3));
    assert_eq!(ed.eval_viml("getcurpos()[1]").unwrap().echo_string(), "2");
    assert_eq!(ed.eval_viml("getcurpos()[2]").unwrap().echo_string(), "3");
    ed.eval_viml("setpos('.', [0, 3, 2, 0])").unwrap();
    assert_eq!(ed.cursor().lnum, 3);
    assert_eq!(ed.cursor().col, 1);
    ed.eval_viml("setpos(\"'a\", [0, 4, 2, 0])").unwrap();
    assert_eq!(
        ed.eval_viml("getpos(\"'a\")[1]").unwrap().echo_string(),
        "4"
    );
    assert_eq!(
        ed.eval_viml("getpos(\"'a\")[2]").unwrap().echo_string(),
        "2"
    );
}

#[test]
fn getmarklist_reports_buffer_marks() {
    let path = temp_path("marklist.txt");
    std::fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.set_cursor(BufPos::new(2, 1));
    ed.execute_ex("normal! ma");
    assert_eq!(
        ed.eval_viml("getmarklist()[0].mark").unwrap().echo_string(),
        "a"
    );
    assert_eq!(
        ed.eval_viml("getmarklist()[0].pos[1]")
            .unwrap()
            .echo_string(),
        "2"
    );
}

#[test]
fn tagfiles_and_taglist_read_tags_option() {
    let tags = temp_path("taglist.tags");
    std::fs::write(&tags, b"foo\tfile.txt\t1\nbar\tfile.txt\t2\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex(&format!("set tags={}", tags.display()));
    assert_eq!(ed.eval_viml("len(tagfiles())").unwrap().echo_string(), "1");
    assert_eq!(
        ed.eval_viml("len(taglist('foo'))").unwrap().echo_string(),
        "1"
    );
    assert_eq!(
        ed.eval_viml("taglist('foo')[0].name")
            .unwrap()
            .echo_string(),
        "foo"
    );
}

#[test]
fn tagstack_apis_roundtrip() {
    let path = temp_path("tagstack.txt");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    let buffer = ed.current_buffer;
    ed.tag_stack.push((buffer, BufPos::new(2, 0)));
    assert_eq!(
        ed.eval_viml("gettagstack().curidx").unwrap().echo_string(),
        "1"
    );
    ed.eval_viml("settagstack(0, {'items': []}, 'r')").unwrap();
    assert_eq!(
        ed.eval_viml("gettagstack().curidx").unwrap().echo_string(),
        "0"
    );
}

#[test]
fn getscriptinfo_lists_sourced_scripts() {
    let script = temp_path("scriptinfo.vim");
    std::fs::write(&script, b"let g:from_script = 1\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex(&format!("source {}", script.display()));
    assert_eq!(ed.eval_viml("g:from_script").unwrap().echo_string(), "1");
    assert_ne!(
        ed.eval_viml("len(getscriptinfo())").unwrap().echo_string(),
        "0"
    );
    assert_ne!(
        ed.eval_viml("getscriptinfo()[0].sid")
            .unwrap()
            .echo_string(),
        "0"
    );
}

#[test]
fn caddexpr_and_caddbuffer_append_entries() {
    let path = temp_path("cadd.txt");
    std::fs::write(&path, b"file:7:oops\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("caddexpr ['file:5:oops']");
    assert_eq!(ed.quickfix.entries.len(), 1);
    assert_eq!(ed.quickfix.entries[0].lnum, 5);
    ed.execute_ex("caddbuffer");
    assert_eq!(ed.quickfix.entries.len(), 2);
    assert_eq!(ed.quickfix.entries[1].lnum, 7);
}

#[test]
fn cgetexpr_and_cbuffer_replace_quickfix() {
    let path = temp_path("cget.txt");
    std::fs::write(&path, b"file:7:oops\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("cgetexpr ['file:9:oops']");
    assert_eq!(ed.quickfix.entries.len(), 1);
    assert_eq!(ed.quickfix.entries[0].lnum, 9);
    ed.execute_ex("cbuffer");
    assert_eq!(ed.quickfix.entries.len(), 1);
    assert_eq!(ed.quickfix.entries[0].lnum, 7);
}

#[test]
fn virtcol_accounts_for_tabs() {
    let path = temp_path("virtcol.txt");
    std::fs::write(&path, b"a\tb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.set_cursor(BufPos::new(1, 2));
    assert_eq!(ed.eval_viml("virtcol('.')").unwrap().echo_string(), "9");
    assert_eq!(
        ed.eval_viml("virtcol2col(0, 1, 9)").unwrap().echo_string(),
        "3"
    );
}

#[test]
fn screenchar_and_screenstring_read_frame() {
    let path = temp_path("screenchar.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_eq!(
        ed.eval_viml("screenstring(1, 1, 3)").unwrap().echo_string(),
        "hel"
    );
    assert_eq!(
        ed.eval_viml("screenchar(1, 1)").unwrap().echo_string(),
        "104"
    );
}

#[test]
fn winlayout_reports_split_tree() {
    let path = temp_path("winlayout.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("split");
    assert_eq!(ed.eval_viml("winlayout()[0]").unwrap().echo_string(), "row");
    assert_eq!(ed.eval_viml("len(winlayout())").unwrap().echo_string(), "3");
    assert_eq!(
        ed.eval_viml("winlayout()[1][0]").unwrap().echo_string(),
        "leaf"
    );
}

#[test]
fn screenpos_matches_window_origin() {
    let path = temp_path("screenpos.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    let row = ed
        .eval_viml("screenpos(win_getid(), 1, 1).row")
        .unwrap()
        .echo_string();
    let col = ed
        .eval_viml("screenpos(win_getid(), 1, 1).col")
        .unwrap()
        .echo_string();
    assert_eq!(
        row,
        ed.eval_viml("getwininfo()[0].winrow")
            .unwrap()
            .echo_string()
    );
    assert_eq!(
        col,
        ed.eval_viml("getwininfo()[0].wincol")
            .unwrap()
            .echo_string()
    );
}

#[test]
fn filesystem_viml_functions() {
    let directory = temp_path("fs-functions");
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("data.txt");
    std::fs::write(&file, b"hello").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    let file = file.display().to_string();
    let directory = directory.display().to_string();
    assert_eq!(
        ed.eval_viml(&format!("filewritable('{file}')"))
            .unwrap()
            .echo_string(),
        "1"
    );
    assert_eq!(
        ed.eval_viml(&format!("getfsize('{file}')"))
            .unwrap()
            .echo_string(),
        "5"
    );
    assert_eq!(
        ed.eval_viml(&format!("getftype('{file}')"))
            .unwrap()
            .echo_string(),
        "file"
    );
    assert_eq!(
        ed.eval_viml(&format!("getftype('{directory}')"))
            .unwrap()
            .echo_string(),
        "dir"
    );
    assert_eq!(
        ed.eval_viml(&format!("readdir('{directory}')"))
            .unwrap()
            .echo_string(),
        "['data.txt']"
    );
}

#[test]
fn glob_and_globpath_expand_wildcards() {
    let directory = temp_path("glob-functions");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("a.txt"), b"a").unwrap();
    std::fs::write(directory.join("b.txt"), b"b").unwrap();
    std::fs::write(directory.join("c.md"), b"c").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    let directory = directory.display().to_string();
    assert_eq!(
        ed.eval_viml(&format!("len(glob('{directory}/*.txt', 0, 1))"))
            .unwrap()
            .echo_string(),
        "2"
    );
    assert_eq!(
        ed.eval_viml(&format!("len(globpath('{directory}', '*.txt', 0, 1))"))
            .unwrap()
            .echo_string(),
        "2"
    );
}

#[test]
fn environment_functions_roundtrip() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.eval_viml("setenv('RVIM_TEST_ENV', 'yes')").unwrap();
    assert_eq!(
        ed.eval_viml("getenv('RVIM_TEST_ENV')")
            .unwrap()
            .echo_string(),
        "yes"
    );
    assert_eq!(
        ed.eval_viml("environ()['RVIM_TEST_ENV']")
            .unwrap()
            .echo_string(),
        "yes"
    );
}

#[test]
fn tempname_mkdir_and_delete() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.execute_ex("let g:t = tempname()");
    assert_eq!(ed.eval_viml("mkdir(g:t)").unwrap().echo_string(), "0");
    assert_eq!(ed.eval_viml("isdirectory(g:t)").unwrap().echo_string(), "1");
    assert_eq!(ed.eval_viml("delete(g:t, 'd')").unwrap().echo_string(), "0");
    assert_eq!(ed.eval_viml("isdirectory(g:t)").unwrap().echo_string(), "0");
}

#[test]
fn path_modifier_functions() {
    let path = temp_path("path-functions.txt");
    std::fs::write(&path, b"x").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    let text = path.display().to_string();
    assert_eq!(
        ed.eval_viml(&format!("fnamemodify('{text}', ':t')"))
            .unwrap()
            .echo_string(),
        "path-functions.txt"
    );
    assert_eq!(
        ed.eval_viml(&format!("fnamemodify('{text}', ':e')"))
            .unwrap()
            .echo_string(),
        "txt"
    );
    assert!(ed
        .eval_viml(&format!("resolve('{text}')"))
        .unwrap()
        .echo_string()
        .ends_with("path-functions.txt"));
    assert_eq!(
        ed.eval_viml("simplify('/a/./b/../c')")
            .unwrap()
            .echo_string(),
        "/a/c"
    );
}

#[test]
fn sball_opens_split_windows() {
    let first = temp_path("sball-a.txt");
    let second = temp_path("sball-b.txt");
    std::fs::write(&first, b"aaa\n").unwrap();
    std::fs::write(&second, b"bbb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&first).unwrap();
    ed.execute_ex(&format!("badd {}", second.display()));
    ed.execute_ex(&format!("bufload {}", second.display()));
    ed.execute_ex("sball");
    assert_eq!(ed.view.windows.len(), 2);
}

#[test]
fn tabwinvar_apis_across_tabs() {
    let path = temp_path("tabwinvar.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("tabnew");
    ed.eval_viml("settabwinvar(1, 1, 'marker', 7)").unwrap();
    assert_eq!(
        ed.eval_viml("gettabwinvar(1, 1, 'marker')")
            .unwrap()
            .echo_string(),
        "7"
    );
    assert_eq!(
        ed.eval_viml("gettabwinvar(1, 1, 'missing', 9)")
            .unwrap()
            .echo_string(),
        "9"
    );
    assert_eq!(ed.eval_viml("tabpagewinnr(1)").unwrap().echo_string(), "1");
    assert_eq!(
        ed.eval_viml("tabpagewinnr(1, '$')").unwrap().echo_string(),
        "1"
    );
}

#[test]
fn buffer_line_functions() {
    let path = temp_path("bufline.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_eq!(
        ed.eval_viml("getbufoneline(1, 1)").unwrap().echo_string(),
        "hello"
    );
    ed.eval_viml("append(1, ['a', 'b'])").unwrap();
    assert_eq!(
        ed.eval_viml("getbufline(1, 2, 3)[1]")
            .unwrap()
            .echo_string(),
        "b"
    );
    ed.eval_viml("setbufline(1, 1, 'changed')").unwrap();
    assert_eq!(
        ed.eval_viml("getbufline(1, 1, 1)[0]")
            .unwrap()
            .echo_string(),
        "changed"
    );
    ed.eval_viml("deletebufline(1, 2)").unwrap();
    assert_eq!(
        ed.eval_viml("getbufoneline(1, 2)").unwrap().echo_string(),
        "b"
    );
}

#[test]
fn getline_and_setline_roundtrip() {
    let path = temp_path("getline.txt");
    std::fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_eq!(ed.eval_viml("getline(2)").unwrap().echo_string(), "two");
    assert_eq!(
        ed.eval_viml("getline(1, 2)[1]").unwrap().echo_string(),
        "two"
    );
    ed.eval_viml("setline(2, 'changed')").unwrap();
    assert_eq!(ed.eval_viml("getline(2)").unwrap().echo_string(), "changed");
    ed.execute_ex("normal! G");
    assert_eq!(ed.cursor().lnum, 3);
    ed.eval_viml("setline(3, 'current')").unwrap();
    assert_eq!(ed.eval_viml("getline(3)").unwrap().echo_string(), "current");
}

#[test]
fn buffer_query_functions() {
    let first = temp_path("buffunc-a.txt");
    let second = temp_path("buffunc-b.txt");
    std::fs::write(&first, b"aaa\n").unwrap();
    std::fs::write(&second, b"bbb\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&first).unwrap();
    assert_eq!(ed.eval_viml("bufnr('%')").unwrap().echo_string(), "1");
    assert!(ed
        .eval_viml("bufname(1)")
        .unwrap()
        .echo_string()
        .ends_with("buffunc-a.txt"));
    assert_eq!(ed.eval_viml("bufexists(1)").unwrap().echo_string(), "1");
    assert_eq!(ed.eval_viml("bufloaded(1)").unwrap().echo_string(), "1");
    assert_eq!(ed.eval_viml("bufwinnr(1)").unwrap().echo_string(), "1");
    assert_eq!(
        ed.eval_viml(&format!("bufadd('{}')", second.display()))
            .unwrap()
            .echo_string(),
        "2"
    );
    assert_eq!(ed.eval_viml("bufexists(2)").unwrap().echo_string(), "1");
}

#[test]
fn window_query_functions() {
    let path = temp_path("winfuncs.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_eq!(ed.eval_viml("winnr()").unwrap().echo_string(), "1");
    assert_eq!(ed.eval_viml("winbufnr(1)").unwrap().echo_string(), "1");
    assert_ne!(ed.eval_viml("winheight(1)").unwrap().echo_string(), "0");
    assert_ne!(ed.eval_viml("winwidth(1)").unwrap().echo_string(), "0");
    assert_eq!(ed.eval_viml("winline()").unwrap().echo_string(), "1");
    assert_eq!(ed.eval_viml("wincol()").unwrap().echo_string(), "1");
    ed.execute_ex("split");
    assert_eq!(ed.eval_viml("winnr('$')").unwrap().echo_string(), "2");
}

#[test]
fn winsaveview_and_winrestview_roundtrip() {
    let path = temp_path("winview.txt");
    std::fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.set_cursor(BufPos::new(2, 1));
    ed.execute_ex("let g:view = winsaveview()");
    ed.set_cursor(BufPos::new(1, 0));
    ed.execute_ex("call winrestview(g:view)");
    assert_eq!(ed.cursor().lnum, 2);
    assert_eq!(ed.cursor().col, 1);
}

#[test]
fn getwinpos_reports_window_geometry() {
    let path = temp_path("getwinpos.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_ne!(ed.eval_viml("getwinposx()").unwrap().echo_string(), "0");
    assert_ne!(ed.eval_viml("getwinposy()").unwrap().echo_string(), "0");
    assert_eq!(ed.eval_viml("len(getwinpos())").unwrap().echo_string(), "4");
    assert_ne!(ed.eval_viml("getwinpos()[2]").unwrap().echo_string(), "0");
}

#[test]
fn register_functions_roundtrip() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.eval_viml("setreg('a', 'hello')").unwrap();
    assert_eq!(ed.eval_viml("getreg('a')").unwrap().echo_string(), "hello");
    assert_eq!(ed.eval_viml("getregtype('a')").unwrap().echo_string(), "v");
    ed.eval_viml("setreg('b', \"line1\\nline2\", 'V')").unwrap();
    assert_eq!(ed.eval_viml("getregtype('b')").unwrap().echo_string(), "V");
    assert_eq!(
        ed.eval_viml("len(getreg('b', 1))").unwrap().echo_string(),
        "2"
    );
}

#[test]
fn completion_functions_populate_menu() {
    let mut ed = Editor::new(Size::new(12, 40));
    ed.eval_viml("complete(1, ['alpha', 'beta'])").unwrap();
    assert_eq!(ed.eval_viml("pumvisible()").unwrap().echo_string(), "1");
    ed.eval_viml("complete_add(['gamma'])").unwrap();
    assert_eq!(
        ed.eval_viml("len(complete_info()['items'])")
            .unwrap()
            .echo_string(),
        "3"
    );
    assert_eq!(ed.eval_viml("complete_check()").unwrap().echo_string(), "0");
}

#[test]
fn getmousepos_reports_last_mouse_event() {
    let path = temp_path("mousepos.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.handle_event(UiEvent::Mouse(MouseEvent {
        row: 1,
        col: 1,
        kind: MouseKind::Press,
        button: MouseButton::Left,
        mods: Mods::empty(),
    }));
    assert_eq!(
        ed.eval_viml("getmousepos().screenrow")
            .unwrap()
            .echo_string(),
        "2"
    );
    assert_eq!(
        ed.eval_viml("getmousepos().screencol")
            .unwrap()
            .echo_string(),
        "2"
    );
    assert_ne!(
        ed.eval_viml("getmousepos().winid").unwrap().echo_string(),
        "0"
    );
}

#[test]
fn charpos_functions_use_character_indices() {
    let path = temp_path("charpos.txt");
    std::fs::write(&path, "éx\n".as_bytes()).unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.set_cursor(BufPos::new(1, 2));
    assert_eq!(
        ed.eval_viml("getcharpos('.')[2]").unwrap().echo_string(),
        "2"
    );
    assert_eq!(ed.eval_viml("getpos('.')[2]").unwrap().echo_string(), "3");
    ed.eval_viml("setcharpos('.', [0, 1, 2, 0])").unwrap();
    assert_eq!(ed.cursor().col, 2);
    ed.eval_viml("setcursorcharpos([0, 1, 1, 0])").unwrap();
    assert_eq!(ed.cursor().col, 0);
}

#[test]
fn byte_and_character_index_functions() {
    let mut ed = Editor::new(Size::new(12, 40));
    assert_eq!(ed.eval_viml("byteidx('éx', 1)").unwrap().echo_string(), "2");
    assert_eq!(ed.eval_viml("byteidx('éx', 2)").unwrap().echo_string(), "3");
    assert_eq!(ed.eval_viml("charidx('éx', 2)").unwrap().echo_string(), "1");
    assert_eq!(
        ed.eval_viml("charidx('éx', 1)").unwrap().echo_string(),
        "-1"
    );
}

#[test]
fn string_width_functions() {
    let mut ed = Editor::new(Size::new(12, 40));
    assert_eq!(ed.eval_viml("strwidth('é')").unwrap().echo_string(), "1");
    assert_eq!(
        ed.eval_viml("strdisplaywidth(\"\\t\")")
            .unwrap()
            .echo_string(),
        "8"
    );
    assert_eq!(
        ed.eval_viml("strdisplaywidth(\"a\\tb\")")
            .unwrap()
            .echo_string(),
        "9"
    );
}

#[test]
fn matchlist_and_matchstrpos() {
    let mut ed = Editor::new(Size::new(12, 40));
    assert_eq!(
        ed.eval_viml("matchstrpos('hello5', '[0-9]')[0]")
            .unwrap()
            .echo_string(),
        "5"
    );
    assert_eq!(
        ed.eval_viml("matchstrpos('hello5', '[0-9]')[1]")
            .unwrap()
            .echo_string(),
        "5"
    );
    assert_eq!(
        ed.eval_viml("len(matchlist('abc123', '\\([a-z]*\\)\\([0-9]*\\)'))")
            .unwrap()
            .echo_string(),
        "3"
    );
    assert_eq!(
        ed.eval_viml("matchlist('abc123', '\\([a-z]*\\)\\([0-9]*\\)')[1]")
            .unwrap()
            .echo_string(),
        "abc"
    );
}

#[test]
fn escaping_functions() {
    let mut ed = Editor::new(Size::new(12, 40));
    assert_eq!(
        ed.eval_viml("escape('a.b', '.')").unwrap().echo_string(),
        "a\\.b"
    );
    assert_eq!(
        ed.eval_viml("shellescape('a b')").unwrap().echo_string(),
        "'a b'"
    );
    assert_eq!(
        ed.eval_viml("fnameescape('a b')").unwrap().echo_string(),
        "a\\ b"
    );
}

#[test]
fn expand_handles_words_environment_and_home() {
    let path = temp_path("expand.txt");
    std::fs::write(&path, b"hello world\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("normal! w");
    assert_eq!(
        ed.eval_viml("expand('<cword>')").unwrap().echo_string(),
        "world"
    );
    ed.eval_viml("setenv('RVIM_EXPAND_TEST', 'yes')").unwrap();
    assert_eq!(
        ed.eval_viml("expand('$RVIM_EXPAND_TEST')")
            .unwrap()
            .echo_string(),
        "yes"
    );
    assert!(ed
        .eval_viml("expand('~/expand-test')")
        .unwrap()
        .echo_string()
        .ends_with("/expand-test"));
}

#[test]
fn tr_translates_characters() {
    let mut ed = Editor::new(Size::new(12, 40));
    assert_eq!(
        ed.eval_viml("tr('abc', 'ab', 'xy')").unwrap().echo_string(),
        "xyc"
    );
    assert_eq!(
        ed.eval_viml("tr('abc', 'abc', 'xy')")
            .unwrap()
            .echo_string(),
        "xy"
    );
}

#[test]
fn process_identity_functions() {
    let mut ed = Editor::new(Size::new(12, 40));
    let pid = ed.eval_viml("getpid()").unwrap().echo_string();
    assert!(pid.parse::<u32>().unwrap_or(0) > 0);
    let _ = ed.eval_viml("hostname()").unwrap();
}

#[test]
fn yank_and_put() {
    let mut ed = Editor::new(Size::new(12, 50));
    let path = temp_path("yank.txt");
    std::fs::write(&path, b"hello\nworld\n").unwrap();
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'y');
    key_char(&mut ed, 'y');
    key_char(&mut ed, 'j');
    key_char(&mut ed, 'p');
    assert_eq!(ed.line(2), Some(b"world".as_slice()));
    assert_eq!(ed.line(3), Some(b"hello".as_slice()));
}

#[test]
fn autocmd_registers_and_fires() {
    let path = temp_path("event.txt");
    std::fs::write(&path, b"hello\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("autocmd BufWritePost *.txt set number");
    assert_eq!(ed.autocmds.list().len(), 1, "{:?}", ed.messages.last);
    ed.execute_ex("write");
    eprintln!(
        "error={} last={:?} hist={:?}",
        ed.messages.error, ed.messages.last, ed.messages.history
    );
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert!(
        ed.options.window_bool(ed.current_window_id(), "number"),
        "{:?}",
        ed.messages.last
    );
}

#[test]
fn option_registry_covers_vim_92() {
    use rvim_editor::options::{all_spec, all_specs, spec, specs, DefaultValue};
    assert_eq!(all_specs().count(), 456);
    assert!(specs().count() > 300);
    assert!(spec("number").is_some());
    assert!(spec("guifont").is_none());
    assert_eq!(all_spec("guifont").unwrap().feature, Some("gui"));
    assert_eq!(all_spec("foldmethod").unwrap().feature, Some("folding"));
    assert!(spec("foldmethod").is_none());
    assert_eq!(
        spec("cedit").unwrap().kind,
        rvim_editor::options::OptionType::String
    );
    assert_eq!(spec("cedit").unwrap().default, DefaultValue::Str("\u{6}"));
    assert!(rvim_editor::options::resolve("history")
        .unwrap()
        .flags
        .contains("P_NUM"));
}

#[test]
fn gated_options_and_validators_reject_bad_values() {
    let mut ed = Editor::new(Size::new(12, 50));
    ed.execute_ex("set foldmethod=manual");
    assert!(ed.messages.error, "folding option should be gated");
    ed.execute_ex("set guifont=Monaco");
    assert!(ed.messages.error, "GUI option should be gated");
    ed.execute_ex("set undofile");
    assert!(ed.messages.error, "persistent undo option should be gated");
    ed.execute_ex("set columns=0");
    assert!(ed.messages.error, "columns=0 should be rejected");
    ed.execute_ex("set background=banana");
    assert!(ed.messages.error, "bad background value should be rejected");
    ed.execute_ex("set number=banana");
    assert!(ed.messages.error, "bad boolean value should be rejected");
    ed.messages.clear();
    ed.execute_ex("set number");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.messages.clear();
    ed.execute_ex("set columns=80");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
}

#[test]
fn statusline_click_regions_dispatch_callbacks() {
    let path = temp_path("click.txt");
    std::fs::write(
        &path, b"x
",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(10, 60));
    ed.open_file(&path).unwrap();
    ed.source_viml(
        b"function! ClickMe(info)
  let g:click_minwid = a:info.minwid
  let g:click_area = a:info.area
  let g:click_count = a:info.nclicks
  let g:click_button = a:info.button
  return 0
endfunction
",
        "click.vim",
    )
    .unwrap();
    assert_eq!(
        ed.eval_viml("has('statusline_click')")
            .unwrap()
            .echo_string(),
        "1"
    );
    ed.execute_ex("set laststatus=2");
    ed.execute_ex("set statusline=%3[ClickMe]%f%[]%l:%c");
    let frame = ed.render();
    let click = frame
        .clicks
        .iter()
        .find(|click| click.function == "ClickMe")
        .expect("statusline click region")
        .clone();
    let mouse = MouseEvent {
        row: click.rect.row,
        col: click.rect.col,
        kind: MouseKind::Press,
        button: MouseButton::Left,
        mods: Mods::empty(),
    };
    ed.handle_event(UiEvent::Mouse(mouse.clone()));
    assert_eq!(ed.eval_viml("g:click_minwid").unwrap().echo_string(), "3");
    assert_eq!(
        ed.eval_viml("g:click_area").unwrap().echo_string(),
        "statusline"
    );
    assert_eq!(ed.eval_viml("g:click_count").unwrap().echo_string(), "1");
    assert_eq!(ed.eval_viml("g:click_button").unwrap().echo_string(), "l");
    ed.handle_event(UiEvent::Mouse(mouse));
    assert_eq!(ed.eval_viml("g:click_count").unwrap().echo_string(), "2");
}

#[test]
fn substitute_supports_case_regions_and_repeat() {
    let path = temp_path("subflags.txt");
    std::fs::write(
        &path,
        b"ab ab
ab ab
cd
",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex(r":1s/\(a\)\(b\)/\U\1\E\2/g");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.line(1), Some(b"Ab Ab".as_slice()));
    ed.execute_ex(":2s/ab/XY/");
    assert_eq!(ed.line(2), Some(b"XY ab".as_slice()));
    ed.execute_ex(":2s");
    assert_eq!(ed.line(2), Some(b"XY XY".as_slice()));
    ed.execute_ex(":2s/XY/ZZ/ 2");
    assert_eq!(ed.line(2), Some(b"XY ZZ".as_slice()));
}

#[test]
fn substitute_ampersand_flag_keeps_previous_flags() {
    let path = temp_path("subamp.txt");
    std::fs::write(
        &path, b"x x
",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex(":1s/x/y/g");
    assert_eq!(ed.line(1), Some(b"y y".as_slice()));
    ed.execute_ex(":1s/y/z/&");
    assert_eq!(ed.line(1), Some(b"z z".as_slice()));
}

#[test]
fn substitute_tilde_uses_previous_replacement() {
    let path = temp_path("subtilde.txt");
    std::fs::write(
        &path,
        b"one
two
",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex(":1s/one/FIRST/");
    ed.execute_ex(":2s/two/~/");
    assert_eq!(ed.line(2), Some(b"FIRST".as_slice()));
}

#[test]
fn visual_delete_and_mapping() {
    let path = temp_path("visual.txt");
    std::fs::write(&path, b"hello world\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    key_char(&mut ed, 'v');
    key_char(&mut ed, 'l');
    key_char(&mut ed, 'l');
    key_char(&mut ed, 'd');
    assert_eq!(ed.line(1), Some(b"lo world".as_slice()));

    ed.execute_ex("nnoremap X dd");
    key_char(&mut ed, 'X');
    assert_eq!(ed.line(1), Some(b"".as_slice()));
}

#[test]
fn macros_replay_keys() {
    let path = temp_path("macro.txt");
    std::fs::write(&path, b"abcd\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    for ch in "qxxq@x".chars() {
        key_char(&mut ed, ch);
    }
    assert_eq!(ed.line(1), Some(b"cd".as_slice()));
}

#[test]
fn splits_and_tabs_open() {
    let mut ed = Editor::new(Size::new(20, 60));
    ed.execute_ex("vsplit");
    assert_eq!(ed.view.windows.len(), 2);
    ed.execute_ex("tabnew");
    assert_eq!(ed.view.tab_count(), 2);
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
}

#[test]
fn latin1_roundtrip() {
    let path = temp_path("latin1.txt");
    std::fs::write(&path, [0xE9, b'\n']).unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_eq!(ed.line(1), Some("é".as_bytes()));
    ed.execute_ex("write");
    assert_eq!(std::fs::read(&path).unwrap(), [0xE9, b'\n']);
}

#[test]
fn modeline_applies_safe_options() {
    let path = temp_path("modeline.txt");
    std::fs::write(&path, b"text\n# vim: set ts=4 sw=2:\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    assert_eq!(
        ed.options.buffer_number(ed.current_buffer_id(), "tabstop"),
        4
    );
    assert_eq!(
        ed.options
            .buffer_number(ed.current_buffer_id(), "shiftwidth"),
        2
    );
}

#[test]
fn regex_substitute_captures() {
    let path = temp_path("regex.txt");
    std::fs::write(&path, b"foo foo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex(r"s/\(f\)\(o\)/\2\1/g");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.line(1), Some(b"ofo ofo".as_slice()));
}

#[test]
fn syntax_highlight_reaches_cells() {
    let path = temp_path("syntax.rs");
    std::fs::write(&path, b"fn main() {}\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("syntax on");
    ed.execute_ex("syntax keyword Keyword fn");
    let frame = ed.render();
    let cell = frame.base().cell(0, 0).unwrap();
    assert_ne!(frame.base().attr(cell), rvim_ui_api::Attr::DEFAULT);
}

#[test]
fn cursorline_and_list_render() {
    let path = temp_path("render.txt");
    std::fs::write(&path, b"a\tb  \n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cursorline list");
    let frame = ed.render();
    let cell = frame.base().cell(0, 0).unwrap();
    assert_ne!(frame.base().attr(cell).bg, rvim_ui_api::Color::Default);
    assert!(frame.base().row_text(0).contains('>'));
}

#[test]
fn quickfix_file_and_jump() {
    let dir = temp_path("qf.txt").parent().unwrap().to_path_buf();
    let source = dir.join("source.txt");
    std::fs::write(&source, b"one\ntwo\nthree\n").unwrap();
    let errors = dir.join("errors.txt");
    std::fs::write(&errors, format!("{}:2:1: bad line\n", source.display())).unwrap();
    let mut ed = Editor::new(Size::new(12, 60));
    ed.open_file(&errors).unwrap();
    ed.execute_ex(&format!("cfile {}", errors.display()));
    assert_eq!(ed.quickfix.entries.len(), 1, "{:?}", ed.messages.last);
    ed.execute_ex("copen");
    assert_eq!(ed.view.windows.len(), 2, "{:?}", ed.messages.last);
    ed.execute_ex("cnext");
    assert_eq!(ed.line(2), Some(b"two".as_slice()));
}

#[test]
fn vimgrep_populates_quickfix() {
    let dir = temp_path("vg.txt").parent().unwrap().to_path_buf();
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, b"foo\nbar\n").unwrap();
    std::fs::write(&b, b"baz\nfoo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 60));
    ed.open_file(&a).unwrap();
    ed.execute_ex(&format!("vimgrep /foo/ {} {}", a.display(), b.display()));
    assert_eq!(ed.quickfix.entries.len(), 2, "{:?}", ed.messages.last);
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("cnext");
    assert_eq!(ed.line(2), Some(b"foo".as_slice()));
}

#[test]
fn runtime_syntax_file_loads() {
    let root = temp_path("rt").parent().unwrap().to_path_buf();
    std::fs::create_dir_all(root.join("syntax")).unwrap();
    std::fs::write(
        root.join("syntax/rust.vim"),
        b"syn keyword Keyword fn let\n",
    )
    .unwrap();
    let file = root.join("main.rs");
    std::fs::write(&file, b"fn main() {}\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.runtime = rvim_runtime::RuntimePaths::discover(Some(root), None);
    ed.open_file(&file).unwrap();
    ed.execute_ex("syntax on");
    let frame = ed.render();
    let cell = frame.base().cell(0, 0).unwrap();
    assert_ne!(
        frame.base().attr(cell),
        rvim_ui_api::Attr::DEFAULT,
        "{:?}",
        ed.messages.last
    );
}

#[test]
fn viml_let_echo_and_execute() {
    let path = temp_path("viml.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let x = 40 + 2");
    ed.execute_ex("echo x");
    assert_eq!(ed.messages.last.as_deref(), Some("42"));
    ed.execute_ex("execute 'set number'");
    assert!(
        ed.options.window_bool(ed.current_window_id(), "number"),
        "{:?}",
        ed.messages.last
    );
}

#[test]
fn viml_source_functions_and_loop() {
    let root = temp_path("viml-script.vim").parent().unwrap().to_path_buf();
    let script = root.join("test.vim");
    std::fs::write(
        &script,
        b"let g:total = 0\nfor i in range(4)\n  let g:total += i\nendfor\nfunction! Twice(a)\n  return a:a * 2\nendfunction\n",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.execute_ex(&format!("source {}", script.display()));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    let value = ed.eval_viml("g:total").unwrap();
    assert_eq!(value.echo_string(), "6");
    let value = ed.eval_viml("Twice(21)").unwrap();
    assert_eq!(value.echo_string(), "42");
}

#[test]
fn swap_file_creation_and_recovery() {
    let path = temp_path("swap.txt");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let directory = path.parent().unwrap().to_path_buf();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.execute_ex(&format!("set directory={}", directory.display()));
    ed.open_file(&path).unwrap();
    let swap = rvim_editor::swap::swap_path(&path, directory.to_str());
    assert!(swap.exists(), "swap missing at {}", swap.display());
    let (original, content, _) = rvim_editor::swap::read_swap(&swap).unwrap();
    assert_eq!(original, path);
    assert!(content.windows(3).any(|window| window == b"one"));

    std::fs::write(&swap, {
        let mut bytes = b"RVIMSWAP1\npid 1\ntick 2\nfile ".to_vec();
        bytes.extend_from_slice(path.display().to_string().as_bytes());
        bytes.extend_from_slice(b"\n\nrecovered\ntext\n");
        bytes
    })
    .unwrap();

    let mut recovered = Editor::new(Size::new(12, 50));
    recovered.recover_current(Some(&path)).unwrap();
    assert_eq!(recovered.line(1), Some(b"recovered".as_slice()));
    recovered.shutdown_swaps();
    let _ = std::fs::remove_file(swap);
}

#[test]
fn autocmd_once_and_clear() {
    let path = temp_path("autocmd-once.txt");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:count = 0");
    ed.execute_ex("autocmd User Foo ++once let g:count += 1");
    ed.execute_ex("doautocmd User Foo");
    ed.execute_ex("doautocmd User Foo");
    let value = ed.eval_viml("g:count").unwrap();
    assert_eq!(value.echo_string(), "1");
    ed.execute_ex("autocmd! User Foo");
    assert!(ed.autocmds.list().is_empty(), "{:?}", ed.autocmds.list());
}

#[test]
fn compiler_command_sets_makeprg() {
    let root = temp_path("compiler-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let compiler = root.join("compiler");
    std::fs::create_dir_all(&compiler).unwrap();
    std::fs::write(
        compiler.join("demo.vim"),
        b"CompilerSet makeprg=demo-build\n",
    )
    .unwrap();
    let file = root.join("demo.src");
    std::fs::write(&file, b"x\n").unwrap();
    let old = std::env::var("VIMRUNTIME").ok();
    std::env::set_var("VIMRUNTIME", &root);
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&file).unwrap();
    ed.execute_ex("compiler demo");
    let id = ed.current_buffer_id();
    assert_eq!(ed.options.buffer_string(id, "makeprg"), "demo-build");
    if let Some(old) = old {
        std::env::set_var("VIMRUNTIME", old);
    } else {
        std::env::remove_var("VIMRUNTIME");
    }
}

#[test]
fn bare_autocmd_bang_clears_all() {
    let path = temp_path("autocmd-clear.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("autocmd User Foo let g:x = 1");
    assert_eq!(ed.autocmds.list().len(), 1);
    ed.execute_ex("autocmd!");
    assert!(ed.autocmds.list().is_empty(), "{:?}", ed.autocmds.list());
}

#[test]
fn statusline_evaluates_viml_expression() {
    let path = temp_path("statusline.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:status_marker = 'READY'");
    ed.execute_ex("set laststatus=2");
    ed.execute_ex("set statusline=%a%{g:status_marker}");
    let frame = ed.render();
    let row = (0..frame.base().size.rows)
        .map(|row| frame.base().row_text(row))
        .find(|line| line.contains("READY"));
    let row = row.unwrap();
    assert!(row.contains("NORMAL"), "{row}");
}

#[test]
fn append_and_append_end_place_cursor_correctly() {
    let path = temp_path("append.txt");
    std::fs::write(&path, b"abc\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    type_text(&mut ed, "A!");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"abc!".as_slice()));
    ed.set_cursor(BufPos::new(1, 0));
    type_text(&mut ed, "aX");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"aXbc!".as_slice()));
}

#[test]
fn message_pipeline_kinds_append_and_vvars() {
    use rvim_editor::MessageKind;
    let path = temp_path("messages.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 50));
    ed.open_file(&path).unwrap();
    ed.messages.clear();
    ed.execute_ex("echon 'a'");
    ed.execute_ex("echon 'b'");
    assert_eq!(ed.messages.last.as_deref(), Some("ab"));
    assert_eq!(ed.messages.kind, MessageKind::Echo);
    ed.execute_ex("echomsg 'kept'");
    assert_eq!(ed.messages.kind, MessageKind::EchoMsg);
    assert!(ed.messages.history.iter().any(|message| message == "kept"));
    ed.execute_ex("echoerr 'bad'");
    assert!(ed.messages.error);
    assert_eq!(ed.messages.kind, MessageKind::EchoErr);
    let errmsg = ed.eval_viml("v:errmsg").unwrap();
    assert_eq!(errmsg.echo_string(), "bad");
    ed.execute_ex("echowindow 'popup'");
    assert!(ed.messages.echo_window);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)]
fn partial_write_and_readonly_override() {
    let path = temp_path("range-src.txt");
    std::fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    let out = path.with_file_name("range-out.txt");
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex(&format!("2,3w {}", out.display()));
    assert_eq!(std::fs::read(&out).unwrap(), b"two\nthree\n");
    ed.execute_ex(&format!("w >> {}", out.display()));
    assert!(std::fs::read(&out).unwrap().len() > b"two\nthree\n".len());

    let readonly = path.with_file_name("readonly.txt");
    std::fs::write(&readonly, b"old\n").unwrap();
    let mut perms = std::fs::metadata(&readonly).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&readonly, perms).unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex(&format!("file {}", readonly.display()));
    ed.execute_ex("write");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("write!");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    let mut perms = std::fs::metadata(&readonly).unwrap().permissions();
    perms.set_readonly(false);
    let _ = std::fs::set_permissions(&readonly, perms);
}

#[test]
fn full_metadata_registries_are_present() {
    use rvim_editor::registry::{ALL_COMMANDS, ALL_EVENTS, ALL_FUNCTIONS};
    assert_eq!(ALL_COMMANDS.len(), 593);
    assert_eq!(ALL_FUNCTIONS.len(), 605);
    assert_eq!(ALL_EVENTS.len(), 136);
    assert!(ALL_COMMANDS
        .iter()
        .any(|spec| spec.name == "edit" && spec.implemented));
    assert!(ALL_COMMANDS
        .iter()
        .any(|spec| spec.name == "terminal" && !spec.implemented));
    assert_eq!(rvim_editor::registry::command_abbrev("edit"), Some("e"));
    assert_eq!(
        rvim_editor::registry::command_abbrev("substitute"),
        Some("s")
    );
    assert!(rvim_editor::registry::function_metadata("abs").is_some());
    assert!(rvim_editor::registry::event_metadata("BufReadPost").is_some());
    let edit = rvim_editor::registry::command_metadata("edit").unwrap();
    assert_eq!(edit.handler, "ex_edit");
    assert!(edit.flags.contains("EX_"));
    assert_eq!(edit.min_args, 0);
    let abs = rvim_editor::registry::function_metadata("abs").unwrap();
    assert_eq!((abs.min_args, abs.max_args), (1, 1));
    let range = rvim_editor::registry::function_metadata("range").unwrap();
    assert_eq!((range.min_args, range.max_args), (1, 3));
    assert_eq!(
        rvim_editor::registry::command_metadata("terminal")
            .unwrap()
            .feature,
        "terminal"
    );
}

#[test]
fn scheduler_runs_due_timer_commands() {
    let path = temp_path("timer.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("let g:timer_fired = 0");
    let id = ed.start_timer(0, false, "let g:timer_fired = 1".to_string());
    assert!(ed.next_deadline().is_some());
    ed.on_tick(std::time::Instant::now() + std::time::Duration::from_millis(5));
    let value = ed.eval_viml("g:timer_fired").unwrap();
    assert_eq!(value.echo_string(), "1");
    assert!(!ed.stop_timer(id));
}

#[test]
fn frame_publishes_semantic_hit_regions() {
    use rvim_ui_api::ActionKind;
    let path = temp_path("hit.txt");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set laststatus=2");
    let frame = ed.render();
    assert!(frame
        .hit_regions
        .iter()
        .any(|region| region.action.kind == ActionKind::Window));
    assert!(frame
        .hit_regions
        .iter()
        .any(|region| region.action.kind == ActionKind::Statusline));
}

#[test]
fn popup_layer_composites_and_marks_dirty_rows() {
    use rvim_ui_api::{ActionKind, LayerKind};
    let path = temp_path("popup.txt");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("echowindow 'hello popup'");
    let frame = ed.render();
    assert!(frame
        .layers
        .iter()
        .any(|layer| layer.kind == LayerKind::Popup));
    assert!(frame
        .hit_regions
        .iter()
        .any(|region| region.action.kind == ActionKind::PopupItem));
    assert!(frame.composite().to_plain_text().contains("hello popup"));
    assert!(!frame.dirty_rows.is_empty());
}

#[test]
fn undo_tree_keeps_branches_and_redo_picks_latest() {
    let path = temp_path("undo-branch.txt");
    std::fs::write(&path, b"a\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    type_text(&mut ed, "AX");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"aX".as_slice()));
    type_text(&mut ed, "u");
    assert_eq!(ed.line(1), Some(b"a".as_slice()));
    type_text(&mut ed, "AY");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(1), Some(b"aY".as_slice()));
    type_text(&mut ed, "u");
    ed.handle_key(KeyEvent::with_mods(Key::Char('r'), Mods::CTRL));
    assert_eq!(ed.line(1), Some(b"aY".as_slice()));
    let seq = ed.eval_viml("undotree().seq_last").unwrap();
    assert!(seq.echo_string().parse::<u64>().unwrap() >= 2);
}

#[test]
fn global_statusline_replaces_per_window_rows() {
    use rvim_ui_api::ActionKind;
    let path = temp_path("global-status.txt");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("split");
    ed.execute_ex("set laststatus=3");
    let frame = ed.render();
    let statuses = frame
        .hit_regions
        .iter()
        .filter(|region| region.action.kind == ActionKind::Statusline)
        .count();
    assert_eq!(statuses, 1, "{:?}", frame.hit_regions);
}

#[test]
fn custom_tabline_template_renders_tabs() {
    let path = temp_path("tabline.txt");
    std::fs::write(&path, b"one\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("tabnew");
    ed.execute_ex("set tabline=ROOT%1T%2T");
    let frame = ed.render();
    let row = frame.base().row_text(0);
    assert!(row.contains("ROOT"), "{row}");
    assert!(row.contains("[1]") && row.contains("[2]"), "{row}");
    use rvim_ui_api::ActionKind;
    let tabs = frame
        .hit_regions
        .iter()
        .filter(|region| region.action.kind == ActionKind::Tab)
        .count();
    assert_eq!(tabs, 2, "{:?}", frame.hit_regions);
}

#[test]
fn wildmenu_renders_completion_popup() {
    use rvim_editor::editor::CmdCompletion;
    let path = temp_path("wildmenu.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.cmdline_completion = Some(CmdCompletion {
        start: 0,
        candidates: vec!["alpha".to_string(), "alpine".to_string()],
        index: 1,
    });
    let frame = ed.render();
    assert!(frame.layers.len() >= 2);
    assert!(frame.composite().to_plain_text().contains("[alpine]"));
}

#[test]
fn command_line_window_opens_and_executes_line() {
    let path = temp_path("cmdwin.txt");
    std::fs::write(&path, b"one\n").unwrap();
    let mut ed = Editor::new(Size::new(14, 50));
    ed.open_file(&path).unwrap();
    ed.messages.clear();
    type_text(&mut ed, "q:");
    assert!(
        ed.cmdwin.is_some(),
        "cmdwin did not open: {:?}",
        ed.messages.last
    );
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 2);
    assert_eq!(ed.eval_viml("getcmdwintype()").unwrap().echo_string(), ":");
    ed.execute_ex("call setline(1, 'let g:cmdwin_value = 42')");
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert!(ed.cmdwin.is_none());
    let value = ed.eval_viml("g:cmdwin_value").unwrap();
    assert_eq!(value.echo_string(), "42");
}

#[test]
fn gq_formats_paragraph_to_textwidth() {
    let path = temp_path("gq.txt");
    std::fs::write(&path, b"    alpha beta gamma delta epsilon zeta\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set textwidth=18");
    type_text(&mut ed, "gq");
    assert!(ed.line_count() > 1, "{:?}", ed.line(1));
    for lnum in 1..=ed.line_count() {
        assert!(ed.line(lnum).map_or(0, |line| line.len()) <= 18);
    }
    assert!(ed.line(1).is_some_and(|line| line.starts_with(b"    ")));
}

#[test]
fn shift_operators_indent_and_outdent() {
    let path = temp_path("shift.txt");
    std::fs::write(&path, b"line\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set shiftwidth=4");
    type_text(&mut ed, ">>");
    assert_eq!(ed.line(1), Some(b"    line".as_slice()));
    type_text(&mut ed, "<<");
    assert_eq!(ed.line(1), Some(b"line".as_slice()));
}

#[test]
fn engine_event_reactor_applies_events() {
    use rvim_editor::scheduler::EngineEvent;
    let path = temp_path("reactor.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.post_event(EngineEvent::Resize(Size::new(12, 50)));
    ed.post_event(EngineEvent::User("let g:reactor_value = 7".to_string()));
    ed.post_event(EngineEvent::Redraw);
    ed.on_tick(std::time::Instant::now());
    assert_eq!(ed.size, Size::new(12, 50));
    assert_eq!(ed.eval_viml("g:reactor_value").unwrap().echo_string(), "7");
    assert!(ed.should_redraw);
}

#[test]
fn dynamic_screen_dependent_option_defaults() {
    let ed = Editor::new(Size::new(24, 80));
    assert_eq!(ed.options.get_number("window"), 23);
    assert_eq!(ed.options.get_number("scroll"), 11);
    assert_eq!(ed.options.get_number("maxmem"), 5120);
}

#[test]
fn indentexpr_controls_newline_indent() {
    let path = temp_path("indentexpr.txt");
    std::fs::write(&path, b"a\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set indentexpr=4");
    ed.set_cursor(BufPos::new(1, 0));
    type_text(&mut ed, "A");
    ed.handle_key(KeyEvent::new(Key::Enter));
    type_text(&mut ed, "b");
    ed.handle_key(KeyEvent::new(Key::Esc));
    assert_eq!(ed.line(2), Some(b"    b".as_slice()));
}

#[test]
fn getcompletion_returns_typed_candidates() {
    let path = temp_path("completion.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    let commands = ed.eval_viml("getcompletion('edi', 'command')").unwrap();
    assert!(commands.echo_string().contains("edit"));
    let events = ed.eval_viml("getcompletion('BufRead', 'event')").unwrap();
    assert!(events.echo_string().contains("BufReadPost"));
    let options = ed.eval_viml("getcompletion('number', 'option')").unwrap();
    assert!(options.echo_string().contains("numberwidth"));
    ed.execute_ex("nnoremap <F2> :echo<CR>");
    let mappings = ed.eval_viml("getcompletion('<F2>', 'mapping')").unwrap();
    assert!(mappings.echo_string().contains("<F2>"));
    let syntax = ed.eval_viml("getcompletion('', 'syntax')").unwrap();
    assert!(syntax.echo_string().contains("Comment"));
}

#[test]
fn vimgrep_supports_recursive_globs() {
    let root = temp_path("vimgrep-glob.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("a.rs"), b"needle one\n").unwrap();
    std::fs::write(root.join("sub").join("b.rs"), b"needle two\n").unwrap();
    std::fs::write(root.join("c.txt"), b"needle three\n").unwrap();
    let main = root.join("main.txt");
    std::fs::write(&main, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 50));
    ed.open_file(&main).unwrap();
    let pattern = root.join("**/*.rs");
    ed.execute_ex(&format!("vimgrep /needle/ {}", pattern.display()));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.quickfix.entries.len(), 2, "{:?}", ed.quickfix.entries);
}

#[test]
fn path_modifiers_resolve_relative_to_current_file() {
    let root = temp_path("path-mod-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    std::fs::write(&first, b"one\n").unwrap();
    std::fs::write(&second, b"two\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&first).unwrap();
    ed.execute_ex("edit %:h/second.txt");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(second)
    );
}

#[test]
fn tag_select_and_next_navigate_matches() {
    let root = temp_path("tags-root.txt").parent().unwrap().to_path_buf();
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
    let mut ed = Editor::new(Size::new(10, 50));
    ed.open_file(&root.join("main.txt")).unwrap();
    ed.tag_index = rvim_editor::tags::TagIndex::load(&tags);
    ed.execute_ex("tselect target");
    assert_eq!(ed.tag_matches.len(), 2);
    ed.execute_ex("tnext");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(two)
    );
    ed.execute_ex("tfirst");
    assert_eq!(
        ed.current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone()),
        Some(one)
    );
}

#[test]
fn help_opens_split_and_helpclose_closes_it() {
    let root = temp_path("help-root.txt").parent().unwrap().to_path_buf();
    std::fs::create_dir_all(root.join("doc")).unwrap();
    std::fs::write(root.join("doc/help.txt"), b"*topic*\nhelp text\n").unwrap();
    std::fs::write(root.join("doc/tags"), b"topic\thelp.txt\t/*topic*\n").unwrap();
    let main = root.join("main.txt");
    std::fs::write(&main, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(14, 50));
    ed.runtime = rvim_runtime::RuntimePaths::discover(Some(root.clone()), None);
    ed.tag_index = rvim_editor::tags::TagIndex::load(&root.join("doc/tags"));
    ed.open_file(&main).unwrap();
    ed.execute_ex("help topic");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 2);
    assert_eq!(
        ed.current_buffer().map(|buffer| buffer.buftype.as_str()),
        Some("help")
    );
    ed.execute_ex("helpclose");
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 1);
}

#[test]
fn keyword_lookup_runs_keywordprg_for_word_under_cursor() {
    let path = temp_path("keywordprg.txt");
    std::fs::write(&path, b"alpha beta\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set keywordprg=echo");
    ed.set_cursor(BufPos::new(1, 6));
    key_char(&mut ed, 'K');
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.messages.last.as_deref(), Some("beta"));
}

#[test]
fn keyword_lookup_opens_help_without_keywordprg() {
    let root = temp_path("kp-help-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(root.join("doc")).unwrap();
    std::fs::write(root.join("doc/help.txt"), b"*beta*\nbeta help\n").unwrap();
    std::fs::write(root.join("doc/tags"), b"beta\thelp.txt\t/*beta*\n").unwrap();
    let main = root.join("main.txt");
    std::fs::write(&main, b"beta\n").unwrap();
    let mut ed = Editor::new(Size::new(14, 50));
    ed.runtime = rvim_runtime::RuntimePaths::discover(Some(root.clone()), None);
    ed.tag_index = rvim_editor::tags::TagIndex::load(&root.join("doc/tags"));
    ed.open_file(&main).unwrap();
    ed.execute_ex("set keywordprg=");
    key_char(&mut ed, 'K');
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.current_buffer().map(|buffer| buffer.buftype.as_str()),
        Some("help")
    );
}

#[test]
fn g_bracket_selects_tag_for_identifier() {
    let root = temp_path("g-bracket-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
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
    let main = root.join("main.txt");
    std::fs::write(&main, b"target\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 50));
    ed.open_file(&main).unwrap();
    ed.tag_index = rvim_editor::tags::TagIndex::load(&tags);
    type_text(&mut ed, "g]");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.tag_matches.len(), 2);
}

#[test]
fn exrc_sources_local_vimrc_and_exrc() {
    let root = temp_path("exrc-root.txt").parent().unwrap().to_path_buf();
    std::fs::write(root.join(".vimrc"), b"let g:exrc_vimrc = 1\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.options
        .set_global("exrc", rvim_editor::OptionValue::Bool(true))
        .unwrap();
    ed.source_local_exrc(&root).unwrap();
    assert_eq!(ed.eval_viml("g:exrc_vimrc").unwrap().echo_string(), "1");

    let exrc_only = root.join("exrc-only");
    std::fs::create_dir_all(&exrc_only).unwrap();
    std::fs::write(exrc_only.join(".exrc"), b"let g:exrc_fallback = 1\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.options
        .set_global("exrc", rvim_editor::OptionValue::Bool(true))
        .unwrap();
    ed.source_local_exrc(&exrc_only).unwrap();
    assert_eq!(ed.eval_viml("g:exrc_fallback").unwrap().echo_string(), "1");
}

#[test]
fn secure_exrc_rejects_shell_and_write_commands() {
    let root = temp_path("exrc-secure-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::write(
        root.join(".vimrc"),
        b"let g:exrc_before = 1\n!echo nope\nwrite somewhere.txt\n",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.options
        .set_global("exrc", rvim_editor::OptionValue::Bool(true))
        .unwrap();
    ed.options
        .set_global("secure", rvim_editor::OptionValue::Bool(true))
        .unwrap();
    let error = ed.source_local_exrc(&root).unwrap_err();
    assert!(
        error.to_string().contains("not allowed from exrc"),
        "{error}"
    );
    assert_eq!(ed.eval_viml("g:exrc_before").unwrap().echo_string(), "1");
}

#[test]
fn insecure_exrc_allows_shell_commands() {
    let root = temp_path("exrc-insecure-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::write(root.join(".vimrc"), b"!echo allowed\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.options
        .set_global("exrc", rvim_editor::OptionValue::Bool(true))
        .unwrap();
    ed.source_local_exrc(&root).unwrap();
    assert_eq!(ed.messages.last.as_deref(), Some("allowed"));
}

#[test]
fn sandbox_modifier_blocks_shell_and_writes() {
    let path = temp_path("sandbox.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("sandbox !echo pwned");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.messages.last.as_deref(),
        Some("E048: Not allowed in sandbox")
    );
    assert!(!ed
        .messages
        .history
        .iter()
        .any(|line| line.contains("pwned")));
    ed.execute_ex("sandbox echo 'ok'");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.messages.last.as_deref(), Some("ok"));
    let shell = ed.options.get_string("shell");
    ed.execute_ex("sandbox set shell=/bin/sh");
    assert_eq!(
        ed.messages.last.as_deref(),
        Some("E048: Not allowed in sandbox")
    );
    assert_eq!(ed.options.get_string("shell"), shell);
}

#[test]
fn secure_exrc_rejects_p_secure_options() {
    let root = temp_path("exrc-secure-options-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::write(root.join(".vimrc"), b"set exrc\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.options
        .set_global("exrc", rvim_editor::OptionValue::Bool(true))
        .unwrap();
    ed.options
        .set_global("secure", rvim_editor::OptionValue::Bool(true))
        .unwrap();
    let error = ed.source_local_exrc(&root).unwrap_err();
    assert!(
        error.to_string().contains("not allowed from exrc"),
        "{error}"
    );
}
#[test]
fn cedit_opens_command_window_with_typed_text() {
    let path = temp_path("cedit.txt");
    std::fs::write(&path, b"one\n").unwrap();
    let mut ed = Editor::new(Size::new(20, 60));
    ed.open_file(&path).unwrap();
    ed.messages.clear();
    type_text(&mut ed, ":set number");
    ed.handle_key(KeyEvent::with_mods(Key::Char('f'), Mods::CTRL));
    assert!(ed.cmdwin.is_some(), "{:?}", ed.messages.last);
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 2);
    let last = ed.line_count();
    assert_eq!(ed.line(last), Some(b"set number".as_slice()));
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert!(ed.cmdwin.is_none());
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert!(ed
        .options
        .window_bool(ed.view.current_window().unwrap().id.raw(), "number"));
}

#[test]
fn command_window_uses_cmdwinheight() {
    let path = temp_path("cmdwin-height.txt");
    std::fs::write(&path, b"one\n").unwrap();
    let mut ed = Editor::new(Size::new(24, 60));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set cmdwinheight=5");
    type_text(&mut ed, "q:");
    let id = ed.view.current_window().unwrap().id;
    let rows = ed
        .view
        .layout_rects(0, 0)
        .into_iter()
        .find(|rect| rect.window == id)
        .unwrap()
        .text
        .rows;
    assert_eq!(rows, 5);
}

#[test]
fn invalid_cedit_value_is_rejected() {
    let mut ed = Editor::new(Size::new(10, 40));
    ed.execute_ex("set cedit=x");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
    assert!(ed.messages.last.as_deref().unwrap().contains("E474"));
}

#[test]
fn diagnostics_log_records_categories() {
    let root = temp_path("trace-root.txt").parent().unwrap().to_path_buf();
    let log = root.join("rvim.log");
    let mut ed = Editor::new(Size::new(10, 40));
    ed.set_trace(&log, "input,ex,viml,render").unwrap();
    ed.execute_ex("set number");
    ed.handle_key(KeyEvent::new(Key::Char('j')));
    ed.eval_viml("1 + 1").unwrap();
    let _ = ed.render();
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("start rvim log session"), "{text}");
    assert!(text.contains("[ex] set number"), "{text}");
    assert!(text.contains("[input]"), "{text}");
    assert!(text.contains("[viml] eval 1 + 1"), "{text}");
    assert!(text.contains("[render]"), "{text}");
    assert!(!text.contains("[runtime]"), "{text}");
}

#[test]
fn crash_report_is_written() {
    let path = temp_path("crash-report.log");
    let written =
        rvim_editor::trace::write_crash_report(Some(&path), "boom", Some("src/main.rs:1:1"))
            .unwrap();
    assert_eq!(written, path);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("RVim crash report"), "{text}");
    assert!(text.contains("message: boom"), "{text}");
    assert!(text.contains("location: src/main.rs:1:1"), "{text}");
}

#[test]
fn location_list_is_per_window_and_separate_from_quickfix() {
    let root = temp_path("loclist-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let one = root.join("one.txt");
    let two = root.join("two.txt");
    std::fs::write(&one, b"alpha\nbeta\n").unwrap();
    std::fs::write(&two, b"gamma\n").unwrap();
    let mut ed = Editor::new(Size::new(20, 60));
    ed.open_file(&one).unwrap();
    ed.execute_ex(&format!("lgetexpr '{}:2:first'", one.display()));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.quickfix.entries.len(), 0);
    assert_eq!(
        ed.eval_viml("len(getloclist())").unwrap().echo_string(),
        "1"
    );
    ed.execute_ex("split");
    ed.open_file(&two).unwrap();
    ed.execute_ex(&format!("lgetexpr '{}:1:second'", two.display()));
    assert_eq!(
        ed.eval_viml("len(getloclist())").unwrap().echo_string(),
        "1"
    );
    let second = ed.eval_viml("getloclist()[0].text").unwrap().echo_string();
    assert_eq!(second, "second");
    ed.execute_ex("wincmd p");
    let first = ed.eval_viml("getloclist()[0].text").unwrap().echo_string();
    assert_eq!(first, "first");
    ed.execute_ex("cgetexpr 'q:9:quickfix'");
    assert_eq!(ed.quickfix.entries.len(), 1);
    assert_eq!(
        ed.eval_viml("len(getloclist())").unwrap().echo_string(),
        "1"
    );
}

#[test]
fn location_list_jumps_and_window() {
    let root = temp_path("locjump-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let one = root.join("one.txt");
    let two = root.join("two.txt");
    std::fs::write(&one, b"alpha\nbeta\n").unwrap();
    std::fs::write(&two, b"gamma\ndelta\n").unwrap();
    let mut ed = Editor::new(Size::new(24, 60));
    ed.open_file(&one).unwrap();
    ed.execute_ex(&format!(
        "lgetexpr \"{}:2:a\\n{}:2:b\"",
        one.display(),
        two.display()
    ));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("lopen");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 2);
    ed.execute_ex("lnext");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.cursor().lnum, 2);
    assert!(ed
        .current_buffer()
        .map(|buffer| buffer.name_string())
        .unwrap_or_default()
        .ends_with("two.txt"));
    ed.execute_ex("lclose");
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 1);
}

#[test]
fn location_list_stack_older_and_newer() {
    let path = temp_path("locstack.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("lgetexpr 'a.txt:1:first'");
    ed.execute_ex("lgetexpr 'b.txt:1:second'");
    assert_eq!(
        ed.eval_viml("getloclist()[0].text").unwrap().echo_string(),
        "second"
    );
    ed.execute_ex("lolder");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.eval_viml("getloclist()[0].text").unwrap().echo_string(),
        "first"
    );
    ed.execute_ex("lnewer");
    assert_eq!(
        ed.eval_viml("getloclist()[0].text").unwrap().echo_string(),
        "second"
    );
}
#[test]
fn setloclist_and_getloclist_roundtrip() {
    let path = temp_path("locapi.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex(
        "call setloclist(0, [{'filename': 'api.txt', 'lnum': 3, 'col': 2, 'text': 'boom'}])",
    );
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.eval_viml("len(getloclist())").unwrap().echo_string(),
        "1"
    );
    assert_eq!(
        ed.eval_viml("getloclist()[0].lnum").unwrap().echo_string(),
        "3"
    );
    assert_eq!(
        ed.eval_viml("getloclist()[0].text").unwrap().echo_string(),
        "boom"
    );
}
#[test]
fn ctrl_w_brace_opens_preview_tag() {
    let root = temp_path("preview-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let main = root.join("main.txt");
    let target = root.join("target.txt");
    std::fs::write(&main, b"foo\n").unwrap();
    std::fs::write(&target, b"target line one\ntarget line two\n").unwrap();
    let tags = root.join("tags");
    std::fs::write(&tags, format!("foo\t{}\t1\n", target.display())).unwrap();
    let mut ed = Editor::new(Size::new(30, 60));
    ed.open_file(&main).unwrap();
    ed.tag_index = rvim_editor::tags::TagIndex::load(&tags);
    ed.handle_key(KeyEvent::with_mods(Key::Char('w'), Mods::CTRL));
    ed.handle_key(KeyEvent::new(Key::Char('}')));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert!(ed.preview_window.is_some());
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 2);
    assert_eq!(
        ed.current_buffer().map(|buffer| buffer.name_string()),
        Some(main.display().to_string())
    );
    let preview = ed.preview_window.unwrap();
    let preview_buffer = ed.view.windows.get(&preview).unwrap().buffer;
    assert!(ed
        .buffers
        .get(preview_buffer)
        .map(|buffer| buffer.name_string())
        .unwrap_or_default()
        .ends_with("target.txt"));
    ed.execute_ex("pclose");
    assert!(ed.preview_window.is_none());
    assert_eq!(ed.view.window_ids_in_current_tab().len(), 1);
}

#[test]
fn ptag_uses_previewheight_and_pedit() {
    let root = temp_path("preview-height-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let main = root.join("main.txt");
    let target = root.join("tagged.txt");
    let other = root.join("other.txt");
    std::fs::write(&main, b"x\n").unwrap();
    std::fs::write(&target, b"a\nb\n").unwrap();
    std::fs::write(&other, b"other\n").unwrap();
    let tags = root.join("tags");
    std::fs::write(&tags, format!("foo\t{}\t2\n", target.display())).unwrap();
    let mut ed = Editor::new(Size::new(30, 60));
    ed.open_file(&main).unwrap();
    ed.tag_index = rvim_editor::tags::TagIndex::load(&tags);
    ed.execute_ex("set previewheight=5");
    ed.execute_ex("ptag foo");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    let preview = ed.preview_window.unwrap();
    let rows = ed
        .view
        .layout_rects(0, 0)
        .into_iter()
        .find(|rect| rect.window == preview)
        .unwrap()
        .text
        .rows;
    assert_eq!(rows, 5);
    ed.execute_ex(&format!("pedit {}", other.display()));
    let preview_buffer = ed.view.windows.get(&preview).unwrap().buffer;
    assert!(ed
        .buffers
        .get(preview_buffer)
        .map(|buffer| buffer.name_string())
        .unwrap_or_default()
        .ends_with("other.txt"));
    ed.handle_key(KeyEvent::with_mods(Key::Char('w'), Mods::CTRL));
    ed.handle_key(KeyEvent::new(Key::Char('z')));
    assert!(ed.preview_window.is_none());
}

#[test]
fn ptag_reports_missing_tag() {
    let path = temp_path("ptag-missing.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("ptag nope");
    assert!(ed.messages.error, "{:?}", ed.messages.last);
    assert!(ed
        .messages
        .last
        .as_deref()
        .unwrap_or_default()
        .contains("E433"));
}
#[test]
fn prompt_buffer_callbacks_and_prompt_text() {
    let path = temp_path("prompt.txt");
    std::fs::write(&path, b"\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set buftype=prompt");
    ed.source_viml(
        b"function! PromptCb(text)\n  let g:prompt_value = a:text\nendfunction\nfunction! PromptInt()\n  let g:prompt_interrupted = 1\nendfunction\n",
        "[prompt-test]",
    )
    .unwrap();
    ed.execute_ex("call prompt_setprompt(bufnr('%'), '> ')");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.line(1), Some(b"> ".as_slice()));
    ed.execute_ex("call prompt_setcallback(bufnr('%'), function('PromptCb'))");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    key_char(&mut ed, 'a');
    type_text(&mut ed, "hello");
    ed.handle_key(KeyEvent::new(Key::Enter));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.eval_viml("g:prompt_value").unwrap().echo_string(),
        "hello"
    );
    assert_eq!(ed.line(1), Some(b"> ".as_slice()));
    assert!(matches!(ed.mode, rvim_editor::Mode::Insert(_)));
    ed.execute_ex("call prompt_setinterrupt(bufnr('%'), function('PromptInt'))");
    ed.handle_key(KeyEvent::with_mods(Key::Char('c'), Mods::CTRL));
    assert_eq!(
        ed.eval_viml("g:prompt_interrupted").unwrap().echo_string(),
        "1"
    );
    assert!(matches!(ed.mode, rvim_editor::Mode::Insert(_)));
}
#[test]
fn shell_temp_files_and_quoting_are_safe() {
    let root = temp_path("shell-safety-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let path = root.join("input.txt");
    let marker = root.join("pwned");
    std::fs::write(&path, b"one\ntwo\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    // Shell metacharacters from user input are quoted, not executed.
    ed.execute_ex("set grepprg=/bin/echo $*");
    ed.execute_ex(&format!("grep $(touch {})", marker.display()));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert!(!marker.exists(), "shell injection executed");
    // Filtering uses private temp files with 'shellredir' when 'shelltemp' is on.
    ed.execute_ex("set shellredir=>%s\\ 2>&1");
    ed.execute_ex("1,2!tr a-z A-Z");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(ed.line(1), Some(b"ONE".as_slice()));
    assert_eq!(ed.line(2), Some(b"TWO".as_slice()));
    // Pipes are used when 'shelltemp' is off.
    ed.execute_ex("set noshelltemp");
    ed.execute_ex("1,2!tr A-Z a-z");
    assert_eq!(ed.line(1), Some(b"one".as_slice()));
    assert_eq!(
        ed.eval_viml("has('filterpipe')").unwrap().echo_string(),
        "1"
    );
    assert!(!marker.exists(), "shell injection executed");
}
#[test]
fn fileencodings_probe_order_sets_fileencoding() {
    let root = temp_path("encoding-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let latin = root.join("latin1.txt");
    std::fs::write(&latin, b"caf\xe9\n").unwrap();
    let utf = root.join("utf8.txt");
    std::fs::write(&utf, "naïve\n".as_bytes()).unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.open_file(&latin).unwrap();
    assert_eq!(
        ed.options
            .buffer_string(ed.current_buffer_id(), "fileencoding"),
        "latin1"
    );
    assert_eq!(ed.line(1), Some("café".as_bytes()));
    ed.open_file(&utf).unwrap();
    assert_eq!(
        ed.options
            .buffer_string(ed.current_buffer_id(), "fileencoding"),
        "utf-8"
    );
    assert_eq!(ed.line(1), Some("naïve".as_bytes()));
}

#[test]
fn iconv_fallback_reads_and_writes_encodings() {
    let root = temp_path("iconv-root.txt").parent().unwrap().to_path_buf();
    let path = root.join("cp1252.txt");
    std::fs::write(&path, b"caf\xe9\n").unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    ed.execute_ex("set fileencodings=ucs-bom,utf-8,cp1252");
    ed.open_file(&path).unwrap();
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.options
            .buffer_string(ed.current_buffer_id(), "fileencoding"),
        "cp1252"
    );
    assert_eq!(ed.line(1), Some("café".as_bytes()));
    ed.execute_ex("write");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(std::fs::read(&path).unwrap(), b"caf\xe9\n");
}

#[test]
fn scriptencoding_converts_sourced_script() {
    let root = temp_path("scriptenc-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    let path = root.join("latin1.vim");
    std::fs::write(
        &path,
        b"scriptencoding latin1\nlet g:script_text = 'caf\xe9'\n",
    )
    .unwrap();
    let mut ed = Editor::new(Size::new(10, 40));
    let bytes = std::fs::read(&path).unwrap();
    ed.source_viml(&bytes, &path.display().to_string()).unwrap();
    assert_eq!(ed.eval_viml("g:script_text").unwrap().echo_string(), "café");
}
#[test]
fn getloclist_and_setloclist_accept_window_numbers() {
    let root = temp_path("locwin-root.txt").parent().unwrap().to_path_buf();
    let one = root.join("one.txt");
    let two = root.join("two.txt");
    std::fs::write(&one, b"one\n").unwrap();
    std::fs::write(&two, b"two\n").unwrap();
    let mut ed = Editor::new(Size::new(20, 60));
    ed.open_file(&one).unwrap();
    ed.execute_ex("split");
    ed.open_file(&two).unwrap();
    ed.execute_ex(&format!("lgetexpr '{}:1:second'", two.display()));
    ed.execute_ex("wincmd p");
    ed.execute_ex(&format!("lgetexpr '{}:1:first'", one.display()));
    assert_eq!(
        ed.eval_viml("getloclist()[0].text").unwrap().echo_string(),
        "first"
    );
    assert_eq!(
        ed.eval_viml("getloclist(1)[0].text").unwrap().echo_string(),
        "second"
    );
    assert_eq!(
        ed.eval_viml("getloclist(2)[0].text").unwrap().echo_string(),
        "first"
    );
    ed.execute_ex(
        "call setloclist(2, [{'filename': 'changed.txt', 'lnum': 5, 'text': 'changed'}])",
    );
    assert_eq!(
        ed.eval_viml("getloclist(2)[0].text").unwrap().echo_string(),
        "changed"
    );
    assert_eq!(
        ed.eval_viml("getloclist()[0].text").unwrap().echo_string(),
        "changed"
    );
}

#[test]
fn list_do_commands_iterate_entries() {
    let root = temp_path("listdo-root.txt").parent().unwrap().to_path_buf();
    let one = root.join("one.txt");
    let two = root.join("two.txt");
    std::fs::write(&one, b"aa\nbb\n").unwrap();
    std::fs::write(&two, b"cc\ndd\n").unwrap();
    let mut ed = Editor::new(Size::new(16, 60));
    ed.open_file(&one).unwrap();
    ed.execute_ex("set hidden");
    ed.execute_ex(&format!(
        "lgetexpr \"{}:2:a\\n{}:2:b\"",
        one.display(),
        two.display()
    ));
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    ed.execute_ex("ldo s/^/X/");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    for (name, expected) in [("one.txt", "Xbb"), ("two.txt", "Xdd")] {
        let line = ed
            .buffers
            .iter()
            .find(|(_, buffer)| buffer.name_string().ends_with(name))
            .and_then(|(_, buffer)| {
                buffer
                    .text
                    .line(2)
                    .map(|line| String::from_utf8_lossy(line).into_owned())
            });
        assert_eq!(line.as_deref(), Some(expected), "{name}");
    }
    ed.execute_ex(&format!(
        "cgetexpr \"{}:1:q\\n{}:1:r\"",
        one.display(),
        two.display()
    ));
    ed.execute_ex("cfdo s/^/Y/");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    let one_line = ed
        .buffers
        .iter()
        .find(|(_, buffer)| buffer.name_string().ends_with("one.txt"))
        .and_then(|(_, buffer)| {
            buffer
                .text
                .line(1)
                .map(|line| String::from_utf8_lossy(line).into_owned())
        });
    assert_eq!(one_line.as_deref(), Some("Yaa"));
}
#[test]
fn quickfix_title_and_context_roundtrip() {
    let mut ed = Editor::new(Size::new(10, 40));
    ed.execute_ex("call setqflist([], 'r', {'title': 'ctx list', 'context': {'x': 7}})");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.eval_viml("getqflist({'context': 1}).context.x")
            .unwrap()
            .echo_string(),
        "7"
    );
    assert_eq!(
        ed.eval_viml("getqflist({'title': 1}).title")
            .unwrap()
            .echo_string(),
        "ctx list"
    );
}
#[test]
fn help_reloads_runtime_tags_after_tags_option_changes() {
    let root = temp_path("help-retags-root.txt")
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(root.join("doc")).unwrap();
    std::fs::write(root.join("doc/help.txt"), b"*topic*\nhelp text\n").unwrap();
    std::fs::write(root.join("doc/tags"), b"topic\thelp.txt\t/*topic*\n").unwrap();
    let main = root.join("main.txt");
    std::fs::write(&main, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(14, 50));
    ed.runtime = rvim_runtime::RuntimePaths::discover(Some(root.clone()), None);
    ed.tag_index = rvim_editor::tags::TagIndex::load(&root.join("doc/tags"));
    ed.open_file(&main).unwrap();
    // Simulate a startup file replacing 'tags' with a non-existent index.
    ed.execute_ex("set tags=./missing-tags");
    ed.execute_ex("help topic");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.current_buffer().map(|buffer| buffer.buftype.as_str()),
        Some("help")
    );
}
#[test]
fn gutter_options_reserve_columns_and_render() {
    let path = temp_path("gutter.txt");
    std::fs::write(&path, b"abc\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 50));
    ed.open_file(&path).unwrap();
    ed.messages.clear();
    let base_width = ed.view.current_window().unwrap().text_width();
    ed.execute_ex("set foldcolumn=2");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.view.current_window().unwrap().text_width(),
        base_width - 2
    );
    ed.execute_ex("set signcolumn=yes");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    assert_eq!(
        ed.view.current_window().unwrap().text_width(),
        base_width - 4
    );
    let numberwidth = ed
        .options
        .window_number(ed.view.current_window().unwrap().id.raw(), "numberwidth")
        .clamp(1, 20) as u16;
    ed.execute_ex("set signcolumn=number");
    assert_eq!(
        ed.view.current_window().unwrap().text_width(),
        base_width - 2 - (numberwidth + 1)
    );
    ed.execute_ex("set signcolumn=auto");
    assert_eq!(
        ed.view.current_window().unwrap().text_width(),
        base_width - 2
    );
    ed.execute_ex("set number signcolumn=yes foldcolumn=2");
    let frame = ed.render();
    let mut text_col = None;
    for col in 0..40u16 {
        if frame
            .base()
            .cell(0, col)
            .is_some_and(|cell| cell.text == "a")
        {
            text_col = Some(col);
            break;
        }
    }
    assert_eq!(text_col, Some(9), "text should start after the gutters");
    for col in 0..4u16 {
        let cell = frame.base().cell(0, col).unwrap();
        assert!(
            cell.text.is_empty() || cell.text == " ",
            "{col}: {:?}",
            cell.text
        );
    }
}
#[test]
fn numberformat_formats_line_numbers() {
    let path = temp_path("numberformat.txt");
    std::fs::write(&path, b"abc\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.execute_ex("set number numberformat=%03d");
    assert!(!ed.messages.error, "{:?}", ed.messages.last);
    let frame = ed.render();
    let row: String = (0..10u16)
        .map(|col| {
            frame
                .base()
                .cell(0, col)
                .map_or(String::new(), |cell| cell.text.clone())
        })
        .collect();
    assert!(row.contains("001"), "{row:?}");
    ed.execute_ex("set numberformat=%d");
    let frame = ed.render();
    let row: String = (0..10u16)
        .map(|col| {
            frame
                .base()
                .cell(0, col)
                .map_or(String::new(), |cell| cell.text.clone())
        })
        .collect();
    assert!(row.contains('1'), "{row:?}");
}
#[test]
fn backward_search_cmdwin_reports_question_type() {
    let path = temp_path("cmdwin-q.txt");
    std::fs::write(&path, b"x\n").unwrap();
    let mut ed = Editor::new(Size::new(12, 40));
    ed.open_file(&path).unwrap();
    ed.messages.clear();
    type_text(&mut ed, "q?");
    assert!(ed.cmdwin.is_some(), "{:?}", ed.messages.last);
    assert_eq!(ed.eval_viml("getcmdwintype()").unwrap().echo_string(), "?");
    ed.handle_key(KeyEvent::with_mods(Key::Char('c'), Mods::CTRL));
    assert!(ed.cmdwin.is_none());
}
