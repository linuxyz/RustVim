import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "scriptnames" | "scriptn" | "scripts" => {'''
arms = '''        "wviminfo" | "wv" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                crate::viminfo::default_path(editor)
                    .ok_or_else(|| VimError::new(484, "Can't find viminfo file"))?
            } else {
                expand_path_arg(arg)
            };
            crate::viminfo::write(editor, &path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor
                .messages
                .info(format!("written {}", path.display()));
        }
        "rviminfo" | "rv" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                crate::viminfo::default_path(editor)
                    .ok_or_else(|| VimError::new(484, "Can't find viminfo file"))?
            } else {
                expand_path_arg(arg)
            };
            crate::viminfo::read(editor, &path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor
                .messages
                .info(format!("read {}", path.display()));
        }
        "oldfiles" | "ol" => {
            if editor.oldfiles.is_empty() {
                editor.messages.info("No old files".to_string());
            } else {
                let text = editor
                    .oldfiles
                    .iter()
                    .enumerate()
                    .map(|(index, file)| format!("{:>3}: {file}", index + 1))
                    .collect::<Vec<_>>()
                    .join("\n");
                editor.messages.info(text);
            }
        }
        "scriptnames" | "scriptn" | "scripts" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("ex patched")
# registry implemented true
p = pathlib.Path("crates/rvim-editor/src/registry_generated.rs")
s = p.read_text()
for name in ['oldfiles', 'rviminfo', 'wviminfo']:
    old = f'name: "{name}", doc: ":'
    idx = s.find(old)
    assert idx >= 0, name
    end = s.find('implemented: false', idx)
    assert end >= 0
    s = s[:end] + 'implemented: true' + s[end+len('implemented: false'):]
p.write_text(s)
print("registry patched")
