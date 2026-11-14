import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    fn insert_completion_candidates(&self, prefix: &str) -> Vec<String> {"""
methods = """    pub(crate) fn complete_matches(&mut self, startcol: u32, matches: Vec<String>) {
        let cursor = self.cursor();
        let start = BufPos::new(cursor.lnum, startcol.saturating_sub(1));
        self.insert_completion = Some(InsertCompletion {
            start,
            candidates: matches,
            index: 0,
        });
        self.request_redraw();
    }

    pub(crate) fn complete_add_matches(&mut self, matches: Vec<String>) {
        if let Some(state) = &mut self.insert_completion {
            state.candidates.extend(matches);
            self.request_redraw();
        } else {
            self.complete_matches(self.cursor().col + 1, matches);
        }
    }

    pub(crate) fn completion_visible(&self) -> bool {
        self.insert_completion.is_some()
    }

    pub(crate) fn completion_items(&self) -> Vec<String> {
        self.insert_completion
            .as_ref()
            .map(|state| state.candidates.clone())
            .unwrap_or_default()
    }

    fn insert_completion_candidates(&self, prefix: &str) -> Vec<String> {"""
assert anchor in s
s = s.replace(anchor, methods, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor2 = """            "getreg" => {"""
arms = """            "complete" => {
                let startcol = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let matches = match args.get(1) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    _ => Vec::new(),
                };
                self.editor.complete_matches(startcol, matches);
                Ok(Some(Value::Number(0)))
            }
            "complete_add" => {
                let matches = match args.first() {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    Some(other) => vec![other.echo_string()],
                    None => Vec::new(),
                };
                self.editor.complete_add_matches(matches);
                Ok(Some(Value::Number(0)))
            }
            "complete_check" => Ok(Some(Value::Number(0))),
            "pumvisible" => Ok(Some(Value::Number(i64::from(
                self.editor.completion_visible(),
            )))),
            "complete_info" => {
                let items = self
                    .editor
                    .completion_items()
                    .into_iter()
                    .map(|word| {
                        let mut item = VimDict::new();
                        item.set(VimString::from("word"), Value::string(word));
                        Value::dict(item)
                    })
                    .collect();
                let mut dict = VimDict::new();
                dict.set(VimString::from("mode"), Value::string(""));
                dict.set(VimString::from("selected"), Value::Number(0));
                dict.set(VimString::from("items"), Value::list(items));
                Ok(Some(Value::dict(dict)))
            }
            "getreg" => {"""
assert anchor2 in s
s = s.replace(anchor2, arms, 1)
p.write_text(s)
print("host patched")
