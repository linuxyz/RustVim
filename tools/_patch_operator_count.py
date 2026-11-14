import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub pending_count: Option<u32>,""", """    pub pending_count: Option<u32>,
    pub operator_count: Option<u32>,""", 1)
s = s.replace("""            pending_count: None,""", """            pending_count: None,
            operator_count: None,""", 1)
# showcmd count
s = s.replace("""        if let Some(count) = self.pending_count {
            out.push_str(&count.to_string());
        }""", """        if let Some(count) = self.pending_count.or(self.operator_count) {
            out.push_str(&count.to_string());
        }""", 1)
# begin_operator capture
s = s.replace("""        if op != Operator::Yank && self.deny_modification() {
            return;
        }
        self.pending_operator = Some(op);""", """        if op != Operator::Yank && self.deny_modification() {
            return;
        }
        let count = self.pending_count.take();
        self.operator_count = count;
        self.pending_operator = Some(op);""", 1)
# handle_operator count
s = s.replace("""        let count = self.take_count();
        if ch.is_ascii_digit() && !(ch == '0' && self.pending_count.is_none()) {""", """        let count = self
            .operator_count
            .take()
            .or_else(|| self.pending_count.take())
            .unwrap_or(1)
            .max(1);
        if ch.is_ascii_digit() && !(ch == '0' && self.pending_count.is_none()) {""", 1)
# g pending store count
s = s.replace("""            'g' => {
                self.pending_g = true;
                self.pending_operator = Some(op);
                return;
            }""", """            'g' => {
                self.pending_g = true;
                self.operator_count = Some(count);
                self.pending_operator = Some(op);
                return;
            }""", 1)
# reset operator_count alongside pending_count resets (3 occurrences around 1200)
s = s.replace("""                self.pending_count = None;
            }""", """                self.pending_count = None;
                self.operator_count = None;
            }""", 1)
p.write_text(s)
print("patched")
