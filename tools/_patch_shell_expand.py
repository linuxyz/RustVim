import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
start = s.index("fn expand_shell_command(editor: &Editor, command: &str) -> String {")
end = s.index("fn parse_color(value: &str)", start)
new = '''fn expand_shell_command(editor: &Editor, command: &str) -> String {
    if command.is_empty() || command == "!" {
        return editor.last_bang.clone();
    }
    let bytes = command.as_bytes();
    let mut out = String::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '\\\\' && index + 1 < bytes.len() {
            let next = bytes[index + 1] as char;
            if matches!(next, '%' | '#' | '!') {
                out.push(next);
                index += 2;
                continue;
            }
        }
        if ch == '!' && bytes.get(index + 1) == Some(&b'!') {
            out.push_str(&editor.last_bang);
            index += 2;
            continue;
        }
        if ch == '%' || ch == '#' {
            let path = if ch == '%' {
                editor
                    .current_buffer()
                    .and_then(|buffer| buffer.text.meta.name.clone())
            } else {
                editor
                    .alternate_buffer
                    .and_then(|id| editor.buffers.get(id))
                    .and_then(|buffer| buffer.text.meta.name.clone())
            };
            if let Some(path) = path {
                let mut end = index + 1;
                let mut modifiers = String::new();
                if bytes.get(end) == Some(&b':') {
                    end += 1;
                    while end < bytes.len() {
                        let modifier = bytes[end] as char;
                        if modifier.is_ascii_alphanumeric() || modifier == ':' {
                            modifiers.push(modifier);
                            end += 1;
                        } else {
                            break;
                        }
                    }
                }
                let path = apply_path_modifiers(path, &modifiers);
                out.push_str(&shell_quote(&path.display().to_string()));
                index = end;
                continue;
            }
        }
        out.push(ch);
        index += 2usize.saturating_sub(1);
        index = index;
    }
    out
}

'''
# fix weird index increment: use one
new = new.replace("        index += 2usize.saturating_sub(1);\n        index = index;\n", "        index += 1;\n")
s = s[:start] + new + s[end:]
p.write_text(s)
print("patched")
