import pathlib
p = pathlib.Path("crates/rvim-text/src/undo.rs")
s = p.read_text()
anchor = """    pub fn record(&mut self, buffer: &TextBuffer, cursor: BufPos) {"""
methods = """    /// Seed a session from a persisted pre-edit snapshot.
    pub fn seed(&mut self, previous: Vec<VimString>, current: Vec<VimString>, cursor: BufPos) {
        self.nodes.clear();
        self.pending = None;
        let root = UndoNode {
            id: 0,
            snapshot: Snapshot {
                lines: previous,
                cursor,
                seq: 0,
            },
            parent: None,
            children: vec![1],
            time: now(),
        };
        let child = UndoNode {
            id: 1,
            snapshot: Snapshot {
                lines: current,
                cursor,
                seq: 1,
            },
            parent: Some(0),
            children: Vec::new(),
            time: now(),
        };
        self.nodes.push(root);
        self.nodes.push(child);
        self.current = 1;
        self.seq = 1;
    }

    pub fn root_lines(&self) -> Option<Vec<VimString>> {
        self.nodes.first().map(|node| node.snapshot.lines.clone())
    }

    pub fn record(&mut self, buffer: &TextBuffer, cursor: BufPos) {"""
assert anchor in s
s = s.replace(anchor, methods, 1)
p.write_text(s)
print("undo patched")
