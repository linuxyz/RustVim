import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = """        "runtime" | "ru" => {"""
arm = """        "packadd" | "pa" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            editor.packadd(name, !command.bang)?;
        }
        "packloadall" | "packl" => {
            editor.packloadall()?;
        }
        "runtime" | "ru" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
