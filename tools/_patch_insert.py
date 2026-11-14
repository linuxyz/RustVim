import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("pub struct InsertState {\n    pub start: BufPos,\n    pub append: bool,\n    pub undo_started: bool,\n}", "pub struct InsertState {\n    pub start: BufPos,\n    pub append: bool,\n    pub replace: bool,\n    pub undo_started: bool,\n}")
s = s.replace("""        self.mode = Mode::Insert(InsertState {
            start: pos,
            append: true,
            undo_started: true,
        });""", """        self.mode = Mode::Insert(InsertState {
            start: pos,
            append: true,
            replace: false,
            undo_started: true,
        });""")
s = s.replace("""        self.mode = Mode::Insert(InsertState {
            start: target,
            append: !matches!(kind, InsertKind::Before),
            undo_started: true,
        });""", """        self.mode = Mode::Insert(InsertState {
            start: target,
            append: !matches!(kind, InsertKind::Before),
            replace,
            undo_started: true,
        });""")
open(p, "w").write(s)
print("ok")
