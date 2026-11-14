import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
pos = s.rfind('}')
test = '''
    #[test]
    fn containedin_can_enter_named_region() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Comment start=/\\\\/\\\\*/ end=/\\\\*\\\\//")
            .unwrap();
        engine
            .define("keyword Todo TODO containedin=Comment")
            .unwrap();
        assert!(engine.highlight_line(b"/*").iter().any(|span| span.group == "Comment"));
        assert!(engine.highlight_line(b"TODO").iter().any(|span| span.group == "Todo"));

        let mut top = SyntaxEngine::new();
        top.set_enabled(true);
        top.define("keyword Todo TODO containedin=Comment").unwrap();
        assert!(!top.highlight_line(b"TODO").iter().any(|span| span.group == "Todo"));
    }
'''
s = s[:pos] + test + s[pos:]
p.write_text(s)
print("patched")
