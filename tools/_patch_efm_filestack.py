import pathlib
p = pathlib.Path("crates/rvim-editor/src/errorformat.rs")
s = p.read_text()
s = s.replace("""    directory: Option<bool>,
    error_kind: Option<char>,
}""", """    directory: Option<bool>,
    file_stack: Option<bool>,
    error_kind: Option<char>,
}""", 1)
s = s.replace("""    let mut directory = None;
    let mut error_kind = None;""", """    let mut directory = None;
    let mut file_stack = None;
    let mut error_kind = None;""", 1)
s = s.replace("""            'D' => directory = Some(true),
            'X' => directory = Some(false),""", """            'D' => directory = Some(true),
            'X' => directory = Some(false),
            'P' => file_stack = Some(true),
            'Q' => file_stack = Some(false),""", 1)
s = s.replace("""        directory,
        error_kind,
    })""", """        directory,
        file_stack,
        error_kind,
    })""", 1)
s = s.replace("""            if let Some(push) = format.directory {
                if push {
                    directories.push(entry.file);
                } else {
                    directories.pop();
                }
                break;
            }""", """            if let Some(push) = format.directory.or(format.file_stack) {
                if push {
                    directories.push(entry.file);
                } else {
                    directories.pop();
                }
                break;
            }""", 1)
s = s.replace("""        if format.directory.is_some() {
            continue;
        }""", """        if format.directory.is_some() || format.file_stack.is_some() {
            continue;
        }""", 1)
s += '''

#[cfg(test)]
mod file_stack_tests {
    use super::*;

    #[test]
    fn file_stack_prefixes_relative_files() {
        let text = "dir/\\nfile.rs:3: boom\\nend\\n";
        let entries = parse(text, "%P%f,%f:%l: %m,%Q%f");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("dir/file.rs"));
    }
}
'''
p.write_text(s)
print("patched")
