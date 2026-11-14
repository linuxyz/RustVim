import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
old = '''        "version" | "ve" | "ver" => {
            editor.messages.info(format!(
                "RVim {} (Vim {})",
                rvim_types::RVIM_VERSION,
                rvim_types::VIM_VERSION
            ));
        }'''
new = '''        "version" | "ve" | "ver" => {
            let features = [
                "eval",
                "syntax",
                "quickfix",
                "linebreak",
                "statusline_click",
                "compl_func",
                "digraphs",
                "clipboard",
                "mouse",
                "timers",
                "job",
                "pty",
            ]
            .into_iter()
            .filter(|feature| rvim_types::has_feature(feature))
            .collect::<Vec<_>>()
            .join(" ");
            editor.messages.info(format!(
                "RVim {} (Vim {})\\nFeatures: {}",
                rvim_types::RVIM_VERSION,
                rvim_types::VIM_VERSION,
                features
            ));
        }'''
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("ex patched")
# CLI version features
p = pathlib.Path("crates/rvim-cli/src/main.rs")
s = p.read_text()
old2 = '''        println!(
            "RVim {} (Vim {} target)",
            rvim_types::RVIM_VERSION,
            rvim_types::VIM_VERSION
        );'''
new2 = '''        let features = [
            "eval",
            "syntax",
            "quickfix",
            "linebreak",
            "statusline_click",
            "compl_func",
            "digraphs",
            "clipboard",
            "mouse",
            "timers",
            "job",
            "pty",
        ]
        .into_iter()
        .filter(|feature| rvim_types::has_feature(feature))
        .collect::<Vec<_>>()
        .join(" ");
        println!(
            "RVim {} (Vim {} target)\\nFeatures: {}",
            rvim_types::RVIM_VERSION,
            rvim_types::VIM_VERSION,
            features
        );'''
assert old2 in s
s = s.replace(old2, new2, 1)
# help lines
s = s.replace('''  -c COMMAND         execute an Ex command after loading\\n''','''  -c COMMAND         execute an Ex command after loading\\n  -p                 open files in tabs\\n  -o                 open files in horizontal splits\\n  -O                 open files in vertical splits\\n  -u NONE|NORC|DEFAULTS  startup file mode\\n''')
p.write_text(s)
print("cli patched")
