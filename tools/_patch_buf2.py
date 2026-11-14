import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
old = '''        "badd" | "ba" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            editor.add_buffer(Some(&path))?;
        }
        "bdelete" | "bd" | "bwipeout" | "bw" => {
            let id = editor.current_buffer;
            editor.delete_buffer(id, command.bang)?;
        }'''
new = '''        "badd" | "ba" | "bufadd" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            editor.add_unlisted_buffer(Some(&path))?;
        }
        "bufload" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.load_buffer(id)?;
        }
        "bdelete" | "bd" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang)?;
        }
        "bwipeout" | "bw" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.wipe_buffer(id, command.bang)?;
        }
        "bunload" | "bun" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang)?;
        }'''
assert old in s
s = s.replace(old, new, 1)
# helper before resolve_file_arg
anchor = '''fn resolve_file_arg(editor: &Editor, arg: &str) -> Result<std::path::PathBuf, VimError> {'''
helper = '''fn buffer_argument(editor: &Editor, arg: &str) -> Result<rvim_types::BufferId, VimError> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Ok(editor.current_buffer);
    }
    if let Ok(number) = arg.parse::<u32>() {
        return editor
            .buffer_by_number(number)
            .ok_or_else(|| VimError::new(94, "No matching buffer"));
    }
    let path = expand_path_arg(arg);
    editor
        .buffer_ids()
        .into_iter()
        .find(|id| {
            editor
                .buffers
                .get(*id)
                .and_then(|buffer| buffer.text.meta.name.as_ref())
                .is_some_and(|name| name == &path || name.file_name() == path.file_name())
        })
        .ok_or_else(|| VimError::new(94, "No matching buffer"))
}

fn resolve_file_arg(editor: &Editor, arg: &str) -> Result<std::path::PathBuf, VimError> {'''
assert anchor in s
s = s.replace(anchor, helper, 1)
p.write_text(s)
print("patched")
