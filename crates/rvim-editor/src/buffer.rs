use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rvim_text::{TextBuffer, UndoManager};
use rvim_types::{BufPos, BufferId};

#[derive(Clone, Debug)]
pub struct BufferEntry {
    pub id: BufferId,
    pub text: TextBuffer,
    pub listed: bool,
    pub loaded: bool,
    pub filetype: String,
    pub vars: HashMap<String, String>,
    pub marks: BTreeMap<char, BufPos>,
    pub undo: UndoManager,
    pub readonly: bool,
    pub buftype: String,
}

impl BufferEntry {
    pub fn new(id: BufferId) -> Self {
        Self {
            id,
            text: TextBuffer::new(),
            listed: true,
            loaded: true,
            filetype: String::new(),
            vars: HashMap::new(),
            marks: BTreeMap::new(),
            undo: UndoManager::new(),
            readonly: false,
            buftype: String::new(),
        }
    }

    pub fn name(&self) -> Option<&Path> {
        self.text.meta.name.as_deref()
    }

    pub fn name_string(&self) -> String {
        self.text
            .meta
            .name
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    }

    pub fn short_name(&self) -> String {
        let Some(path) = self.name() else {
            return String::new();
        };
        path.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
    }

    pub fn set_name(&mut self, path: PathBuf) {
        self.text.meta.name = Some(path);
    }

    pub fn modified(&self) -> bool {
        self.text.meta.modified
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.text.meta.modified = modified;
    }

    pub fn line_count(&self) -> u32 {
        self.text.line_count()
    }

    pub fn load_bytes(&mut self, bytes: &[u8]) {
        self.text.load_from_bytes(bytes);
        self.undo.clear();
    }

    pub fn cursor_after_load(&self) -> BufPos {
        BufPos::new(1, 0)
    }
}
