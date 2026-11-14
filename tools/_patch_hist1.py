import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub command_history: VecDeque<String>,
    pub command_history_index: usize,""", """    pub command_history: VecDeque<String>,
    pub search_history: VecDeque<String>,
    pub command_history_index: usize,""", 1)
s = s.replace("""            command_history: VecDeque::new(),
            command_history_index: 0,""", """            command_history: VecDeque::new(),
            search_history: VecDeque::new(),
            command_history_index: 0,""", 1)
# push search pattern in set_search/set_search_direction
s = s.replace("""        self.search_pattern = Some(pattern.to_string());
        self.search_highlight_active = true;
        self.last_search = Some((pattern.to_string(), 1));""", """        self.search_pattern = Some(pattern.to_string());
        self.search_highlight_active = true;
        self.push_search_history(pattern);
        self.last_search = Some((pattern.to_string(), 1));""", 1)
s = s.replace("""        self.search_pattern = Some(pattern.to_string());
        self.search_highlight_active = true;
        self.last_search = Some((pattern.to_string(), if forward { 1 } else { -1 }));""", """        self.search_pattern = Some(pattern.to_string());
        self.search_highlight_active = true;
        self.push_search_history(pattern);
        self.last_search = Some((pattern.to_string(), if forward { 1 } else { -1 }));""", 1)
# add methods before set_search
anchor = """    fn set_search(&mut self, pattern: &str) {"""
methods = """    fn push_search_history(&mut self, pattern: &str) {
        if pattern.is_empty() || self.search_history.back().is_some_and(|last| last == pattern) {
            return;
        }
        self.search_history.push_back(pattern.to_string());
        let limit = self.options.get_number("history").max(0) as usize;
        while self.search_history.len() > limit {
            self.search_history.pop_front();
        }
    }

    pub(crate) fn history(&self, kind: &str) -> Option<&VecDeque<String>> {
        match kind {
            ":" | "cmd" | "command" => Some(&self.command_history),
            "/" | "search" => Some(&self.search_history),
            _ => None,
        }
    }

    pub(crate) fn history_mut(&mut self, kind: &str) -> Option<&mut VecDeque<String>> {
        match kind {
            ":" | "cmd" | "command" => Some(&mut self.command_history),
            "/" | "search" => Some(&mut self.search_history),
            _ => None,
        }
    }

    fn set_search(&mut self, pattern: &str) {"""
assert anchor in s
s = s.replace(anchor, methods, 1)
p.write_text(s)
print("editor patched")
