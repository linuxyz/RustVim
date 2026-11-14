import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub fed_keys: std::collections::VecDeque<rvim_viml::Value>,""", """    pub fed_keys: std::collections::VecDeque<rvim_viml::Value>,
    pub sourced_scripts: Vec<(u64, String)>,
    next_script_id: u64,""", 1)
s = s.replace("""            fed_keys: std::collections::VecDeque::new(),""", """            fed_keys: std::collections::VecDeque::new(),
            sourced_scripts: Vec::new(),
            next_script_id: 0,""", 1)
old = """        let result = rvim_viml::execute_script(source, name, &mut runtime, &mut host);
        let vars = std::mem::take(&mut host.vars);
        drop(host);
        self.viml = runtime;
        self.viml_vars = vars;
        self.sync_message_vvars();
        result.map_err(|error| VimError::plain(error.to_string()))"""
new = """        let result = rvim_viml::execute_script(source, name, &mut runtime, &mut host);
        let vars = std::mem::take(&mut host.vars);
        drop(host);
        self.viml = runtime;
        self.viml_vars = vars;
        self.sync_message_vvars();
        if !name.is_empty() && name != "[command]" {
            self.next_script_id += 1;
            self.sourced_scripts
                .push((self.next_script_id, name.to_string()));
        }
        result.map_err(|error| VimError::plain(error.to_string()))"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
old = """                for id in self.editor.viml.script_ids() {\n                    let name = self.editor.viml.script_name(id).unwrap_or_default();\n                    let mut dict = VimDict::new();"""
new = """                for (sid, name) in &self.editor.sourced_scripts {\n                    let name = name.clone();\n                    let sid = *sid;\n                    let mut dict = VimDict::new();"""
assert old in s
s = s.replace(old, new, 1)
s = s.replace("""                        Value::Number(rvim_types::Id::raw(id) as i64),\n                    );\n                    dict.set(VimString::from(\"name\"), Value::string(name.as_str()));""", """                        Value::Number(sid as i64),\n                    );\n                    dict.set(VimString::from(\"name\"), Value::string(name.as_str()));""", 1)
p.write_text(s)
print("host patched")
