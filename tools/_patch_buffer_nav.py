import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub(crate) fn next_buffer(&mut self, forward: bool) -> Result<(), VimError> {"""
methods = """    pub(crate) fn buffer_first(&mut self) -> Result<(), VimError> {
        let ids = self.listed_buffer_ids();
        if let Some(id) = ids.first().copied() {
            self.switch_to_buffer(id)?;
        }
        Ok(())
    }

    pub(crate) fn buffer_last(&mut self) -> Result<(), VimError> {
        let ids = self.listed_buffer_ids();
        if let Some(id) = ids.last().copied() {
            self.switch_to_buffer(id)?;
        }
        Ok(())
    }

    pub(crate) fn buffer_modified(&mut self) -> Result<(), VimError> {
        let ids = self.listed_buffer_ids();
        if ids.is_empty() {
            return Err(VimError::new(94, "No matching buffer"));
        }
        let start = ids
            .iter()
            .position(|id| *id == self.current_buffer)
            .unwrap_or(0);
        for offset in 1..=ids.len() {
            let id = ids[(start + offset) % ids.len()];
            if self
                .buffers
                .get(id)
                .is_some_and(|buffer| buffer.modified())
            {
                self.switch_to_buffer(id)?;
                return Ok(());
            }
        }
        Err(VimError::new(94, "No modified buffer"))
    }

    fn listed_buffer_ids(&self) -> Vec<BufferId> {
        self.buffer_ids()
            .into_iter()
            .filter(|id| self.buffers.get(*id).is_some_and(|buffer| buffer.listed))
            .collect()
    }

    pub(crate) fn next_buffer(&mut self, forward: bool) -> Result<(), VimError> {"""
assert anchor in s
s = s.replace(anchor, methods, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor2 = """        "bprevious" | "bp" | "bprev" | "bPrevious" => editor.next_buffer(false)?,"""
arms = """        "bprevious" | "bp" | "bprev" | "bPrevious" => editor.next_buffer(false)?,
        "bfirst" | "brewind" | "br" => editor.buffer_first()?,
        "blast" | "bl" => editor.buffer_last()?,
        "bmodified" | "bm" => editor.buffer_modified()?,"""
assert anchor2 in s
s = s.replace(anchor2, arms, 1)
p.write_text(s)
print("ex patched")
# registry flags
p = pathlib.Path("crates/rvim-editor/src/registry_generated.rs")
s = p.read_text()
for name in ['bfirst','blast','bmodified']:
    idx = s.find(f'name: "{name}"')
    end = s.find('implemented: false', idx)
    if idx >= 0 and end >= 0:
        s = s[:end] + 'implemented: true' + s[end+len('implemented: false'):]
p.write_text(s)
print("registry patched")
