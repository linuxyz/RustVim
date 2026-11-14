import pathlib
p = pathlib.Path("crates/rvim-editor/src/keymap.rs")
s = p.read_text()
s = s.replace("""    timeout: Duration,
}""", """    timeout: Duration,
    timeout_enabled: bool,
}""", 1)
s = s.replace("""        Self {
            timeout: Duration::from_millis(1000),
            ..Self::default()
        }""", """        Self {
            timeout: Duration::from_millis(1000),
            timeout_enabled: true,
            ..Self::default()
        }""", 1)
s = s.replace("""    pub fn set_timeout(&mut self, millis: u64) {
        self.timeout = Duration::from_millis(millis.max(1));
    }""", """    pub fn set_timeout(&mut self, millis: u64) {
        self.timeout = Duration::from_millis(millis.max(1));
    }

    pub fn set_timeout_enabled(&mut self, enabled: bool) {
        self.timeout_enabled = enabled;
    }""", 1)
s = s.replace("""    pub fn flush_timeout(&mut self, mode: MapMode, now: Instant) -> Option<MappingFeed> {
        let since = self.pending_since?;""", """    pub fn flush_timeout(&mut self, mode: MapMode, now: Instant) -> Option<MappingFeed> {
        if !self.timeout_enabled {
            return None;
        }
        let since = self.pending_since?;""", 1)
s = s.replace("""    pub fn deadline(&self) -> Option<Instant> {
        self.pending_since.map(|since| since + self.timeout)
    }""", """    pub fn deadline(&self) -> Option<Instant> {
        if !self.timeout_enabled {
            return None;
        }
        self.pending_since.map(|since| since + self.timeout)
    }""", 1)
p.write_text(s)
print("keymap patched")
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        editor.sync_window_options();\n        editor.update_window_buffer();""", """        editor.mappings.set_timeout(
            editor.options.get_number("timeoutlen").max(1) as u64,
        );\n        editor.mappings.set_timeout_enabled(editor.options.get_bool("timeout"));\n        editor.sync_window_options();\n        editor.update_window_buffer();""", 1)
# after set sync
s = s.replace("""            editor.sync_window_options_public();\n            let result = result.map_err(VimError::plain)?;""", """            editor.sync_window_options_public();\n            editor.mappings.set_timeout(\n                editor.options.get_number("timeoutlen").max(1) as u64,\n            );\n            editor.mappings.set_timeout_enabled(editor.options.get_bool("timeout"));\n            let result = result.map_err(VimError::plain)?;""", 1)
p.write_text(s)
print("editor patched")
