import pathlib
p = pathlib.Path("crates/rvim-editor/src/errorformat.rs")
s = p.read_text()
s = s.replace("""struct Format {
    tokens: Vec<Token>,
    ignore: bool,
    kind: FormatKind,
}""", """struct Format {
    tokens: Vec<Token>,
    ignore: bool,
    kind: FormatKind,
    directory: Option<bool>,
}""", 1)
# compile init
s = s.replace("""    let mut kind = FormatKind::Normal;
    let mut tokens = Vec::new();""", """    let mut kind = FormatKind::Normal;
    let mut directory = None;
    let mut tokens = Vec::new();""", 1)
s = s.replace("""            'D' | 'X' | 'E' | 'W' | 'I' => {}""", """            'D' => directory = Some(true),
            'X' => directory = Some(false),
            'E' | 'W' | 'I' => {}""", 1)
s = s.replace("""    Some(Format {
        tokens,
        ignore,
        kind,
    })""", """    Some(Format {
        tokens,
        ignore,
        kind,
        directory,
    })""", 1)
# parse loop
s = s.replace("""pub fn parse(text: &str, errorformat: &str) -> Vec<QfEntry> {
    let formats = compile_formats(errorformat);
    let mut entries = Vec::new();
    let mut pending: Option<QfEntry> = None;
    for line in text.lines() {
        let mut matched = false;
        for format in &formats {
            let Some(entry) = match_format(line, format) else {
                continue;
            };
            matched = true;
            if format.ignore {
                break;
            }
            match format.kind {""", """pub fn parse(text: &str, errorformat: &str) -> Vec<QfEntry> {
    let formats = compile_formats(errorformat);
    let mut entries = Vec::new();
    let mut pending: Option<QfEntry> = None;
    let mut directories: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let mut matched = false;
        for format in &formats {
            let Some(mut entry) = match_format(line, format) else {
                continue;
            };
            matched = true;
            if format.ignore {
                break;
            }
            if let Some(push) = format.directory {
                if push {
                    directories.push(entry.file);
                } else {
                    directories.pop();
                }
                break;
            }
            if entry.file.is_relative() {
                if let Some(directory) = directories.last() {
                    entry.file = directory.join(&entry.file);
                }
            }
            match format.kind {""", 1)
# parse_line skip directory formats
s = s.replace("""fn parse_line(line: &str, errorformat: &str) -> Option<QfEntry> {
    parse_with_formats(line, &compile_formats(errorformat))
}""", """fn parse_line(line: &str, errorformat: &str) -> Option<QfEntry> {
    parse_with_formats(line, &compile_formats(errorformat))
}""", 1)
# parse_with_formats skip directory
s = s.replace("""    for format in formats {
        if let Some(entry) = match_format(line, format) {
            if format.ignore {
                return None;
            }
            return Some(entry);
        }
    }
    None""", """    for format in formats {
        if format.directory.is_some() {
            continue;
        }
        if let Some(entry) = match_format(line, format) {
            if format.ignore {
                return None;
            }
            return Some(entry);
        }
    }
    None""", 1)
# test
s += '''

#[cfg(test)]
mod directory_tests {
    use super::*;

    #[test]
    fn directory_formats_adjust_relative_files() {
        let text = "Entering src\\nmain.rs:3:7: boom\\nLeaving src\\n";
        let entries = parse(
            text,
            "%DEntering %f,%f:%l:%c: %m,%XLeaving %f",
        );
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("src/main.rs"));
    }
}
'''
p.write_text(s)
print("patched")
