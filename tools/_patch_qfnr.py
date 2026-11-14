import pathlib
p = pathlib.Path("crates/rvim-editor/src/qf.rs")
s = p.read_text()
s = s.replace("""    pub text: String,
    pub kind: Option<char>,
}""", """    pub text: String,
    pub kind: Option<char>,
    pub nr: Option<i64>,
}""", 1)
# add None to constructors in qf.rs
s = s.replace("""        text,
        kind,
    })""", """        text,
        kind,
        nr: None,
    })""", 1)
p.write_text(s)
print("qf patched")
p = pathlib.Path("crates/rvim-editor/src/errorformat.rs")
s = p.read_text()
s = s.replace("""    let mut kind = format.error_kind;""", """    let mut kind = format.error_kind;
    let mut nr = None;""", 1)
s = s.replace("""                if matches!(token, Token::Line) {
                    lnum = value.max(1);
                } else if matches!(token, Token::Column) {
                    col = value.max(1);
                }""", """                if matches!(token, Token::Line) {
                    lnum = value.max(1);
                } else if matches!(token, Token::Column) {
                    col = value.max(1);
                } else {
                    nr = Some(value as i64);
                }""", 1)
s = s.replace("""        text,
        kind,
    })""", """        text,
        kind,
        nr,
    })""", 1)
p.write_text(s)
print("efm patched")
# ex qf entry
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
s = s.replace("""                    kind,
                });""", """                    kind,
                    nr: None,
                });""", 1)
p.write_text(s)
print("ex patched")
# host qf entries
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
s = s.replace("""                    dict.set(VimString::from(\"nr\"), Value::Number(index as i64 + 1));""", """                    dict.set(
                        VimString::from(\"nr\"),
                        Value::Number(entry.nr.unwrap_or(index as i64 + 1)),
                    );""", 1)
s = s.replace("""                    let kind = dict
                        .get_str(\"type\")
                        .and_then(|value| value.echo_string().chars().next());
                    parsed.push(crate::qf::QfEntry {""", """                    let kind = dict
                        .get_str(\"type\")
                        .and_then(|value| value.echo_string().chars().next());
                    let nr = dict
                        .get_str(\"nr\")
                        .and_then(|value| value.echo_string().parse::<i64>().ok());
                    parsed.push(crate::qf::QfEntry {""", 1)
s = s.replace("""                        text,
                        kind,
                    });""", """                        text,
                        kind,
                        nr,
                    });""", 1)
p.write_text(s)
print("host patched")
