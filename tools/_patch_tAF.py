import pathlib
p = pathlib.Path("crates/rvim-platform/src/terminfo.rs")
s = p.read_text()
s = s.replace("""    pub rmcup: String,
    pub el: String,
}""", """    pub rmcup: String,
    pub el: String,
    pub setaf: String,
    pub setab: String,
}""", 1)
s = s.replace("""        rmcup: get(29),
        el: get(3),
    })""", """        rmcup: get(29),
        el: get(3),
        setaf: get(359),
        setab: get(360),
    })""", 1)
p.write_text(s)
print("terminfo patched")
p = pathlib.Path("crates/rvim-editor/src/options.rs")
s = p.read_text()
s = s.replace("""            (\"t_te\", \"\\\\x1b[?1049l\"),\n        ] {""", """            (\"t_te\", \"\\\\x1b[?1049l\"),\n            (\"t_AF\", \"\\\\x1b[3%dm\"),\n            (\"t_AB\", \"\\\\x1b[4%dm\"),\n        ] {""", 1)
s = s.replace("""                (\"t_te\", strings.rmcup),\n            ] {""", """                (\"t_te\", strings.rmcup),\n                (\"t_AF\", strings.setaf),\n                (\"t_AB\", strings.setab),\n            ] {""", 1)
p.write_text(s)
print("options patched")
