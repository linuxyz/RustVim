import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "tabclose" | "tabc" => editor.tab_close()?,'''
arm = '''        "tabclose" | "tabc" => editor.tab_close()?,
        "tabmove" | "tabm" => {
            let arg = command.args.trim();
            let target = if arg.is_empty() {
                editor.view.tab_count().saturating_sub(1)
            } else if let Some(relative) = arg.strip_prefix('+') {
                let index = editor
                    .view
                    .tab_ids()
                    .iter()
                    .position(|id| *id == editor.view.current_tab)
                    .unwrap_or(0);
                index
                    + relative
                        .trim()
                        .parse::<usize>()
                        .unwrap_or(0)
            } else if let Some(relative) = arg.strip_prefix('-') {
                let index = editor
                    .view
                    .tab_ids()
                    .iter()
                    .position(|id| *id == editor.view.current_tab)
                    .unwrap_or(0);
                index.saturating_sub(relative.trim().parse::<usize>().unwrap_or(0))
            } else {
                arg.parse::<usize>().unwrap_or(0)
            };
            editor.view.move_current_tab(target);
            editor.request_redraw();
        }'''
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("ex patched")
p = pathlib.Path("crates/rvim-editor/src/registry_generated.rs")
s = p.read_text()
idx = s.find('name: "tabmove"')
end = s.find('implemented: false', idx)
assert idx >= 0 and end >= 0
s = s[:end] + 'implemented: true' + s[end+len('implemented: false'):]
p.write_text(s)
print("registry patched")
