import pathlib
p = pathlib.Path("crates/rvim-editor/src/options.rs")
s = p.read_text()
s = s.replace("""pub struct OptionStore {
    global: HashMap<String, OptionValue>,
    buffer: HashMap<(u64, String), OptionValue>,
    window: HashMap<(u64, String), OptionValue>,
}""", """pub struct OptionStore {
    global: HashMap<String, OptionValue>,
    buffer: HashMap<(u64, String), OptionValue>,
    window: HashMap<(u64, String), OptionValue>,
    termcap: HashMap<String, String>,
}""", 1)
s = s.replace("""        Self {
            global,
            buffer: HashMap::new(),
            window: HashMap::new(),
        }""", """        let mut termcap = HashMap::new();
        termcap.insert(
            "t_Co".to_string(),
            rvim_platform::term::capabilities().colors.to_string(),
        );
        Self {
            global,
            buffer: HashMap::new(),
            window: HashMap::new(),
            termcap,
        }""", 1)
# replace t_ skip block
old = """        {
            let bare = token.trim_start_matches('n');
            if bare.starts_with("t_") || token.starts_with("t_") {
                continue;
            }
        }"""
# actual block likely no 'n' heuristic; inspect not here
p.write_text(s)
print("struct patched")
