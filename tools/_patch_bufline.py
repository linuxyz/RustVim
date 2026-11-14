import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "gettabwinvar" => {"""
arms = """            "getbufline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let start = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let end = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(u32::MAX))
                    .unwrap_or(u32::MAX);
                let buffer = resolve_buffer(self.editor, &spec);
                let lines = buffer
                    .and_then(|buffer| self.editor.buffers.get(buffer))
                    .map(|buffer| {
                        (start..=end.min(buffer.text.line_count()))
                            .filter_map(|lnum| {
                                buffer
                                    .text
                                    .line(lnum)
                                    .map(|line| Value::string(String::from_utf8_lossy(line).into_owned()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Some(Value::list(lines)))
            }
            "getbufoneline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let lnum = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let buffer = resolve_buffer(self.editor, &spec);
                let line = buffer
                    .and_then(|buffer| self.editor.buffers.get(buffer))
                    .and_then(|buffer| buffer.text.line(lnum))
                    .map(|line| String::from_utf8_lossy(line).into_owned())
                    .unwrap_or_default();
                Ok(Some(Value::string(line)))
            }
            "setbufline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let lnum = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let buffer = resolve_buffer(self.editor, &spec);
                let Some(buffer) = buffer else {
                    return Ok(Some(Value::Number(1)));
                };
                let lines: Vec<String> = match args.get(2) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    Some(other) => vec![other.echo_string()],
                    None => Vec::new(),
                };
                if let Some(entry) = self.editor.buffers.get_mut(buffer) {
                    for (offset, line) in lines.iter().enumerate() {
                        entry
                            .text
                            .replace_line(lnum + offset as u32, VimString::from(line.as_str()));
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "append" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let lines: Vec<String> = match args.get(1) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    Some(other) => vec![other.echo_string()],
                    None => Vec::new(),
                };
                let buffer = self.editor.current_buffer;
                if let Some(entry) = self.editor.buffers.get_mut(buffer) {
                    for (offset, line) in lines.iter().enumerate() {
                        entry.text.insert_line(
                            lnum + offset as u32 + 1,
                            VimString::from(line.as_str()),
                        );
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "deletebufline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let first = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let last = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(first))
                    .unwrap_or(first)
                    .max(first);
                let buffer = resolve_buffer(self.editor, &spec);
                let Some(buffer) = buffer else {
                    return Ok(Some(Value::Number(1)));
                };
                if let Some(entry) = self.editor.buffers.get_mut(buffer) {
                    for lnum in (first..=last).rev() {
                        entry.text.delete_line(lnum);
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "gettabwinvar" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
s += '''

fn resolve_buffer(editor: &crate::editor::Editor, spec: &str) -> Option<rvim_types::BufferId> {
    if let Ok(number) = spec.parse::<u32>() {
        return editor.buffer_by_number(number);
    }
    editor
        .buffer_ids()
        .into_iter()
        .find(|id| {
            editor.buffers.get(*id).is_some_and(|buffer| {
                let name = buffer.name_string();
                name.as_str() == spec || name.ends_with(spec)
            })
        })
}
'''
p.write_text(s)
print("patched")
