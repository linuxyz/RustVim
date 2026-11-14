import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        self.mode = Mode::Insert(InsertState {
            start: BufPos::new(line, 0),
            append: false,
            undo_started: true,
        });""", """        self.mode = Mode::Insert(InsertState {
            start: BufPos::new(line, 0),
            append: false,
            replace: false,
            undo_started: true,
        });""")
s = s.replace("""        self.mode = Mode::Insert(InsertState {
            start: pos,
            append: false,
            undo_started: true,
        });""", """        self.mode = Mode::Insert(InsertState {
            start: pos,
            append: false,
            replace: false,
            undo_started: true,
        });""")
s = s.replace("""        if replace {
            // Replace mode is represented as insert until the dedicated mode lands.
        }
""", "")
old = """    fn insert_text(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let pos = self.cursor();
        let new_pos = if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.insert_bytes(pos.lnum, pos.col, bytes)
        } else {
            return;
        };
        self.set_insert_cursor(new_pos);
        if let Some(InsertState { .. }) = &mut None::<InsertState> {}
        self.request_redraw();
    }"""
new = """    fn insert_text(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let replace = matches!(self.mode, Mode::Insert(InsertState { replace: true, .. }));
        let pos = self.cursor();
        if replace && !bytes.contains(&b'\n') {
            let line_len = self
                .current_buffer()
                .map_or(0, |buffer| buffer.text.line_len(pos.lnum));
            let mut end = pos;
            for ch in String::from_utf8_lossy(bytes).chars() {
                if ch == '\n' {
                    break;
                }
                if end.col < line_len {
                    end.col = self.next_boundary(end.lnum, end.col);
                }
            }
            if end != pos {
                if let Some(buffer) = self.current_buffer_mut() {
                    buffer.text.delete_range(pos, end);
                }
            }
        }
        let new_pos = if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.insert_bytes(pos.lnum, pos.col, bytes)
        } else {
            return;
        };
        self.set_insert_cursor(new_pos);
        self.request_redraw();
    }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("ok")
