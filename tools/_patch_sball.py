import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "sp" | "spl" | "spli" | "split" => {'''
arms = '''        "sbnext" | "sbn" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.next_buffer(true)?;
        }
        "sbprevious" | "sbp" | "sbprev" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.next_buffer(false)?;
        }
        "sbrewind" | "sbr" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.buffer_first()?;
        }
        "sblast" | "sbl" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.buffer_last()?;
        }
        "sball" | "sba" => {
            let current = editor.current_buffer;
            let ids = editor.buffer_ids();
            for id in ids {
                if id == current {
                    continue;
                }
                editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
                editor.switch_to_buffer(id)?;
            }
        }
        "sp" | "spl" | "spli" | "split" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
