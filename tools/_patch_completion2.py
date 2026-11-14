import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = '''        "compiler" => list_runtime_dir(host, "compiler", pattern),'''
add = '''        "syntax" => host
            .editor
            .highlights
            .iter()
            .map(|(name, _)| name)
            .filter(|name| matches(name))
            .map(str::to_string)
            .collect(),
        "filetype" => list_runtime_dir(host, "syntax", pattern),
        "scriptnames" => host
            .editor
            .viml
            .script_ids()
            .into_iter()
            .filter_map(|id| host.editor.viml.script_name(id))
            .filter(|name| matches(name))
            .collect(),
        "shellcmd" => shell_commands(pattern),
        "runtime" => runtime_files(host, pattern),
        "compiler" => list_runtime_dir(host, "compiler", pattern),'''
assert anchor in s
s = s.replace(anchor, add, 1)
# helpers before completion_candidates
anchor2 = '''fn completion_candidates(host: &mut EditorVimHost<'_>, pattern: &str, kind: &str) -> Vec<String> {'''
helpers = '''fn shell_commands(pattern: &str) -> Vec<String> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for directory in std::env::split_paths(&path) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(pattern) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn runtime_files(host: &EditorVimHost<'_>, pattern: &str) -> Vec<String> {
    let mut names = Vec::new();
    for root in host.editor.runtime.expanded_runtimepath() {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(pattern) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn completion_candidates(host: &mut EditorVimHost<'_>, pattern: &str, kind: &str) -> Vec<String> {'''
assert anchor2 in s
s = s.replace(anchor2, helpers, 1)
p.write_text(s)
print("patched")
