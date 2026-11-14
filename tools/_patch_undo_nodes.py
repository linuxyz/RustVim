import pathlib
p = pathlib.Path("crates/rvim-text/src/lib.rs")
s = p.read_text().replace("pub use undo::{Snapshot, UndoManager};", "pub use undo::{Snapshot, UndoManager, UndoNode};", 1)
p.write_text(s)
p = pathlib.Path("crates/rvim-text/src/undo.rs")
s = p.read_text()
anchor = """    pub fn root_lines(&self) -> Option<Vec<VimString>> {"""
methods = """    pub fn nodes(&self) -> &[UndoNode] {
        &self.nodes
    }

    pub fn current_node(&self) -> usize {
        self.current
    }

    pub fn replace_nodes(&mut self, nodes: Vec<UndoNode>, current: usize) {
        if nodes.is_empty() {
            self.nodes.clear();
            self.current = 0;
            self.pending = None;
            self.seq = 0;
            return;
        }
        self.seq = nodes
            .iter()
            .map(|node| node.snapshot.seq)
            .max()
            .unwrap_or(0)
            + 1;
        self.current = current.min(nodes.len() - 1);
        self.nodes = nodes;
        self.pending = None;
    }

    pub fn root_lines(&self) -> Option<Vec<VimString>> {"""
assert anchor in s
s = s.replace(anchor, methods, 1)
p.write_text(s)
print("undo patched")
