use crate::{LineTree, TextBuffer};
use rvim_types::{BufPos, VimString};
use std::time::{SystemTime, UNIX_EPOCH};

/// A complete buffer snapshot. The first implementation favours correctness;
/// structural sharing can replace the line vectors without changing the API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub lines: Vec<VimString>,
    pub cursor: BufPos,
    pub seq: u64,
}

#[derive(Clone, Debug)]
pub struct UndoNode {
    pub id: usize,
    pub snapshot: Snapshot,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub time: u64,
}

#[derive(Clone, Debug)]
pub struct UndoManager {
    nodes: Vec<UndoNode>,
    current: usize,
    pending: Option<Snapshot>,
    seq: u64,
    limit: usize,
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            current: 0,
            pending: None,
            seq: 0,
            limit: 1000,
        }
    }

    /// Seed a session from a persisted pre-edit snapshot.
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

    pub fn root_lines(&self) -> Option<Vec<VimString>> {
        self.nodes.first().map(|node| node.snapshot.lines.clone())
    }

    pub fn record(&mut self, buffer: &TextBuffer, cursor: BufPos) {
        if self.pending.is_none() {
            self.pending = Some(Snapshot {
                lines: buffer.lines_vec(),
                cursor,
                seq: self.seq,
            });
        }
    }

    /// Finish an editing action. Returns true when the buffer changed.
    pub fn commit(&mut self, buffer: &TextBuffer, cursor: BufPos) -> bool {
        let Some(before) = self.pending.take() else {
            return false;
        };
        let after = buffer.lines_vec();
        if before.lines == after {
            return false;
        }
        if self.nodes.is_empty() {
            self.nodes.push(UndoNode {
                id: 0,
                snapshot: before,
                parent: None,
                children: Vec::new(),
                time: now(),
            });
            self.current = 0;
        }
        self.seq += 1;
        let id = self.nodes.len();
        let parent = self.current;
        self.nodes.push(UndoNode {
            id,
            snapshot: Snapshot {
                lines: after,
                cursor,
                seq: self.seq,
            },
            parent: Some(parent),
            children: Vec::new(),
            time: now(),
        });
        if self.nodes.get(parent).is_some() {
            self.nodes[parent].children.push(id);
        }
        self.current = id;
        while self.nodes.len() > self.limit.max(1) {
            self.prune_root();
        }
        true
    }

    fn prune_root(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        self.nodes.remove(0);
        for node in &mut self.nodes {
            node.id = node.id.saturating_sub(1);
            node.parent = node.parent.and_then(|parent| parent.checked_sub(1));
            node.children = node
                .children
                .iter()
                .filter_map(|child| child.checked_sub(1))
                .collect();
        }
        self.current = self.current.saturating_sub(1);
    }

    pub fn undo(&mut self, buffer: &mut TextBuffer, _cursor: BufPos) -> Option<BufPos> {
        let node = self.nodes.get(self.current)?;
        let parent = node.parent?;
        let snapshot = self.nodes.get(parent)?.snapshot.clone();
        self.current = parent;
        buffer.restore_lines(snapshot.lines.clone());
        buffer.set_modified(true);
        Some(snapshot.cursor)
    }

    pub fn redo(&mut self, buffer: &mut TextBuffer, _cursor: BufPos) -> Option<BufPos> {
        let node = self.nodes.get(self.current)?;
        let child = *node.children.last()?;
        let snapshot = self.nodes.get(child)?.snapshot.clone();
        self.current = child;
        buffer.restore_lines(snapshot.lines.clone());
        buffer.set_modified(true);
        Some(snapshot.cursor)
    }

    pub fn undo_count(&self) -> usize {
        if self.nodes.is_empty() {
            0
        } else {
            self.current
        }
    }

    pub fn redo_count(&self) -> usize {
        self.nodes
            .get(self.current)
            .map_or(0, |node| node.children.len())
    }

    pub fn nodes(&self) -> &[UndoNode] {
        &self.nodes
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.current = 0;
        self.pending = None;
        self.seq = 0;
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        while self.nodes.len() > self.limit {
            self.prune_root();
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

impl TextBuffer {
    pub fn restore_tree(&mut self, tree: LineTree) {
        self.lines = tree;
        self.meta.changedtick = self.meta.changedtick.wrapping_add(1);
        self.meta.modified = true;
    }
}
