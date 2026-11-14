import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getpos" => {"""
arms = """            "getcharpos" => {
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
                } else if let Some(mark) = expr.trim_start_matches('\\'').chars().next() {
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
                let line = self.editor.line(pos.lnum).unwrap_or(b"");
                let char_col = character_index(line, pos.col) + 1;
                let bufnr = self.editor.buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(pos.lnum as i64),
                    Value::Number(char_col as i64),
                    Value::Number(0),
                ])))
            }
            "setcharpos" => {
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
                let char_col = values.get(2).copied().unwrap_or(1).max(1) as u32;
                let line = self.editor.line(lnum).unwrap_or(b"").to_vec();
                let col = byte_column(&line, char_col.saturating_sub(1));
                let pos = BufPos::new(lnum, col);
                if let Some(mark) = expr.strip_prefix('\\'') {
                    let mark = mark.chars().next().unwrap_or('\\'');
                    self.editor.set_mark_position(mark, pos);
                } else if expr == "." {
                    self.editor.set_cursor(pos);
                }
                Ok(Some(Value::Number(0)))
            }
            "getcursorcharpos" => {
                let cursor = self.editor.cursor();
                let line = self.editor.line(cursor.lnum).unwrap_or(b"");
                let char_col = character_index(line, cursor.col) + 1;
                let bufnr = self.editor.buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(cursor.lnum as i64),
                    Value::Number(char_col as i64),
                    Value::Number(0),
                ])))
            }
            "setcursorcharpos" => {
                let Some(Value::List(list)) = args.first() else {
                    return Ok(Some(Value::Number(-1)));
                };
                let values: Vec<i64> = list
                    .borrow()
                    .items
                    .iter()
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(0))
                    .collect();
                let lnum = values.get(1).copied().unwrap_or(0).max(1) as u32;
                let char_col = values.get(2).copied().unwrap_or(1).max(1) as u32;
                let line = self.editor.line(lnum).unwrap_or(b"").to_vec();
                let col = byte_column(&line, char_col.saturating_sub(1));
                self.editor.set_cursor(BufPos::new(lnum, col));
                Ok(Some(Value::Number(0)))
            }
            "getpos" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
s += '''

fn character_index(line: &[u8], byte_col: u32) -> u32 {
    let end = (byte_col as usize).min(line.len());
    let mut index = 0u32;
    let mut position = 0usize;
    while position < end {
        let mut next = position + 1;
        while next < end && (line[next] & 0xC0) == 0x80 {
            next += 1;
        }
        position = next;
        index += 1;
    }
    index
}

fn byte_column(line: &[u8], char_col: u32) -> u32 {
    let mut index = 0u32;
    let mut position = 0usize;
    while position < line.len() {
        if index >= char_col {
            break;
        }
        position += 1;
        while position < line.len() && (line[position] & 0xC0) == 0x80 {
            position += 1;
        }
        index += 1;
    }
    position.min(line.len()) as u32
}
'''
p.write_text(s)
print("patched")
