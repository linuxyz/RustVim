import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    insert_completion: Option<InsertCompletion>,""", """    insert_completion: Option<InsertCompletion>,
    insert_ctrl_x: bool,""", 1)
s = s.replace("""            insert_completion: None,""", """            insert_completion: None,
            insert_ctrl_x: false,""", 1)
# top of handle_insert_key
anchor = """    fn handle_insert_key(&mut self, key: KeyEvent, _state: InsertState) {"""
add = """    fn handle_insert_key(&mut self, key: KeyEvent, _state: InsertState) {
        if self.insert_ctrl_x {
            self.insert_ctrl_x = false;
            match key.key {
                Key::Char('u') => self.complete_from_func(\"completefunc\"),
                Key::Char('o') => self.complete_from_func(\"omnifunc\"),
                _ => {}
            }
            return;
        }"""
assert anchor in s
s = s.replace(anchor, add, 1)
# add x in insert_ctrl
s = s.replace("""            'n' => self.complete_insert(true),""", """            'x' => self.insert_ctrl_x = true,\n            'n' => self.complete_insert(true),""", 1)
# add complete_from_func before insert_completion_candidates
anchor2 = """    fn insert_completion_candidates(&self, prefix: &str) -> Vec<String> {"""
method = """    fn complete_from_func(&mut self, option: &str) {
        let name = self
            .options
            .buffer_string(self.current_buffer_id(), option);
        if name.is_empty() {
            return;
        }
        let result = self.call_viml_function(
            &name,
            vec![
                rvim_viml::Value::Number(1),
                rvim_viml::Value::string(String::new()),
            ],
        );
        let start = match result {
            Ok(value) => value.echo_string().parse::<i64>().unwrap_or(-1),
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        if start < 0 {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let start = (start as usize).min(pos.col as usize).min(line.len());
        let base = String::from_utf8_lossy(&line[start..pos.col as usize]).into_owned();
        let result = self.call_viml_function(
            &name,
            vec![rvim_viml::Value::Number(0), rvim_viml::Value::string(base)],
        );
        let candidates: Vec<String> = match result {
            Ok(value) => value
                .as_list()
                .map(|list| {
                    list.borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect()
                })
                .unwrap_or_default(),
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        if candidates.is_empty() {
            return;
        }
        let start = BufPos::new(pos.lnum, start as u32);
        self.delete_insert_range(start, pos);
        self.insert_text(candidates[0].as_bytes());
        self.insert_completion = Some(InsertCompletion {
            start,
            candidates,
            index: 0,
        });
    }

    fn insert_completion_candidates(&self, prefix: &str) -> Vec<String> {"""
assert anchor2 in s
s = s.replace(anchor2, method, 1)
# finish insert clear ctrl_x
s = s.replace("""    fn finish_insert(&mut self) {\n        self.insert_completion = None;""", """    fn finish_insert(&mut self) {\n        self.insert_completion = None;\n        self.insert_ctrl_x = false;""", 1)
p.write_text(s)
print("patched")
