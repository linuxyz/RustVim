import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    fn resolve_mark(&self, mark: char) -> Option<BufPos> {"""
wrappers = """    pub(crate) fn mark_position(&self, mark: char) -> Option<BufPos> {
        self.resolve_mark(mark)
    }

    pub(crate) fn set_mark_position(&mut self, mark: char, pos: BufPos) {
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.marks.insert(mark, pos);
        }
    }

    pub(crate) fn buffer_number_of(&self, id: BufferId) -> u32 {
        self.buffer_ids()
            .iter()
            .position(|buffer| *buffer == id)
            .map_or(0, |index| index as u32 + 1)
    }

    fn resolve_mark(&self, mark: char) -> Option<BufPos> {"""
assert anchor in s
s = s.replace(anchor, wrappers, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor2 = """            "confirm" => {"""
arms = """            "getpos" => {
                let expr = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let pos = if expr == "." {
                    Some(self.editor.cursor())
                } else if expr == "$" {
                    Some(BufPos::new(self.editor.line_count().max(1), 0))
                } else if expr == "^" {
                    Some(BufPos::new(1, 0))
                } else if let Some(mark) = expr.trim_start_matches('\'').chars().next() {
                    self.editor.mark_position(mark)
                } else {
                    None
                };
                let Some(pos) = pos else {
                    return Ok(Some(Value::list(vec![
                        Value::Number(0),
                        Value::Number(0),
                        Value::Number(0),
                        Value::Number(0),
                    ])));
                };
                let bufnr = self.editor.buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(pos.lnum as i64),
                    Value::Number(pos.col as i64 + 1),
                    Value::Number(0),
                ])))
            }
            "getcurpos" => {
                let cursor = self.editor.cursor();
                let bufnr = self.editor.buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(cursor.lnum as i64),
                    Value::Number(cursor.col as i64 + 1),
                    Value::Number(0),
                    Value::Number(0),
                ])))
            }
            "getmousepos" => {
                let mut dict = VimDict::new();
                dict.set(VimString::from("screenrow"), Value::Number(0));
                dict.set(VimString::from("screencol"), Value::Number(0));
                dict.set(VimString::from("winid"), Value::Number(0));
                dict.set(VimString::from("winrow"), Value::Number(0));
                dict.set(VimString::from("wincol"), Value::Number(0));
                Ok(Some(Value::dict(dict)))
            }
            "setpos" => {
                let expr = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let Some(Value::List(list)) = args.get(1) else {
                    return Ok(Some(Value::Number(-1)));
                };
                let values: Vec<i64> = list
                    .borrow()
                    .items
                    .iter()
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(0))
                    .collect();
                let lnum = values.get(1).copied().unwrap_or(0).max(1) as u32;
                let col = values.get(2).copied().unwrap_or(1).max(1) as u32 - 1;
                let pos = BufPos::new(lnum, col);
                if let Some(mark) = expr.strip_prefix('\'') {
                    let mark = mark.chars().next().unwrap_or('\'');
                    self.editor.set_mark_position(mark, pos);
                } else if expr == "." {
                    self.editor.set_cursor(pos);
                }
                Ok(Some(Value::Number(0)))
            }
            "confirm" => {"""
assert anchor2 in s
s = s.replace(anchor2, arms, 1)
p.write_text(s)
print("host patched")
