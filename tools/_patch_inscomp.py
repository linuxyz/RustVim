import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("use std::collections::{HashMap, VecDeque};", "use std::collections::{HashMap, HashSet, VecDeque};", 1)
# struct near InsertState
s = s.replace("""pub struct InsertState {
    pub start: BufPos,
    pub append: bool,
    pub replace: bool,
    pub undo_started: bool,
}""", """pub struct InsertState {
    pub start: BufPos,
    pub append: bool,
    pub replace: bool,
    pub undo_started: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct InsertCompletion {
    start: BufPos,
    candidates: Vec<String>,
    index: usize,
}""", 1)
# field
s = s.replace("""    pub(crate) redir_target: Option<RedirTarget>,""", """    insert_completion: Option<InsertCompletion>,
    pub(crate) redir_target: Option<RedirTarget>,""", 1)
s = s.replace("""            redir_target: None,""", """            insert_completion: None,
            redir_target: None,""", 1)
# add n/p to insert_ctrl match
s = s.replace("""            'r' => {\n                self.insert_register_pending = true;\n            }""", """            'n' => self.complete_insert(true),\n            'p' => self.complete_insert(false),\n            'r' => {\n                self.insert_register_pending = true;\n            }""", 1)
# add methods before insert_ctrl
anchor = """    fn insert_ctrl(&mut self, ch: char) {"""
methods = """    fn insert_completion_candidates(&self, prefix: &str) -> Vec<String> {
        let mut words = HashSet::new();
        for lnum in 1..=self.line_count() {
            let Some(line) = self.current_buffer().and_then(|buffer| buffer.text.line(lnum))
            else {
                continue;
            };
            let mut start = None;
            for (index, byte) in line.iter().enumerate() {
                if byte.is_ascii_alphanumeric() || *byte == b'_' {
                    if start.is_none() {
                        start = Some(index);
                    }
                } else if let Some(begin) = start.take() {
                    let word = String::from_utf8_lossy(&line[begin..index]).into_owned();
                    if word.starts_with(prefix) && word != prefix {
                        words.insert(word);
                    }
                }
            }
            if let Some(begin) = start {
                let word = String::from_utf8_lossy(&line[begin..]).into_owned();
                if word.starts_with(prefix) && word != prefix {
                    words.insert(word);
                }
            }
        }
        let mut words: Vec<String> = words.into_iter().collect();
        words.sort();
        words
    }

    fn complete_insert(&mut self, forward: bool) {
        if let Some(mut state) = self.insert_completion.take() {
            if !state.candidates.is_empty() {
                let len = state.candidates.len();
                state.index = if forward {
                    (state.index + 1) % len
                } else {
                    (state.index + len - 1) % len
                };
                let candidate = state.candidates[state.index].clone();
                let pos = self.cursor();
                self.delete_insert_range(state.start, pos);
                self.insert_text(candidate.as_bytes());
                self.insert_completion = Some(state);
            }
            return;
        }
        let pos = self.cursor();
        let line = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .unwrap_or(b"");
        let mut start = pos.col as usize;
        while start > 0 && (line[start - 1].is_ascii_alphanumeric() || line[start - 1] == b'_') {
            start -= 1;
        }
        if start == pos.col as usize {
            return;
        }
        let prefix = String::from_utf8_lossy(&line[start..pos.col as usize]).into_owned();
        let candidates = self.insert_completion_candidates(&prefix);
        if candidates.is_empty() {
            return;
        }
        let index = if forward { 0 } else { candidates.len() - 1 };
        let candidate = candidates[index].clone();
        let start = BufPos::new(pos.lnum, start as u32);
        self.delete_insert_range(start, pos);
        self.insert_text(candidate.as_bytes());
        self.insert_completion = Some(InsertCompletion {
            start,
            candidates,
            index,
        });
    }

    fn insert_ctrl(&mut self, ch: char) {"""
assert anchor in s
s = s.replace(anchor, methods, 1)
# clear on finish insert
s = s.replace("""    fn finish_insert(&mut self) {\n        let insert_start = match self.mode {""", """    fn finish_insert(&mut self) {\n        self.insert_completion = None;\n        let insert_start = match self.mode {""", 1)
p.write_text(s)
print("patched")
