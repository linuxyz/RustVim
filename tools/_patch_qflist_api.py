import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "gettabinfo" => {"""
arms = """            "getqflist" => {
                let mut entries = Vec::new();
                for (index, entry) in self.editor.quickfix.entries.iter().enumerate() {
                    let bufnr = self
                        .editor
                        .buffers
                        .iter()
                        .position(|(_, buffer)| buffer.name_string() == entry.file.display().to_string())
                        .map_or(0, |index| index + 1);
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("bufnr"), Value::Number(bufnr as i64));
                    dict.set(
                        VimString::from("filename"),
                        Value::string(entry.file.display().to_string()),
                    );
                    dict.set(VimString::from("lnum"), Value::Number(entry.lnum as i64));
                    dict.set(VimString::from("col"), Value::Number(entry.col as i64));
                    dict.set(VimString::from("text"), Value::string(entry.text.as_str()));
                    dict.set(VimString::from("valid"), Value::Number(1));
                    dict.set(VimString::from("nr"), Value::Number(index as i64 + 1));
                    let kind = entry.kind.map_or(0, |kind| kind as i64);
                    dict.set(VimString::from("type"), Value::Number(kind));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "setqflist" => {
                let Some(Value::List(list)) = args.first() else {
                    return Ok(Some(Value::Number(-1)));
                };
                let items = list.borrow().items.clone();
                let mut parsed = Vec::new();
                for item in items {
                    let Value::Dict(dict) = item else {
                        continue;
                    };
                    let dict = dict.borrow();
                    let filename = dict
                        .get_str("filename")
                        .map(|value| value.echo_string())
                        .unwrap_or_default();
                    if filename.is_empty() {
                        continue;
                    }
                    let lnum = dict
                        .get_str("lnum")
                        .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                        .unwrap_or(0);
                    let col = dict
                        .get_str("col")
                        .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                        .unwrap_or(0);
                    let text = dict
                        .get_str("text")
                        .map(|value| value.echo_string())
                        .unwrap_or_default();
                    let kind = dict
                        .get_str("type")
                        .and_then(|value| value.echo_string().chars().next());
                    parsed.push(crate::qf::QfEntry {
                        file: std::path::PathBuf::from(filename),
                        lnum: lnum.max(1),
                        col: col.max(1),
                        text,
                        kind,
                    });
                }
                let action = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let title = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "setqflist()".to_string());
                if action.starts_with('a') {
                    let mut combined = self.editor.quickfix.entries.clone();
                    combined.extend(parsed);
                    self.editor.set_quickfix(title, combined);
                } else {
                    self.editor.set_quickfix(title, parsed);
                }
                Ok(Some(Value::Number(0)))
            }
            "gettabinfo" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
