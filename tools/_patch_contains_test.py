import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
pos = s.rfind('}')
test = '''
    #[test]
    fn contains_allows_nested_groups_inside_region() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Comment start=/\\\\/\\\\*/ end=/\\\\*\\\\// contains=Todo")
            .unwrap();
        engine.define("keyword Todo TODO").unwrap();
        assert!(engine
            .highlight_line(b"/*")
            .iter()
            .any(|span| span.group == "Comment"));
        let nested = engine.highlight_line(b"TODO");
        assert!(nested.iter().any(|span| span.group == "Todo"));

        let mut strict = SyntaxEngine::new();
        strict.set_enabled(true);
        strict
            .define("region Comment start=/\\\\/\\\\*/ end=/\\\\*\\\\//")
            .unwrap();
        strict.define("keyword Todo TODO").unwrap();
        assert!(strict.highlight_line(b"/*").iter().any(|span| span.group == "Comment"));
        let nested = strict.highlight_line(b"TODO");
        assert!(!nested.iter().any(|span| span.group == "Todo"));
    }
'''
s = s[:pos] + test + s[pos:]
p.write_text(s)
print("patched")
