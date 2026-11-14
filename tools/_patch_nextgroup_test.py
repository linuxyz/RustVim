import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
pos = s.rfind('}')
test = '''
    #[test]
    fn nextgroup_matches_after_region_end() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Tag start=/</ end=/>/ nextgroup=String")
            .unwrap();
        engine.define("match String /[a-z]\\\\+/").unwrap();
        let spans = engine.highlight_line(b"<x>abc");
        assert!(spans
            .iter()
            .any(|span| span.group == "Tag" && span.start == 0 && span.end == 3));
        assert!(spans
            .iter()
            .any(|span| span.group == "String" && span.start == 3 && span.end == 6));
    }
'''
s = s[:pos] + test + s[pos:]
p.write_text(s)
print("patched")
