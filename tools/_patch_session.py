import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "source" | "so" | "sourc" => {'''
arms = '''        "mksession" | "mks" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                std::path::PathBuf::from("Session.vim")
            } else {
                resolve_file_arg(editor, arg)?
            };
            let script = session_script(editor);
            rvim_platform::fs::write_file_atomic(&path, script.as_bytes())
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor.messages.info(format!("session written to {}", path.display()));
        }
        "mkview" => {
            let arg = command.args.trim();
            let path = view_path(editor, arg)?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let pos = editor.cursor();
            let script = format!(
                "\" RVim view\ncall cursor({}, {})\n",
                pos.lnum,
                pos.col + 1
            );
            rvim_platform::fs::write_file_atomic(&path, script.as_bytes())
                .map_err(|error| VimError::new(13, error.to_string()))?;
        }
        "loadview" => {
            let arg = command.args.trim();
            let path = view_path(editor, arg)?;
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor.source_viml(&text, &path.display().to_string())?;
        }
        "source" | "so" | "sourc" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
# helpers at end
s += '''

fn ex_escape_path(path: &str) -> String {
    path.replace('\\\\', "\\\\\\\\").replace(' ', "\\\\ ")
}

fn session_script(editor: &Editor) -> String {
    let mut out = String::from("\\\" RVim session\\nset shortmess=aoO\\nsilent only\\n");
    out.push_str(&format!(
        "cd {}\\n",
        ex_escape_path(&rvim_platform::fs::cwd().display().to_string())
    ));
    let tabs = editor.view.tab_ids();
    for (tab_index, tab) in tabs.iter().enumerate() {
        if tab_index > 0 {
            out.push_str("tabnew\\n");
        }
        let Some(page) = editor.view.tabs.get(tab) else {
            continue;
        };
        let windows = rvim_view::layout::all_windows(&page.root);
        for (window_index, window_id) in windows.iter().enumerate() {
            if window_index > 0 {
                out.push_str("split\\n");
            }
            let Some(window) = editor.view.windows.get(window_id) else {
                continue;
            };
            let name = editor
                .buffers
                .get(window.buffer)
                .and_then(|buffer| buffer.text.meta.name.as_ref())
                .map(|path| path.display().to_string());
            match name {
                Some(name) => out.push_str(&format!("edit {}\\n", ex_escape_path(&name))),
                None => out.push_str("enew\\n"),
            }
        }
    }
    out
}

fn view_path(editor: &Editor, arg: &str) -> Result<std::path::PathBuf, VimError> {
    if !arg.is_empty() {
        return Ok(resolve_file_arg(editor, arg)?);
    }
    let viewdir = editor.options.get_string("viewdir");
    let dir = rvim_platform::fs::expand_env(if viewdir.is_empty() {
        "~/.vim/view"
    } else {
        &viewdir
    });
    let name = editor
        .current_buffer()
        .and_then(|buffer| buffer.text.meta.name.as_ref())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unnamed".to_string());
    let name = name.replace('/', "%");
    Ok(dir.join(format!("{name}.vim")))
}
'''
p.write_text(s)
print("ex patched")
# cursor builtin
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor2 = '''            "col" => {'''
add2 = '''            "cursor" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let col = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                self.editor.set_cursor(rvim_types::BufPos::new(lnum.max(1), col.saturating_sub(1)));
                Ok(Some(Value::Number(0)))
            }
            "col" => {'''
assert anchor2 in s
s = s.replace(anchor2, add2, 1)
p.write_text(s)
print("host patched")
