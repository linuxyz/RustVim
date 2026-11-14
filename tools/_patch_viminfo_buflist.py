import pathlib
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
s = s.replace("""    globals: Vec<(String, rvim_viml::Value)>,
}""", """    globals: Vec<(String, rvim_viml::Value)>,
    buffers: Vec<String>,
}""", 1)
# parse % before ! branch
old = """        } else if let Some(rest) = line.strip_prefix('!') {"""
new = """        } else if let Some(path) = line.strip_prefix('%') {
            if !path.is_empty() {
                parsed.buffers.push(path.to_string());
            }
        } else if let Some(rest) = line.strip_prefix('!') {"""
assert old in s
s = s.replace(old, new, 1)
# write buffer list before file marks
old2 = """    out.push_str("\\n# File marks:\\n");"""
new2 = """    out.push_str("\\n# Buffer list:\\n");
    for id in editor.buffer_ids() {
        let Some(buffer) = editor.buffers.get(id) else {
            continue;
        };
        if !buffer.listed {
            continue;
        }
        let name = buffer.name_string();
        if !name.is_empty() {
            out.push('%');
            out.push_str(&name);
            out.push('\\n');
        }
    }

    out.push_str("\\n# File marks:\\n");"""
assert old2 in s
s = s.replace(old2, new2, 1)
# apply buffers in read before globals/current_name
old3 = """    for (name, value) in parsed.globals {
        editor.viml.set_global(&name, value);
    }"""
new3 = """    for path in parsed.buffers {
        let already = editor
            .buffer_ids()
            .into_iter()
            .any(|id| editor.buffers.get(id).is_some_and(|buffer| buffer.name_string() == path));
        if !already {
            let _ = editor.add_unlisted_buffer(Some(std::path::Path::new(&path)));
        }
    }
    for (name, value) in parsed.globals {
        editor.viml.set_global(&name, value);
    }"""
assert old3 in s
s = s.replace(old3, new3, 1)
p.write_text(s)
print("patched")
