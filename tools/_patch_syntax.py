import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
first = s.index("#[cfg(test)]")
second = s.index("#[cfg(test)]", first + 1)
s = s[:second].rstrip() + "\n"
pos = s.rfind("}")
test = '''
    #[test]
    fn multiline_region_terminates_on_later_line() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region String start=/begin/ end=/end/")
            .unwrap();
        let first = engine.highlight_line(b"begin");
        assert_eq!(first.len(), 1);
        assert_eq!((first[0].start, first[0].end), (0, 5));
        let middle = engine.highlight_line(b"middle");
        assert_eq!(middle.len(), 1);
        assert_eq!((middle[0].start, middle[0].end), (0, 6));
        let last = engine.highlight_line(b"end tail");
        assert_eq!(last.len(), 1);
        assert_eq!((last[0].start, last[0].end), (0, 3));
        let after = engine.highlight_line(b"plain");
        assert!(after.is_empty());
    }
'''
s = s[:pos] + test + "}\n"
p.write_text(s)
print("patched")
