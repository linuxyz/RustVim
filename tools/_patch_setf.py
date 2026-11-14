import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "filetype" | "filet" | "filety" => {'''
arm = '''        "setf" | "setfiletype" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            if command.name == "setfiletype" {
                let current = editor
                    .options
                    .buffer_string(editor.current_buffer_id(), "filetype");
                if !current.is_empty() {
                    return Ok(());
                }
            }
            let buffer_id = editor.current_buffer_id();
            editor
                .options
                .set_buffer(
                    buffer_id,
                    "filetype",
                    crate::options::OptionValue::String(VimString::from(name)),
                )
                .map_err(VimError::plain)?;
            editor.refresh_filetype();
        }
        "filetype" | "filet" | "filety" => {'''
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
