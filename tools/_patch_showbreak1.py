import pathlib
p = pathlib.Path("crates/rvim-view/src/window.rs")
s = p.read_text()
s = s.replace("""    pub linebreak: bool,
    pub breakat: String,""", """    pub linebreak: bool,
    pub breakat: String,
    pub showbreak: String,
    pub breakindent: bool,""", 1)
s = s.replace("""            linebreak: false,
            breakat: String::new(),""", """            linebreak: false,
            breakat: String::new(),
            showbreak: String::new(),
            breakindent: false,""", 1)
p.write_text(s)
print("window patched")
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""            let breakat = self
                .options
                .window_string(rvim_types::Id::raw(id), "breakat");""", """            let breakat = self
                .options
                .window_string(rvim_types::Id::raw(id), "breakat");
            let showbreak = self
                .options
                .window_string(rvim_types::Id::raw(id), "showbreak");
            let breakindent = self
                .options
                .window_bool(rvim_types::Id::raw(id), "breakindent");""", 1)
s = s.replace("""                window.options.breakat = breakat;""", """                window.options.breakat = breakat;
                window.options.showbreak = showbreak;
                window.options.breakindent = breakindent;""", 1)
p.write_text(s)
print("editor patched")
