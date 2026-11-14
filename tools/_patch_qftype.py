import pathlib
p = pathlib.Path("crates/rvim-editor/src/qf.rs")
s = p.read_text()
s = s.replace("""    pub col: u32,
    pub text: String,
}""", """    pub col: u32,
    pub text: String,
    pub kind: Option<char>,
}""", 1)
s = s.replace("""        lnum: line_number,
        col,
        text,
    })""", """        lnum: line_number,
        col,
        text,
        kind: None,
    })""", 1)
p.write_text(s)
print("qf patched")
p = pathlib.Path("crates/rvim-editor/src/errorformat.rs")
s = p.read_text()
s = s.replace("""    kind: FormatKind,
    directory: Option<bool>,
}""", """    kind: FormatKind,
    directory: Option<bool>,
    error_kind: Option<char>,
}""", 1)
s = s.replace("""    let mut directory = None;
    let mut tokens = Vec::new();""", """    let mut directory = None;
    let mut error_kind = None;
    let mut tokens = Vec::new();""", 1)
s = s.replace("""            'D' => directory = Some(true),
            'X' => directory = Some(false),
            'E' | 'W' | 'I' => {}""", """            'D' => directory = Some(true),
            'X' => directory = Some(false),
            'E' => error_kind = Some('E'),
            'W' => error_kind = Some('W'),
            'I' => error_kind = Some('I'),""", 1)
s = s.replace("""        ignore,
        kind,
        directory,
    })""", """        ignore,
        kind,
        directory,
        error_kind,
    })""", 1)
# match_format local kind and QfEntry field
s = s.replace("""    let mut text = String::new();
    for (index, token) in format.tokens.iter().enumerate() {""", """    let mut text = String::new();
    let mut kind = format.error_kind;
    for (index, token) in format.tokens.iter().enumerate() {""", 1)
s = s.replace("""            Token::Type => {
                if position < bytes.len() {
                    position += 1;
                }
            }""", """            Token::Type => {
                if position < bytes.len() {
                    kind = Some(bytes[position] as char);
                    position += 1;
                }
            }""", 1)
s = s.replace("""        lnum,
        col,
        text,
    })""", """        lnum,
        col,
        text,
        kind,
    })""", 1)
p.write_text(s)
print("errorformat patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
s = s.replace("""                entries.push(crate::qf::QfEntry {
                    file: path.clone(),
                    lnum: 0,
                    col: 0,
                    text,
                });""", """                entries.push(crate::qf::QfEntry {
                    file: path.clone(),
                    lnum: 0,
                    col: 0,
                    text,
                    kind: None,
                });""", 1)
p.write_text(s)
print("ex patched")
