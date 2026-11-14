import pathlib
p = pathlib.Path("crates/rvim-editor/src/keymap.rs")
s = p.read_text()
s = s.replace("""    pub buffer: Option<BufferId>,
}""", """    pub buffer: Option<BufferId>,
    pub expr: Option<String>,
}""", 1)
s = s.replace("""    pub buffer: Option<BufferId>,
    pub unique: bool,
}""", """    pub buffer: Option<BufferId>,
    pub unique: bool,
    pub expr: Option<String>,
}""", 1)
s = s.replace("""pub struct MappingFeed {
    pub keys: Vec<KeyEvent>,
    pub pending: bool,
    pub silent: bool,
}""", """pub struct MappingFeed {
    pub keys: Vec<KeyEvent>,
    pub pending: bool,
    pub silent: bool,
    pub expr: Option<String>,
}""", 1)
s = s.replace("""            buffer: options.buffer,
        };""", """            buffer: options.buffer,
            expr: options.expr,
        };""", 1)
# all MappingFeed literals add expr None
s = s.replace("""            return MappingFeed {
                keys,
                pending: false,
                silent: false,
            };""", """            return MappingFeed {
                keys,
                pending: false,
                silent: false,
                expr: None,
            };""", 1)
# resolve exact mapping expr branch
old = """            if mapping.nowait || !has_longer {
                self.pending.clear();
                self.pending_since = None;
                if mapping.noremap {"""
new = """            if mapping.nowait || !has_longer {
                self.pending.clear();
                self.pending_since = None;
                if let Some(expression) = mapping.expr {
                    return MappingFeed {
                        keys: Vec::new(),
                        pending: false,
                        silent: mapping.silent,
                        expr: Some(expression),
                    };
                }
                if mapping.noremap {"""
assert old in s
s = s.replace(old, new, 1)
# noremap feed literal
s = s.replace("""                    return MappingFeed {
                        keys: mapping.rhs,
                        pending: false,
                        silent: mapping.silent,
                    };""", """                    return MappingFeed {
                        keys: mapping.rhs,
                        pending: false,
                        silent: mapping.silent,
                        expr: None,
                    };""", 1)
# silent computation feed from state
s = s.replace("""                    silent |= feed.silent;
                    out.extend(feed.keys);""", """                    silent |= feed.silent;
                    out.extend(feed.keys);
                    if let Some(expression) = feed.expr {
                        return MappingFeed {
                            keys: Vec::new(),
                            pending: false,
                            silent,
                            expr: Some(expression),
                        };
                    }""", 1)
# recursive return literal
s = s.replace("""                return MappingFeed {
                    keys: out,
                    pending: false,
                    silent,
                };""", """                return MappingFeed {
                    keys: out,
                    pending: false,
                    silent,
                    expr: None,
                };""", 1)
# waiting literals
s = s.replace("""            return MappingFeed {
                keys: Vec::new(),
                pending: true,
                silent: false,
            };""", """            return MappingFeed {
                keys: Vec::new(),
                pending: true,
                silent: false,
                expr: None,
            };""", 2)
# final feed literal
s = s.replace("""        let feed = MappingFeed {
            keys: std::mem::take(&mut self.pending),
            pending: false,
            silent: false,
        };""", """        let feed = MappingFeed {
            keys: std::mem::take(&mut self.pending),
            pending: false,
            silent: false,
            expr: None,
        };""", 1)
p.write_text(s)
print("keymap patched")
