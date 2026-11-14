import pathlib
p = pathlib.Path("crates/rvim-viml/src/parser.rs")
s = p.read_text()
s = s.replace("""pub struct LambdaBody {
    pub params: Vec<VimString>,
    pub varargs: bool,
    pub body: Expr,
    pub line: u32,
}""", """pub struct LambdaBody {
    pub params: Vec<VimString>,
    pub varargs: bool,
    pub body: Expr,
    pub line: u32,
    pub source: VimString,
}""", 1)
s = s.replace("""        let start_index = self.index;
        let line = self.current().line;
        self.bump(); // '{'""", """        let start_index = self.index;
        let line = self.current().line;
        let start_pos = self.current().pos;
        self.bump(); // '{'""", 1)
s = s.replace("""            self.expect_punct(Punct::RBrace)?;
            return Ok(Expr::Lambda(Rc::new(LambdaBody {
                params,
                varargs,
                body,
                line,
            })));""", """            self.expect_punct(Punct::RBrace)?;
            let end_pos = self.tokens[self.index.saturating_sub(1)].pos + 1;
            let source = VimString::from(
                self.source[start_pos.min(end_pos)..end_pos.min(self.source.len())].to_vec(),
            );
            return Ok(Expr::Lambda(Rc::new(LambdaBody {
                params,
                varargs,
                body,
                line,
                source,
            })));""", 1)
p.write_text(s)
print("parser patched")
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
s = s.replace("""        rvim_viml::Value::Funcref(function) => {
            let function = function.try_borrow().ok()?;
            if function.name.is_empty() {
                return None;
            }""", """        rvim_viml::Value::Funcref(function) => {
            let function = function.try_borrow().ok()?;
            if let Some(lambda) = &function.lambda {
                return Some(String::from_utf8_lossy(lambda.source.as_bytes()).into_owned());
            }
            if function.name.is_empty() {
                return None;
            }""", 1)
p.write_text(s)
print("viminfo patched")
