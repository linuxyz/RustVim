import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "tagfiles" => {"""
arms = """            "gettagstack" => {
                let bufnr = self.editor.buffer_number_of(self.editor.current_buffer_id());
                let mut items = Vec::new();
                for (buffer, pos) in &self.editor.tag_stack {
                    let from_buf = self.editor.buffer_number_of(rvim_types::Id::raw(*buffer));
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("tagname"), Value::string(""));
                    dict.set(
                        VimString::from("from"),
                        Value::list(vec![
                            Value::Number(from_buf as i64),
                            Value::Number(pos.lnum as i64),
                            Value::Number(pos.col as i64 + 1),
                            Value::Number(0),
                        ]),
                    );
                    dict.set(VimString::from("matchnr"), Value::Number(1));
                    items.push(Value::dict(dict));
                }
                let _ = bufnr;
                let mut dict = VimDict::new();
                dict.set(
                    VimString::from("curidx"),
                    Value::Number(self.editor.tag_stack.len() as i64),
                );
                dict.set(VimString::from("items"), Value::list(items));
                Ok(Some(Value::dict(dict)))
            }
            "settagstack" => {
                let action = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "r".to_string());
                if action.starts_with('r') {
                    self.editor.tag_stack.clear();
                }
                if let Some(Value::Dict(dict)) = args.get(1) {
                    let dict = dict.borrow();
                    if let Some(Value::List(items)) = dict.get_str("items") {
                        for item in items.borrow().items.iter() {
                            let Value::Dict(item) = item else {
                                continue;
                            };
                            let item = item.borrow();
                            let Some(Value::List(from)) = item.get_str("from") else {
                                continue;
                            };
                            let values = from.borrow().items.clone();
                            let lnum = values
                                .get(1)
                                .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                                .unwrap_or(1)
                                .max(1);
                            let col = values
                                .get(2)
                                .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                                .unwrap_or(1)
                                .max(1)
                                - 1;
                            let buffer = values
                                .first()
                                .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                                .unwrap_or(0);
                            if let Some(id) = self.editor.buffer_by_number(buffer) {
                                self.editor
                                    .tag_stack
                                    .push((id, BufPos::new(lnum, col)));
                            }
                        }
                    }
                }
                Ok(Some(Value::Number(0)))
            }
            "tagfiles" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
