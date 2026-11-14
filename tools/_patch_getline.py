import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "line" => {"""
arms = """            "getline" => {
                let first = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let first = if first == 0 {
                    self.editor.cursor().lnum
                } else {
                    first
                };
                let last = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(first))
                    .unwrap_or(first)
                    .max(first);
                let lines: Vec<Value> = self
                    .editor
                    .current_buffer()
                    .map(|buffer| {
                        (first..=last.min(buffer.text.line_count()))
                            .filter_map(|lnum| {
                                buffer.text.line(lnum).map(|line| {
                                    Value::string(String::from_utf8_lossy(line).into_owned())
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if args.len() > 1 {
                    Ok(Some(Value::list(lines)))
                } else {
                    Ok(Some(
                        lines
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| Value::string("")),
                    ))
                }
            }
            "setline" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let lnum = if lnum == 0 {
                    self.editor.cursor().lnum
                } else {
                    lnum
                };
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
                        entry
                            .text
                            .replace_line(lnum + offset as u32, VimString::from(line.as_str()));
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "line" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
