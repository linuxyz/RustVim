import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = '''            "histadd" => {'''
add = '''            "wildmenumode" => Ok(Some(Value::Number(i64::from(
                self.editor.cmdline_completion.is_some(),
            )))),
            "histadd" => {'''
assert anchor in s
s = s.replace(anchor, add, 1)
p.write_text(s)
print("host patched")
p = pathlib.Path("crates/rvim-editor/src/render.rs")
s = p.read_text()
old = """                let width = lines
                    .iter()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0) as u16
                    + 2;
                let width = width.min(self.size.cols).max(2);"""
new = """                let width = lines
                    .iter()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0) as u16
                    + 2;
                let configured_width = self.options.get_number("pumwidth").max(0) as u16;
                let width = width.max(configured_width).min(self.size.cols).max(2);"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("render patched")
