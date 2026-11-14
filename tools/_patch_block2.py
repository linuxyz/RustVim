import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
# ctrl-v in normal
s = s.replace("""            'w' => self.pending_ctrl_w = true,""", """            'v' => {
                self.mode = Mode::Visual {
                    linewise: false,
                    block: true,
                    start: self.cursor(),
                };
            }
            'w' => self.pending_ctrl_w = true,""", 1)
# visual ctrl handling
s = s.replace("""        if let Key::Char(ch) = key.key {
            if key.mods.contains(Mods::CTRL) {
                return;
            }""", """        if let Key::Char(ch) = key.key {
            if key.mods.contains(Mods::CTRL) {
                if ch == 'v' {
                    if let Mode::Visual { start, .. } = self.mode {
                        self.mode = Mode::Visual {
                            linewise: false,
                            block: true,
                            start,
                        };
                        self.request_redraw();
                    }
                }
                return;
            }""", 1)
# V toggle should keep block false already
# block operator in apply_operator_range
old = """    fn apply_operator_range(&mut self, op: Operator, range: rvim_edit::TextRange) {
        self.record_undo();
        match op {"""
new = """    fn apply_operator_range(&mut self, op: Operator, range: rvim_edit::TextRange) {
        self.record_undo();
        if range.kind == rvim_edit::RangeKind::Blockwise {
            self.apply_block_operator(op, range);
            self.commit_undo();
            self.request_redraw();
            return;
        }
        match op {"""
assert old in s
s = s.replace(old, new, 1)
# add block helper before apply_operator_range
anchor = """    fn apply_operator_range(&mut self, op: Operator, range: rvim_edit::TextRange) {"""
helper = """    fn block_segments(&self, range: rvim_edit::TextRange) -> Vec<(u32, u32, u32)> {
        let Some(buffer) = self.current_buffer() else {
            return Vec::new();
        };
        let start_col = range.start.col.min(range.end.col);
        let end_col = range.start.col.max(range.end.col);
        let mut segments = Vec::new();
        for lnum in range.start.lnum..=range.end.lnum {
            let len = buffer.text.line_len(lnum);
            let start = start_col.min(len);
            let end = (end_col.saturating_add(1)).min(len);
            if end > start {
                segments.push((lnum, start, end));
            }
        }
        segments
    }

    fn apply_block_operator(&mut self, op: Operator, range: rvim_edit::TextRange) {
        let segments = self.block_segments(range);
        let lines: Vec<VimString> = segments
            .iter()
            .filter_map(|(lnum, start, end)| {
                self.current_buffer()
                    .and_then(|buffer| buffer.text.line(*lnum))
                    .map(|line| VimString::from(&line[*start as usize..*end as usize]))
            })
            .collect();
        match op {
            Operator::Yank => {
                self.set_register(RegisterKind::Blockwise, lines, true);
                self.set_cursor(range.start);
            }
            Operator::Delete | Operator::Change => {
                self.set_register(RegisterKind::Blockwise, lines, false);
                for (lnum, start, end) in segments.iter().rev() {
                    if let Some(buffer) = self.current_buffer_mut() {
                        buffer.text.delete_range(
                            BufPos::new(*lnum, *start),
                            BufPos::new(*lnum, *end),
                        );
                    }
                }
                let len = self
                    .current_buffer()
                    .map_or(0, |buffer| buffer.text.line_len(range.start.lnum));
                self.set_cursor(BufPos::new(range.start.lnum, range.start.col.min(len)));
                if op == Operator::Change {
                    self.mode = Mode::Insert(InsertState {
                        start: self.cursor(),
                        append: false,
                        replace: false,
                        undo_started: true,
                    });
                }
            }
            Operator::UpperCase | Operator::LowerCase => {
                for (lnum, start, end) in segments {
                    if let Some(buffer) = self.current_buffer_mut() {
                        let line = buffer.text.line(lnum).unwrap_or(b"").to_vec();
                        let mut text = String::from_utf8_lossy(&line[start as usize..end as usize])
                            .into_owned();
                        text = if op == Operator::UpperCase {
                            text.to_uppercase()
                        } else {
                            text.to_lowercase()
                        };
                        let _ = buffer.text.replace_range(
                            BufPos::new(lnum, start),
                            BufPos::new(lnum, end),
                            text.as_bytes(),
                        );
                    }
                }
                self.set_cursor(range.start);
            }
            _ => {}
        }
    }

    fn apply_operator_range(&mut self, op: Operator, range: rvim_edit::TextRange) {"""
assert anchor in s
s = s.replace(anchor, helper, 1)
p.write_text(s)
print("editor patched")
# view selection block
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
old = """    if selection.linewise {
        return lnum >= start.lnum && lnum <= end.lnum;
    }"""
new = """    if selection.linewise {
        return lnum >= start.lnum && lnum <= end.lnum;
    }
    if selection.block {
        let start_col = start.col.min(end.col);
        let end_col = start.col.max(end.col);
        return lnum >= start.lnum && lnum <= end.lnum && byte >= start_col && byte <= end_col;
    }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("view patched")
