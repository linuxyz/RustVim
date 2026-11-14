import pathlib
p = pathlib.Path("crates/rvim-editor/src/keymap.rs")
s = p.read_text()
# imports
s = s.replace("use rvim_ui_api::{Key, KeyEvent, KeyKind, Mods};", "use rvim_types::BufferId;\nuse rvim_ui_api::{Key, KeyEvent, KeyKind, Mods};", 1)
# struct mapping and options/feed
s = s.replace("""#[derive(Clone, Debug)]
pub struct Mapping {
    pub lhs: Vec<KeyEvent>,
    pub rhs: Vec<KeyEvent>,
    pub noremap: bool,
}""", """#[derive(Clone, Debug)]
pub struct Mapping {
    pub lhs: Vec<KeyEvent>,
    pub rhs: Vec<KeyEvent>,
    pub noremap: bool,
    pub nowait: bool,
    pub silent: bool,
    pub buffer: Option<BufferId>,
}

#[derive(Clone, Debug, Default)]
pub struct MappingOptions {
    pub nowait: bool,
    pub silent: bool,
    pub buffer: Option<BufferId>,
    pub unique: bool,
}""", 1)
s = s.replace("""#[derive(Clone, Debug, Default)]
pub struct MappingFeed {
    pub keys: Vec<KeyEvent>,
    pub pending: bool,
}""", """#[derive(Clone, Debug, Default)]
pub struct MappingFeed {
    pub keys: Vec<KeyEvent>,
    pub pending: bool,
    pub silent: bool,
}""", 1)
# state active_buffer
s = s.replace("""    timeout: Duration,
    timeout_enabled: bool,
}""", """    timeout: Duration,
    timeout_enabled: bool,
    active_buffer: Option<BufferId>,
}""", 1)
# add method
s = s.replace("""    pub fn add(&mut self, mode: MapMode, lhs: Vec<KeyEvent>, rhs: Vec<KeyEvent>, noremap: bool) {
        if lhs.is_empty() {
            return;
        }
        let map = Mapping { lhs, rhs, noremap };
        let list = self.list_mut(mode);
        if let Some(existing) = list
            .iter_mut()
            .find(|existing| same_keys(&existing.lhs, &map.lhs))
        {
            *existing = map;
        } else {
            list.push(map);
        }
    }""", """    pub fn add(
        &mut self,
        mode: MapMode,
        lhs: Vec<KeyEvent>,
        rhs: Vec<KeyEvent>,
        noremap: bool,
        options: MappingOptions,
    ) -> bool {
        if lhs.is_empty() {
            return false;
        }
        let map = Mapping {
            lhs,
            rhs,
            noremap,
            nowait: options.nowait,
            silent: options.silent,
            buffer: options.buffer,
        };
        let list = self.list_mut(mode);
        if let Some(existing) = list.iter_mut().find(|existing| {
            same_keys(&existing.lhs, &map.lhs) && existing.buffer == map.buffer
        }) {
            if options.unique {
                return false;
            }
            *existing = map;
        } else {
            list.push(map);
        }
        true
    }""", 1)
# feed signature and active buffer
s = s.replace("""    pub fn feed(&mut self, mode: MapMode, key: KeyEvent) -> MappingFeed {
        self.pending.push(key);
        self.pending_since = Some(Instant::now());
        self.resolve(mode, 10)
    }""", """    pub fn feed(&mut self, mode: MapMode, key: KeyEvent, buffer: BufferId) -> MappingFeed {
        self.active_buffer = Some(buffer);
        self.pending.push(key);
        self.pending_since = Some(Instant::now());
        self.resolve(mode, 10)
    }""", 1)
# flush_timeout feed silent field
s = s.replace("""        Some(MappingFeed {
            keys,
            pending: false,
        })""", """        Some(MappingFeed {
            keys,
            pending: false,
            silent: false,
        })""", 1)
# resolve rewrite
start = s.index("    fn resolve(&mut self, mode: MapMode, depth: usize) -> MappingFeed {")
end = s.index("    fn list_mut(&mut self, mode: MapMode) -> &mut Vec<Mapping> {", start)
new = """    fn visible(&self, mode: MapMode) -> Vec<Mapping> {
        self.list(mode)
            .iter()
            .filter(|mapping| {
                mapping.buffer.is_none() || mapping.buffer == self.active_buffer
            })
            .cloned()
            .collect()
    }

    fn resolve(&mut self, mode: MapMode, depth: usize) -> MappingFeed {
        if depth == 0 {
            let keys = std::mem::take(&mut self.pending);
            return MappingFeed {
                keys,
                pending: false,
                silent: false,
            };
        }
        let mappings = self.visible(mode);
        let exact = mappings
            .iter()
            .find(|mapping| same_keys(&mapping.lhs, &self.pending))
            .cloned();
        let has_longer = mappings.iter().any(|mapping| {
            mapping.lhs.len() > self.pending.len() && prefix(&mapping.lhs, &self.pending)
        });
        if let Some(mapping) = exact {
            if mapping.nowait || !has_longer {
                self.pending.clear();
                self.pending_since = None;
                if mapping.noremap {
                    return MappingFeed {
                        keys: mapping.rhs,
                        pending: false,
                        silent: mapping.silent,
                    };
                }
                let mut out = Vec::new();
                let mut state = MappingState {
                    timeout: self.timeout,
                    timeout_enabled: self.timeout_enabled,
                    ..MappingState::default()
                };
                state.active_buffer = self.active_buffer;
                let mut silent = mapping.silent;
                for key in mapping.rhs {
                    let feed = state.feed(
                        mode,
                        key,
                        self.active_buffer.unwrap_or_default(),
                    );
                    silent |= feed.silent;
                    out.extend(feed.keys);
                }
                out.extend(state.flush());
                return MappingFeed {
                    keys: out,
                    pending: false,
                    silent,
                };
            }
            return MappingFeed {
                keys: Vec::new(),
                pending: true,
                silent: false,
            };
        }
        if has_longer {
            return MappingFeed {
                keys: Vec::new(),
                pending: true,
                silent: false,
            };
        }
        let feed = MappingFeed {
            keys: std::mem::take(&mut self.pending),
            pending: false,
            silent: false,
        };
        self.pending_since = None;
        feed
    }

"""
s = s[:start] + new + s[end:]
# remove old has_exact code? replaced by start-end; ensure no duplicate resolve
p.write_text(s)
print("keymap patched")
