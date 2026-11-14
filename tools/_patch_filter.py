import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
old = """        "!" => {
            let command_text = expand_shell_command(editor, command.args.trim());
            if command_text.is_empty() {
                return Err(VimError::plain("E471: Argument required"));
            }
            editor.last_bang = command_text.clone();
            let output = editor.run_shell(&command_text, None)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            if !output.stderr.is_empty() {
                text.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            editor.messages.info(text.trim_end().to_string());
        }"""
new = """        "!" => {
            let command_text = expand_shell_command(editor, command.args.trim());
            if command_text.is_empty() {
                return Err(VimError::plain("E471: Argument required"));
            }
            editor.last_bang = command_text.clone();
            if !command.range.addresses.is_empty() {
                let range = resolve_range(editor, &command.range)?;
                let mut input = String::new();
                for lnum in range.start..=range.end {
                    if let Some(line) = editor.line(lnum) {
                        input.push_str(&String::from_utf8_lossy(line));
                    }
                    input.push('\n');
                }
                let output = editor
                    .run_shell(&command_text, Some(input.as_bytes()))
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                let mut replacement = output.stdout;
                if replacement.is_empty() {
                    replacement.push(b'\n');
                } else if !replacement.ends_with(b"\n") {
                    replacement.push(b'\n');
                }
                let start = range.start;
                let end = range.end;
                let line_count = editor.line_count();
                let delete_end = if end < line_count {
                    BufPos::new(end + 1, 0)
                } else {
                    let len = editor
                        .current_buffer()
                        .map_or(0, |buffer| buffer.text.line_len(end));
                    BufPos::new(end, len)
                };
                if let Some(buffer) = editor.current_buffer_mut() {
                    buffer.text.delete_range(BufPos::new(start, 0), delete_end);
                    buffer.text.insert_bytes(start, 0, &replacement);
                }
                editor.set_cursor(BufPos::new(start, 0));
                if !output.stderr.is_empty() {
                    editor
                        .messages
                        .info(String::from_utf8_lossy(&output.stderr).trim_end().to_string());
                }
            } else {
                let output = editor
                    .run_shell(&command_text, None)
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
                if !output.stderr.is_empty() {
                    text.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                editor.messages.info(text.trim_end().to_string());
            }
        }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
