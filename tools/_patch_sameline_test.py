import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
pos = s.rfind('}')
test = '''
    #[test]
    fn same_line_region_containers_filter_nested_groups() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Comment start=/\\\\/\\\\*/ end=/\\\\*\\\\// contains=Todo")
            .unwrap();
        engine.define("keyword Todo TODO").unwrap();
        let spans = engine.highlight_line(b"/* TODO */");
        assert!(spans.iter().any(|span| span.group == "Comment"));
        assert!(spans.iter().any(|span| span.group == "Todo"));
        assert!(spans
            .iter()
            .any(|span| span.group == "Todo" && span.start == 3));

        let mut strict = SyntaxEngine::new();
        strict.set_enabled(true);
        strict
            .define("region Comment start=/\\\\/\\\\*/ end=/\\\\*\\\\//")
            .unwrap();
        strict.define("keyword Todo TODO").unwrap();
        let spans = strict.highlight_line(b"/* TODO */");
        assert!(spans.iter().any(|span| span.group == "Comment"));
        assert!(!spans.iter().any(|span| span.group == "Todo"));
    }
'''
s = s[:pos] + test + s[pos:]
p.write_text(s)
print("patched")
