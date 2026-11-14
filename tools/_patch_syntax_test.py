import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
pos = s.rfind('}')
test = '''
    #[test]
    fn matchgroup_and_oneline_regions() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region String start=/\\\"/ end=/\\\"/ matchgroup=Delimiter")
            .unwrap();
        let spans = engine.highlight_line(b"say \\"hi\\"");
        assert!(spans
            .iter()
            .any(|span| span.group == "Delimiter" && span.start == 4 && span.end == 5));
        assert!(spans
            .iter()
            .any(|span| span.group == "String" && span.start == 5 && span.end == 7));

        let mut oneline = SyntaxEngine::new();
        oneline.set_enabled(true);
        oneline
            .define("region Comment start=/<!--/ end=/-->/ oneline")
            .unwrap();
        assert!(!oneline.highlight_line(b"<!-- unfinished").is_empty());
        assert!(oneline.highlight_line(b"next line").is_empty());
    }
'''
s = s[:pos] + test + s[pos:]
p.write_text(s)
print("patched")
