import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub last_substitute_flags: String,
    last_click:""", """    pub last_substitute_flags: String,
    last_change_keys: Vec<KeyEvent>,
    change_keys: Vec<KeyEvent>,
    change_start_tick: u64,
    replaying: bool,
    last_click:""", 1)
s = s.replace("""            last_substitute_flags: String::new(),
            last_click: None,""", """            last_substitute_flags: String::new(),
            last_change_keys: Vec::new(),
            change_keys: Vec::new(),
            change_start_tick: 0,
            replaying: false,
            last_click: None,""", 1)
start = s.index("    fn dispatch_key(&mut self, key: KeyEvent) {")
end = s.index("    pub fn switch_mode(&mut self, mode: Mode) {", start)
new_dispatch = """    fn dispatch_key(&mut self, key: KeyEvent) {
        if !self.replaying && !self.mode.is_command() {
            let at_command_start = matches!(self.mode, Mode::Normal)
                && self.pending_operator.is_none()
                && self.pending_char.is_none()
                && !self.pending_g
                && !self.pending_ctrl_w
                && !self.pending_z
                && self.pending_register.is_none()
                && self.pending_count.is_none();
            if at_command_start {
                self.change_keys.clear();
                self.change_start_tick = self.buffer_tick();
            }
            self.change_keys.push(key.clone());
        }
        match self.mode.clone() {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Insert(state) => self.handle_insert_key(key, state),
            Mode::Command(kind) => self.handle_command_key(key, kind),
            Mode::Visual { .. } => self.handle_visual_key(key),
        }
        self.maybe_finish_change();
    }

"""
s = s[:start] + new_dispatch + s[end:]
old2 = """    fn repeat_change(&mut self) {
        let Some(change) = self.last_change.clone() else {
            return;
        };
        if !change.insert || change.text.is_empty() {
            return;
        }
        let pos = self.cursor();
        self.record_undo();
        if let Some(buffer) = self.current_buffer_mut() {
            let _ = buffer
                .text
                .insert_bytes(pos.lnum, pos.col, change.text.as_bytes());
            buffer.text.touch();
        }
        self.commit_undo();
        self.request_redraw();
    }"""
new2 = """    fn buffer_tick(&self) -> u64 {
        self.current_buffer()
            .map_or(0, |buffer| buffer.text.meta.changedtick)
    }

    fn maybe_finish_change(&mut self) {
        if self.replaying || !matches!(self.mode, Mode::Normal) {
            return;
        }
        if self.pending_operator.is_some()
            || self.pending_char.is_some()
            || self.pending_g
            || self.pending_ctrl_w
            || self.pending_z
            || self.pending_register.is_some()
            || self.pending_count.is_some()
        {
            return;
        }
        if self.change_keys.is_empty() || self.buffer_tick() == self.change_start_tick {
            return;
        }
        self.last_change_keys = std::mem::take(&mut self.change_keys);
        self.change_start_tick = self.buffer_tick();
    }

    fn repeat_change(&mut self, count: u32) {
        if self.last_change_keys.is_empty() {
            let Some(change) = self.last_change.clone() else {
                return;
            };
            if !change.insert || change.text.is_empty() {
                return;
            }
            for _ in 0..count {
                let pos = self.cursor();
                self.record_undo();
                if let Some(buffer) = self.current_buffer_mut() {
                    let _ = buffer
                        .text
                        .insert_bytes(pos.lnum, pos.col, change.text.as_bytes());
                    buffer.text.touch();
                }
                self.commit_undo();
            }
            self.request_redraw();
            return;
        }
        let keys = self.last_change_keys.clone();
        self.replaying = true;
        for _ in 0..count {
            for key in &keys {
                self.handle_key(key.clone());
            }
        }
        self.replaying = false;
        self.change_keys.clear();
        self.change_start_tick = self.buffer_tick();
        self.request_redraw();
    }"""
assert old2 in s, "repeat_change not found"
s = s.replace(old2, new2, 1)
old3 = "'.' => self.repeat_change(),"
new3 = "'.' => {\n                    let count = self.take_count();\n                    self.repeat_change(count);\n                }"
assert old3 in s, "dot call not found"
s = s.replace(old3, new3, 1)
p.write_text(s)
print("patched")
