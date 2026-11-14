//! Recursive-descent parser for Vim9 script.
//!
//! Vim9 is line oriented.  A statement ends at a newline, but the parser
//! accepts the documented continuations: inside (), \[\], {} a newline is
//! ignored, and a newline is allowed before or after a binary operator, before
//! "?" and ":" of a ternary, and before "." and "-&gt;".
//!
//! Ex command lines ("set", "execute", "echomsg", ...) are not tokenized:
//! when a statement does not start like a Vim9 construct the raw line is
//! captured as Stmt::Command, so command-specific syntax never confuses the
//! language parser.  Lines starting with backslash or "|" are continuations
//! of a previous Ex command and are appended to it.

use std::collections::VecDeque;

use rvim_types::VimString;

use crate::error::{Result, VimError};
use crate::lexer::OptionScope;

use super::ast::*;
use super::error::{codes, error};
use super::lexer::{line_bounds, Lexer};
use super::token::{is_statement_keyword, InterpPart, Punct, Token, TokenKind};

/// Parse a complete Vim9 script.
///
/// Files may start with a legacy Vim script preamble before "vim9script"
/// (the |vim9-mix| style); the preamble is skipped here and is the script
/// loader's responsibility in later phases.
pub fn parse_script(source: &[u8]) -> Result<Script> {
    let (offset, line) = find_vim9_header(source);
    if offset == 0 {
        Parser::new(source)?.parse_script()
    } else {
        Parser::new_from(source, offset, line)?.parse_script()
    }
}

/// Locate the first "vim9script" command line, returning its byte offset and
/// 1-based line number.  Returns (0, 1) when no header is found.
///
/// Only the vim9-mix preambles found in practice are accepted: blank lines,
/// legacy quote comments, and an "if ... has('vim9script') ... finish ...
/// endif" guard.  A file whose first real command is legacy code is not a
/// Vim9 script even if the word vim9script appears later (for example inside
/// a heredoc).
fn find_vim9_header(source: &[u8]) -> (usize, u32) {
    let mut line = 1u32;
    let mut start = 0usize;
    let mut guard_lines = 0usize;
    while start <= source.len() {
        let mut end = start;
        while end < source.len() && source[end] != b'\n' {
            end += 1;
        }
        let mut begin = start;
        while begin < end && matches!(source[begin], b' ' | b'\t') {
            begin += 1;
        }
        let mut finish = end;
        while finish > begin && matches!(source[finish - 1], b'\r' | b' ' | b'\t') {
            finish -= 1;
        }
        let text = &source[begin..finish];
        if text.is_empty() || text.starts_with(b"\"") {
            // Blank line or legacy comment in a vim9-mix preamble.
            if end >= source.len() {
                break;
            }
            start = end + 1;
            line += 1;
            continue;
        }
        if text.starts_with(b"vim9script") {
            let after = b"vim9script".len();
            let boundary = text.len() == after || matches!(text[after], b' ' | b'\t' | b'#');
            if boundary {
                return (start, line);
            }
        }
        if guard_lines == 0
            && text.starts_with(b"if ")
            && (contains_bytes(text, b"has('vim9script')")
                || contains_bytes(text, b"has(\"vim9script\")")
                || contains_bytes(text, b"v:version"))
        {
            guard_lines = 20;
        } else if guard_lines == 0 {
            return (0, 1);
        } else {
            guard_lines -= 1;
        }
        if end >= source.len() {
            break;
        }
        start = end + 1;
        line += 1;
    }
    (0, 1)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Parse a single expression.  Used by string interpolation and tests.
pub fn parse_expression_source(source: &[u8], line: u32) -> Result<Expr> {
    let mut parser = Parser::new_at(source, line)?;
    let expr = parser.parse_expression()?;
    parser.expect_line_end()?;
    Ok(expr)
}

/// A block terminator keyword together with its line.
struct Terminator {
    word: String,
    line: u32,
}

struct Parser<'a> {
    source: &'a [u8],
    lexer: Lexer<'a>,
    tok: Token,
    pending: VecDeque<Token>,
    /// Inside a single-line compound statement: "if cond | stmt | endif".
    bar_mode: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a [u8]) -> Result<Self> {
        Self::new_from(source, 0, 1)
    }

    fn new_at(source: &'a [u8], line: u32) -> Result<Self> {
        Self::new_from(source, 0, line)
    }

    fn new_from(source: &'a [u8], pos: usize, line: u32) -> Result<Self> {
        let mut lexer = Lexer::new(source);
        if pos != 0 || line != 1 {
            lexer.seek(pos, line);
        }
        let tok = lexer.next_token()?;
        Ok(Parser {
            source,
            lexer,
            tok,
            pending: VecDeque::new(),
            bar_mode: false,
        })
    }

    // ---------------------------------------------------------------
    // Token cursor helpers
    // ---------------------------------------------------------------

    fn fill(&mut self, count: usize) -> Result<()> {
        while self.pending.len() < count {
            let token = self.lexer.next_token()?;
            self.pending.push_back(token);
        }
        Ok(())
    }

    /// The token "n" positions after the current one (1 = next token).
    fn peek_nth(&mut self, n: usize) -> Result<Token> {
        self.fill(n)?;
        Ok(self.pending[n - 1].clone())
    }

    fn bump(&mut self) -> Result<()> {
        self.tok = match self.pending.pop_front() {
            Some(token) => token,
            None => self.lexer.next_token()?,
        };
        Ok(())
    }

    fn eat_punct(&mut self, punct: Punct) -> Result<bool> {
        if self.tok.kind.is_punct(punct) {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect_punct(&mut self, punct: Punct, code: u32, message: &str) -> Result<()> {
        if self.eat_punct(punct)? {
            Ok(())
        } else {
            Err(self.err(code, message))
        }
    }

    fn eat_ident(&mut self, word: &str) -> Result<bool> {
        if self.tok.kind.is_ident(word) {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<VimString> {
        let name = match &self.tok.kind {
            TokenKind::Ident(name) => name.clone(),
            _ => return Err(self.err(codes::E1004, format!("Expected name: {}", what))),
        };
        self.bump()?;
        Ok(name)
    }

    fn err(&self, code: u32, message: impl Into<String>) -> VimError {
        error(code, message, self.tok.line)
    }

    fn skip_newlines(&mut self) -> Result<()> {
        while matches!(self.tok.kind, TokenKind::Newline) {
            self.bump()?;
        }
        Ok(())
    }

    fn expect_line_end(&mut self) -> Result<()> {
        if self.bar_mode && self.tok.kind.is_punct(Punct::Bar) {
            self.bump()?;
            return Ok(());
        }
        match &self.tok.kind {
            TokenKind::Newline => {
                self.bump()?;
                Ok(())
            }
            TokenKind::Eof => Ok(()),
            _ => Err(self.err(
                codes::E488,
                format!("Trailing characters: {}", self.line_snippet()),
            )),
        }
    }

    fn line_snippet(&self) -> String {
        let (start, end, _) = line_bounds(self.source, self.tok.start);
        String::from_utf8_lossy(&self.source[start..end])
            .trim()
            .to_string()
    }

    fn seek_to_line(&mut self, pos: usize, line: u32) -> Result<()> {
        self.pending.clear();
        self.lexer.seek(pos, line);
        self.tok = self.lexer.next_token()?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Script and blocks
    // ---------------------------------------------------------------

    fn parse_script(mut self) -> Result<Script> {
        self.skip_newlines()?;
        let mut vim9_header = false;
        if self.tok.kind.is_ident("vim9script") {
            vim9_header = true;
            self.bump()?;
            // ":vim9script noclear" is accepted.
            let _ = self.eat_ident("noclear")?;
            if !matches!(self.tok.kind, TokenKind::Newline | TokenKind::Eof) {
                return Err(self.err(
                    codes::E488,
                    format!("Trailing characters: {}", self.line_snippet()),
                ));
            }
            self.expect_line_end()?;
        }
        let (items, terminator) = self.parse_block(&[])?;
        if let Some(terminator) = terminator {
            return Err(unexpected_terminator(&terminator.word, terminator.line));
        }
        if !matches!(self.tok.kind, TokenKind::Eof) {
            return Err(self.err(
                codes::E488,
                format!("Trailing characters: {}", self.line_snippet()),
            ));
        }
        Ok(Script { vim9_header, items })
    }

    fn parse_block(&mut self, terminators: &[&str]) -> Result<(Vec<Stmt>, Option<Terminator>)> {
        let mut statements = Vec::new();
        loop {
            self.skip_newlines()?;
            if matches!(self.tok.kind, TokenKind::Eof) {
                return Ok((statements, None));
            }
            if let TokenKind::Ident(word) = &self.tok.kind {
                if terminators
                    .iter()
                    .any(|terminator| word.as_bytes() == terminator.as_bytes())
                {
                    let word = word.to_string_lossy().into_owned();
                    let line = self.tok.line;
                    self.bump()?;
                    return Ok((statements, Some(Terminator { word, line })));
                }
            }
            if !self.starts_language_statement()? {
                if self.bar_mode {
                    statements.push(self.capture_bar_command()?);
                    continue;
                }
                let command = self.capture_command()?;
                let continuation = match &command {
                    Stmt::Command { text, .. } => {
                        let trimmed = trim_start_bytes(text.as_bytes());
                        is_command_continuation(trimmed)
                    }
                    _ => false,
                };
                if continuation {
                    if let Some(Stmt::Command { text, .. }) = statements.last_mut() {
                        text.push(b'\n');
                        if let Stmt::Command { text: line, .. } = &command {
                            text.extend_from_slice(line.as_bytes());
                        }
                        continue;
                    }
                }
                statements.push(command);
                continue;
            }
            statements.push(self.parse_statement()?);
        }
    }

    /// True when the current token starts a Vim9 language statement rather
    /// than an Ex command line.  The decision for names is made from the raw
    /// bytes after the name so command lines are not tokenized further.
    fn starts_language_statement(&mut self) -> Result<bool> {
        if self.is_command_continuation_line() {
            return Ok(false);
        }
        match &self.tok.kind {
            TokenKind::Ident(word) => {
                let bytes = word.as_bytes();
                if is_statement_keyword(bytes) {
                    if bytes == b"abstract" {
                        // "abstract class" only; anything else is not a
                        // statement we know about.
                        let next = self.peek_nth(1)?;
                        return Ok(
                            matches!(&next.kind, TokenKind::Ident(w) if w.as_bytes() == b"class"),
                        );
                    }
                    return Ok(true);
                }
                Ok(self.name_starts_expression())
            }
            TokenKind::ScopedVar(..)
            | TokenKind::Option(_)
            | TokenKind::Env(_)
            | TokenKind::Register(_) => Ok(self.name_starts_expression()),
            TokenKind::Punct(Punct::LParen) | TokenKind::Punct(Punct::LBracket) => Ok(true),
            TokenKind::String(_)
            | TokenKind::Number(_)
            | TokenKind::Float(_)
            | TokenKind::Blob(_)
            | TokenKind::InterpolatedString(_) => {
                let rest = &self.source[self.tok.end..];
                let rest = &rest[..rest
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .unwrap_or(rest.len())];
                let rest = trim_start_bytes(rest);
                Ok(rest.starts_with(b"->") || rest.starts_with(b"."))
            }
            _ => Ok(false),
        }
    }

    /// A physical line starting with white space and a backslash continues
    /// the previous Ex command.  The lexer skipped the backslash, so inspect
    /// the raw bytes before the current token.
    fn is_command_continuation_line(&self) -> bool {
        let (line_start, _, _) = line_bounds(self.source, self.tok.start);
        let prefix = &self.source[line_start..self.tok.start];
        let mut saw_backslash = false;
        for byte in prefix {
            match byte {
                b' ' | b'\t' => {}
                b'\\' => saw_backslash = true,
                _ => return false,
            }
        }
        saw_backslash
    }

    /// Whether the raw text after the current name token looks like an
    /// assignment/call chain.  Vim9 requires white space around operators, so
    /// a following "=" or "(" (possibly after blanks) is unambiguous.
    fn name_starts_expression(&self) -> bool {
        let rest = &self.source[self.tok.end..];
        let end = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .unwrap_or(rest.len());
        let rest = trim_start_bytes(&rest[..end]);
        match rest.first() {
            Some(b'=') => !rest.starts_with(b"==") && !rest.starts_with(b"=~"),
            Some(b'(') | Some(b'[') | Some(b'.') => true,
            Some(b'+') => rest.starts_with(b"+="),
            Some(b'*') => rest.starts_with(b"*="),
            Some(b'/') => rest.starts_with(b"/="),
            Some(b'%') => rest.starts_with(b"%="),
            Some(b'-') => rest.starts_with(b"->") || rest.starts_with(b"-="),
            None => self.next_line_starts_member_chain(),
            _ => false,
        }
    }

    /// True when the text after the current token ends the line and the next
    /// non-blank, non-comment line continues with "-&gt;" or ".".  This covers a
    /// statement such as "tags" followed by a "-&gt;filter(...)" chain on the
    /// next lines.
    fn next_line_starts_member_chain(&self) -> bool {
        let mut index = self.tok.end;
        while index < self.source.len() && self.source[index] != b'\n' {
            if !matches!(self.source[index], b' ' | b'\t' | b'\r') {
                return false;
            }
            index += 1;
        }
        let mut lines = 0usize;
        while index < self.source.len() && lines < 200 {
            if self.source[index] == b'\n' {
                index += 1;
                lines += 1;
                continue;
            }
            let line_start = index;
            let mut line_end = index;
            while line_end < self.source.len() && self.source[line_end] != b'\n' {
                line_end += 1;
            }
            let mut begin = line_start;
            while begin < line_end && matches!(self.source[begin], b' ' | b'\t') {
                begin += 1;
            }
            if begin >= line_end {
                index = line_end;
                continue;
            }
            if self.source[begin] == b'#' {
                index = line_end;
                continue;
            }
            let tail = &self.source[begin..line_end];
            return tail.starts_with(b"->") || tail.starts_with(b".");
        }
        false
    }

    fn capture_command(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        let (start, end, next) = line_bounds(self.source, self.tok.start);
        let mut begin = start;
        while begin < end && matches!(self.source[begin], b' ' | b'\t') {
            begin += 1;
        }
        let text = VimString::from_bytes(self.source[begin..end].to_vec());
        self.seek_to_line(next, line + 1)?;
        Ok(Stmt::Command { text, line })
    }

    /// Capture one Ex command inside a bar separated inline body, stopping at
    /// the next "|" or at the physical end of line.
    fn capture_bar_command(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        let start = self.tok.start;
        let mut index = 0usize;
        loop {
            let token = self.peek_nth(index + 1)?;
            match token.kind {
                TokenKind::Punct(Punct::Bar) => {
                    let text = trim_bytes(&self.source[start..token.start]);
                    let after = token.end;
                    self.seek_to_line(after, line)?;
                    return Ok(Stmt::Command { text, line });
                }
                TokenKind::Newline | TokenKind::Eof => {
                    let (_, end, _) = line_bounds(self.source, start);
                    let text = trim_bytes(&self.source[start..end]);
                    self.seek_to_line(end, line)?;
                    return Ok(Stmt::Command { text, line });
                }
                _ => index += 1,
            }
        }
    }

    /// Consume the "|" that starts an inline (single line) compound body.
    fn begin_inline_body(&mut self) -> Result<bool> {
        if self.tok.kind.is_punct(Punct::Bar) {
            self.bump()?;
            self.bar_mode = true;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Start the body of a compound statement: either inline after "|" or on
    /// the following line.  "saved_bar" is the bar mode of the outer block.
    fn begin_block_body(&mut self, saved_bar: bool) -> Result<()> {
        if !self.begin_inline_body()? {
            self.bar_mode = saved_bar;
            self.expect_line_end()?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Statements
    // ---------------------------------------------------------------

    fn parse_statement(&mut self) -> Result<Stmt> {
        let word = match &self.tok.kind {
            TokenKind::Ident(word) => word.clone(),
            _ => return self.parse_expression_statement(),
        };
        match word.as_bytes() {
            b"var" => self.parse_var_decl(VarKind::Var),
            b"final" => self.parse_var_decl(VarKind::Final),
            b"const" => self.parse_var_decl(VarKind::Const),
            b"def" => {
                let def = self.parse_def(false, false)?;
                Ok(Stmt::Def(def))
            }
            b"if" => self.parse_if(),
            b"while" => self.parse_while(),
            b"for" => self.parse_for(),
            b"try" => self.parse_try(),
            b"return" => {
                let line = self.tok.line;
                self.bump()?;
                let value = if matches!(self.tok.kind, TokenKind::Newline | TokenKind::Eof)
                    || (self.bar_mode && self.tok.kind.is_punct(Punct::Bar))
                {
                    None
                } else {
                    Some(self.parse_expression()?)
                };
                self.expect_line_end()?;
                Ok(Stmt::Return { value, line })
            }
            b"throw" => {
                let line = self.tok.line;
                self.bump()?;
                let value = self.parse_expression()?;
                self.expect_line_end()?;
                Ok(Stmt::Throw { value, line })
            }
            b"break" => {
                let line = self.tok.line;
                self.bump()?;
                self.expect_line_end()?;
                Ok(Stmt::Break { line })
            }
            b"continue" => {
                let line = self.tok.line;
                self.bump()?;
                self.expect_line_end()?;
                Ok(Stmt::Continue { line })
            }
            b"defer" => {
                let line = self.tok.line;
                self.bump()?;
                let expr = self.parse_expression()?;
                self.expect_line_end()?;
                Ok(Stmt::Defer { expr, line })
            }
            b"import" => self.parse_import(),
            b"export" => self.parse_export(),
            b"class" => self.parse_class(false),
            b"abstract" => {
                let line = self.tok.line;
                self.bump()?;
                if self.tok.kind.is_ident("class") {
                    self.parse_class(true)
                } else {
                    Err(error(
                        codes::E1030,
                        "E1030: Using reserved name: abstract",
                        line,
                    ))
                }
            }
            b"interface" => self.parse_interface(),
            b"enum" => self.parse_enum(),
            b"type" => self.parse_type_alias(),
            b"vim9script" => Err(self.err(codes::E1030, "E1030: Using reserved name: vim9script")),
            b"public" | b"private" | b"static" => Err(self.err(
                codes::E1030,
                format!("E1030: Using reserved name: {}", word.to_string_lossy()),
            )),
            other if is_block_terminator_bytes(other) => Err(unexpected_terminator(
                &word.to_string_lossy(),
                self.tok.line,
            )),
            _ => self.parse_expression_statement(),
        }
    }

    /// Vim9 requires white space before and after "=" (E1004).  A line break
    /// after "=" continues the expression; heredocs have their own syntax.
    fn check_assign_spacing(&self, spaced_before: bool) -> Result<()> {
        if !spaced_before {
            return Err(self.err(1004, "E1004: White space required before and after '='"));
        }
        if matches!(self.tok.kind, TokenKind::Newline | TokenKind::Eof) {
            return Ok(());
        }
        if matches!(self.tok.kind, TokenKind::Heredoc(_)) {
            return Ok(());
        }
        if !self.tok.space_before {
            return Err(self.err(1004, "E1004: White space required before and after '='"));
        }
        Ok(())
    }

    fn parse_expression_statement(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        let expr = self.parse_expression()?;
        if let Some(op) = assign_op_of(&self.tok.kind) {
            let spaced_before = self.tok.space_before;
            self.bump()?;
            self.check_assign_spacing(spaced_before)?;
            self.skip_newlines()?;
            let value = self.parse_expression()?;
            self.expect_line_end()?;
            Ok(Stmt::Assign {
                target: expr,
                op,
                value,
                line,
            })
        } else {
            self.expect_line_end()?;
            Ok(Stmt::Expr { expr, line })
        }
    }

    fn parse_var_decl(&mut self, kind: VarKind) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let pattern = self.parse_var_pattern(true)?;
        let mut ty = None;
        if self.eat_punct(Punct::Colon)? {
            self.skip_newlines()?;
            ty = Some(self.parse_type()?);
        }
        // A simple name pattern may already carry the type (for destructuring
        // each item has its own type).
        if ty.is_none() {
            if let VarPattern::Name(name) = &pattern {
                ty = name.ty.clone();
            }
        }
        let mut init = None;
        if self.tok.kind.is_punct(Punct::Assign) {
            let spaced_before = self.tok.space_before;
            self.bump()?;
            self.check_assign_spacing(spaced_before)?;
            self.skip_newlines()?;
            init = Some(self.parse_expression()?);
        }
        self.expect_line_end()?;
        Ok(Stmt::Var(VarDecl {
            kind,
            pattern,
            ty,
            init,
            line,
        }))
    }

    fn parse_var_pattern(&mut self, allow_types: bool) -> Result<VarPattern> {
        self.skip_newlines()?;
        if self.eat_punct(Punct::LBracket)? {
            let mut items = Vec::new();
            let mut rest = None;
            loop {
                self.skip_newlines()?;
                if self.eat_punct(Punct::RBracket)? {
                    break;
                }
                if self.eat_punct(Punct::Semicolon)? {
                    self.skip_newlines()?;
                    rest = Some(Box::new(self.parse_pattern_name(allow_types)?));
                    self.skip_newlines()?;
                    self.expect_punct(Punct::RBracket, codes::E111, "Missing ]")?;
                    break;
                }
                items.push(self.parse_pattern_name(allow_types)?);
                self.skip_newlines()?;
                if self.eat_punct(Punct::Comma)? {
                    continue;
                }
                if self.eat_punct(Punct::Semicolon)? {
                    self.skip_newlines()?;
                    rest = Some(Box::new(self.parse_pattern_name(allow_types)?));
                    self.skip_newlines()?;
                }
                self.expect_punct(Punct::RBracket, codes::E111, "Missing ]")?;
                break;
            }
            Ok(VarPattern::List { items, rest })
        } else {
            self.parse_pattern_name(allow_types)
        }
    }

    /// Parse one member name (class/interface members cannot be destructured).
    fn parse_member_name(&mut self) -> Result<NamePattern> {
        match self.parse_pattern_name(false)? {
            VarPattern::Name(name) => Ok(name),
            VarPattern::List { .. } => Err(self.err(codes::E1004, "Expected member name")),
        }
    }

    fn parse_pattern_name(&mut self, allow_types: bool) -> Result<VarPattern> {
        let (scope, name) = match &self.tok.kind {
            TokenKind::Ident(name) => (None, name.clone()),
            TokenKind::ScopedVar(scope, name) => (Some(*scope), name.clone()),
            _ => return Err(self.err(codes::E1004, "Expected variable name")),
        };
        self.bump()?;
        let mut ty = None;
        if allow_types && self.eat_punct(Punct::Colon)? {
            self.skip_newlines()?;
            ty = Some(self.parse_type()?);
        }
        Ok(VarPattern::Name(NamePattern { scope, name, ty }))
    }

    fn parse_if(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let cond = self.parse_expression()?;
        let saved_bar = self.bar_mode;
        self.begin_block_body(saved_bar)?;
        let mut branches = Vec::new();
        let (body, mut terminator) = self.parse_block(&["elseif", "else", "endif"])?;
        branches.push(IfBranch { cond, body, line });
        let mut else_body = None;
        loop {
            let Some(terminator_now) = terminator.take() else {
                return Err(error(codes::E580, "Missing :endif", line));
            };
            match terminator_now.word.as_str() {
                "elseif" => {
                    let cond = self.parse_expression()?;
                    self.begin_block_body(saved_bar)?;
                    let (body, next) = self.parse_block(&["elseif", "else", "endif"])?;
                    branches.push(IfBranch {
                        cond,
                        body,
                        line: terminator_now.line,
                    });
                    terminator = next;
                }
                "else" => {
                    self.begin_block_body(saved_bar)?;
                    let (body, next) = self.parse_block(&["endif"])?;
                    let Some(next) = next else {
                        return Err(error(codes::E580, "Missing :endif", line));
                    };
                    if next.word != "endif" {
                        return Err(error(codes::E580, "Missing :endif", line));
                    }
                    self.expect_line_end()?;
                    else_body = Some(body);
                    break;
                }
                "endif" => {
                    self.expect_line_end()?;
                    break;
                }
                _ => return Err(error(codes::E580, "Missing :endif", line)),
            }
        }
        self.bar_mode = saved_bar;
        Ok(Stmt::If(IfStmt {
            branches,
            else_body,
            line,
        }))
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let cond = self.parse_expression()?;
        let saved_bar = self.bar_mode;
        self.begin_block_body(saved_bar)?;
        let (body, terminator) = self.parse_block(&["endwhile"])?;
        let Some(terminator) = terminator else {
            return Err(error(codes::E581, "Missing :endwhile", line));
        };
        if terminator.word != "endwhile" {
            return Err(error(codes::E581, "Missing :endwhile", line));
        }
        self.expect_line_end()?;
        self.bar_mode = saved_bar;
        Ok(Stmt::While(WhileStmt { cond, body, line }))
    }

    fn parse_for(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let pattern = self.parse_var_pattern(true)?;
        if !self.eat_ident("in")? {
            return Err(self.err(codes::E488, "Missing in after :for variable"));
        }
        self.skip_newlines()?;
        let iterable = self.parse_expression()?;
        let saved_bar = self.bar_mode;
        self.begin_block_body(saved_bar)?;
        let (body, terminator) = self.parse_block(&["endfor"])?;
        let Some(terminator) = terminator else {
            return Err(error(codes::E582, "Missing :endfor", line));
        };
        if terminator.word != "endfor" {
            return Err(error(codes::E582, "Missing :endfor", line));
        }
        self.expect_line_end()?;
        self.bar_mode = saved_bar;
        Ok(Stmt::For(ForStmt {
            pattern,
            iterable,
            body,
            line,
        }))
    }

    fn parse_try(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        self.expect_line_end()?;
        let (body, mut terminator) = self.parse_block(&["catch", "finally", "endtry"])?;
        let mut catches = Vec::new();
        let mut finally_body = None;
        loop {
            let Some(terminator_now) = terminator.take() else {
                return Err(error(codes::E584, "Missing :endtry", line));
            };
            match terminator_now.word.as_str() {
                "catch" => {
                    let pattern = if matches!(self.tok.kind, TokenKind::Newline | TokenKind::Eof) {
                        self.expect_line_end()?;
                        None
                    } else {
                        Some(self.capture_rest_of_line()?)
                    };
                    let (body, next) = self.parse_block(&["catch", "finally", "endtry"])?;
                    catches.push(CatchClause {
                        pattern,
                        body,
                        line: terminator_now.line,
                    });
                    terminator = next;
                }
                "finally" => {
                    self.expect_line_end()?;
                    let (body, next) = self.parse_block(&["endtry"])?;
                    let Some(next) = next else {
                        return Err(error(codes::E584, "Missing :endtry", line));
                    };
                    if next.word != "endtry" {
                        return Err(error(codes::E584, "Missing :endtry", line));
                    }
                    self.expect_line_end()?;
                    finally_body = Some(body);
                    break;
                }
                "endtry" => {
                    self.expect_line_end()?;
                    break;
                }
                _ => return Err(error(codes::E584, "Missing :endtry", line)),
            }
        }
        if catches.is_empty() && finally_body.is_none() {
            return Err(error(codes::E1032, "Missing :catch or :finally", line));
        }
        Ok(Stmt::Try(TryStmt {
            body,
            catches,
            finally_body,
            line,
        }))
    }

    /// Capture the rest of the current line verbatim (used for :catch
    /// patterns, which are regexes, not Vim9 expressions).
    fn capture_rest_of_line(&mut self) -> Result<VimString> {
        let line = self.tok.line;
        let (_, end, next) = line_bounds(self.source, self.tok.start);
        let mut begin = self.tok.start;
        while begin < end && matches!(self.source[begin], b' ' | b'\t') {
            begin += 1;
        }
        let text = VimString::from_bytes(self.source[begin..end].to_vec());
        self.seek_to_line(next, line + 1)?;
        Ok(text)
    }

    fn parse_def(&mut self, is_static: bool, is_abstract: bool) -> Result<DefDecl> {
        let line = self.tok.line;
        debug_assert!(self.tok.kind.is_ident("def"));
        self.bump()?;
        let name = self.expect_function_name()?;
        let generics = self.parse_generics()?;
        self.expect_punct(Punct::LParen, codes::E110, "Missing (")?;
        let params = self.parse_params()?;
        let mut ret = None;
        if self.eat_punct(Punct::Colon)? {
            self.skip_newlines()?;
            ret = Some(self.parse_type()?);
        }
        self.expect_line_end()?;
        let body = if is_abstract {
            Vec::new()
        } else {
            let (body, terminator) = self.parse_block(&["enddef"])?;
            let Some(terminator) = terminator else {
                return Err(error(codes::E1098, "Missing :enddef", line));
            };
            if terminator.word != "enddef" {
                return Err(error(codes::E1098, "Missing :enddef", line));
            }
            self.expect_line_end()?;
            body
        };
        Ok(DefDecl {
            name,
            generics,
            params,
            ret,
            body,
            is_abstract,
            is_static,
            line,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>> {
        let mut params = Vec::new();
        loop {
            self.skip_newlines()?;
            if self.eat_punct(Punct::RParen)? {
                break;
            }
            let line = self.tok.line;
            let varargs = self.eat_punct(Punct::Ellipsis)?;
            let optional = self.eat_punct(Punct::Question)?;
            let name = self.parse_param_name()?;
            let mut ty = None;
            if self.eat_punct(Punct::Colon)? {
                self.skip_newlines()?;
                ty = Some(self.parse_type()?);
            }
            let mut default = None;
            if self.tok.kind.is_punct(Punct::Assign) {
                // Vim accepts "def F(a=1)" even though declarations require
                // white space around "=".
                self.bump()?;
                self.skip_newlines()?;
                default = Some(self.parse_expression()?);
            }
            params.push(Param {
                name,
                ty,
                default,
                optional,
                varargs,
                line,
            });
            self.skip_newlines()?;
            if self.eat_punct(Punct::Comma)? {
                continue;
            }
            self.expect_punct(Punct::RParen, codes::E110, "Missing )")?;
            break;
        }
        Ok(params)
    }

    /// Function names may be scoped: "def g:GlobalFunc()".
    fn expect_function_name(&mut self) -> Result<VimString> {
        match &self.tok.kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.bump()?;
                Ok(name)
            }
            TokenKind::ScopedVar(scope, name) => {
                let scope = *scope;
                let name = name.clone();
                self.bump()?;
                Ok(VimString::from(format!(
                    "{}{}",
                    scope.prefix(),
                    name.to_string_lossy()
                )))
            }
            _ => Err(self.err(codes::E1004, "E1004: Expected function name")),
        }
    }

    /// Parse a parameter name.  Enum constructors use "this.member" names.
    fn parse_param_name(&mut self) -> Result<VimString> {
        if self.tok.kind.is_ident("this") && self.peek_nth(1)?.kind.is_punct(Punct::Dot) {
            self.bump()?;
            self.bump()?;
            let member = self.expect_ident("member name")?;
            return Ok(VimString::from(format!(
                "this.{}",
                member.to_string_lossy()
            )));
        }
        self.expect_ident("argument name")
    }

    fn parse_generics(&mut self) -> Result<Vec<VimString>> {
        let mut generics = Vec::new();
        if !self.tok.kind.is_punct(Punct::Lt) {
            return Ok(generics);
        }
        self.bump()?;
        loop {
            let name = self.expect_ident("type parameter")?;
            generics.push(name);
            if self.eat_punct(Punct::Comma)? {
                continue;
            }
            break;
        }
        self.expect_punct(Punct::Gt, codes::E488, "Missing >")?;
        Ok(generics)
    }

    fn parse_class(&mut self, is_abstract: bool) -> Result<Stmt> {
        let line = self.tok.line;
        if !self.eat_ident("class")? {
            return Err(self.err(codes::E1030, "Expected :class"));
        }
        let name = self.expect_ident("class name")?;
        let generics = self.parse_generics()?;
        let mut extends = None;
        if self.eat_ident("extends")? {
            extends = Some(self.parse_type()?);
        }
        let mut implements = Vec::new();
        if self.eat_ident("implements")? {
            loop {
                implements.push(self.parse_type()?);
                if self.eat_punct(Punct::Comma)? {
                    continue;
                }
                break;
            }
        }
        self.expect_line_end()?;
        let mut members = Vec::new();
        loop {
            self.skip_newlines()?;
            if matches!(self.tok.kind, TokenKind::Eof) {
                return Err(error(codes::E1065, "Missing :endclass", line));
            }
            if self.eat_ident("endclass")? {
                self.expect_line_end()?;
                break;
            }
            let member_line = self.tok.line;
            let mut access = Access::Private;
            let mut is_static = false;
            let mut is_abstract_member = false;
            loop {
                if self.eat_ident("public")? {
                    access = Access::Public;
                    continue;
                }
                if self.eat_ident("private")? {
                    access = Access::Private;
                    continue;
                }
                if self.eat_ident("static")? {
                    is_static = true;
                    continue;
                }
                if self.eat_ident("abstract")? {
                    is_abstract_member = true;
                    continue;
                }
                break;
            }
            if self.eat_ident("var")? {
                let pattern = self.parse_member_name()?;
                let mut ty = None;
                if self.eat_punct(Punct::Colon)? {
                    self.skip_newlines()?;
                    ty = Some(self.parse_type()?);
                }
                let mut init = None;
                if self.tok.kind.is_punct(Punct::Assign) {
                    let spaced_before = self.tok.space_before;
                    self.bump()?;
                    self.check_assign_spacing(spaced_before)?;
                    self.skip_newlines()?;
                    init = Some(self.parse_expression()?);
                }
                self.expect_line_end()?;
                members.push(ClassMember::Var {
                    access,
                    is_static,
                    is_public: access == Access::Public,
                    pattern,
                    ty,
                    init,
                    line: member_line,
                });
            } else if self.tok.kind.is_ident("def") {
                let def = self.parse_def(is_static, is_abstract_member)?;
                members.push(ClassMember::Def { access, def });
            } else {
                return Err(error(
                    codes::E1065,
                    "E1065: Expected variable or method in class",
                    member_line,
                ));
            }
        }
        Ok(Stmt::Class(ClassDecl {
            name,
            generics,
            is_abstract,
            extends,
            implements,
            members,
            line,
        }))
    }

    fn parse_interface(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let name = self.expect_ident("interface name")?;
        let generics = self.parse_generics()?;
        let mut extends = Vec::new();
        if self.eat_ident("extends")? {
            loop {
                extends.push(self.parse_type()?);
                if self.eat_punct(Punct::Comma)? {
                    continue;
                }
                break;
            }
        }
        self.expect_line_end()?;
        let mut members = Vec::new();
        loop {
            self.skip_newlines()?;
            if matches!(self.tok.kind, TokenKind::Eof) {
                return Err(error(codes::E1094, "Missing :endinterface", line));
            }
            if self.eat_ident("endinterface")? {
                self.expect_line_end()?;
                break;
            }
            let member_line = self.tok.line;
            let access = Access::Public;
            if self.eat_ident("var")? {
                let pattern = self.parse_member_name()?;
                let mut ty = None;
                if self.eat_punct(Punct::Colon)? {
                    self.skip_newlines()?;
                    ty = Some(self.parse_type()?);
                }
                self.expect_line_end()?;
                members.push(ClassMember::Var {
                    access,
                    is_static: false,
                    is_public: true,
                    pattern,
                    ty,
                    init: None,
                    line: member_line,
                });
            } else if self.tok.kind.is_ident("def") {
                // Interface methods are declarations without a body and
                // without :enddef.
                let def = self.parse_def(false, true)?;
                members.push(ClassMember::Def { access, def });
            } else {
                return Err(error(
                    codes::E1094,
                    "E1094: Expected variable or method in interface",
                    member_line,
                ));
            }
        }
        Ok(Stmt::Interface(InterfaceDecl {
            name,
            generics,
            extends,
            members,
            line,
        }))
    }

    fn parse_enum(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let name = self.expect_ident("enum name")?;
        self.expect_line_end()?;
        let mut members = Vec::new();
        let mut body = Vec::new();
        loop {
            self.skip_newlines()?;
            if matches!(self.tok.kind, TokenKind::Eof) {
                return Err(error(codes::E1095, "Missing :endenum", line));
            }
            if self.eat_ident("endenum")? {
                self.expect_line_end()?;
                break;
            }
            if self.tok.kind.is_ident("var")
                || self.tok.kind.is_ident("def")
                || self.tok.kind.is_ident("public")
                || self.tok.kind.is_ident("private")
                || self.tok.kind.is_ident("static")
            {
                // Members are followed by the enum body: member variables
                // and the constructor.
                let (statements, terminator) = self.parse_block(&["endenum"])?;
                let Some(terminator) = terminator else {
                    return Err(error(codes::E1095, "Missing :endenum", line));
                };
                if terminator.word != "endenum" {
                    return Err(error(codes::E1095, "Missing :endenum", line));
                }
                self.expect_line_end()?;
                body = statements;
                break;
            }
            let member_line = self.tok.line;
            let member = self.expect_ident("enum member")?;
            let args = if self.tok.kind.is_punct(Punct::LParen) {
                self.parse_call_args()?
            } else {
                Vec::new()
            };
            members.push(EnumMember {
                name: member,
                args,
                line: member_line,
            });
            if self.eat_punct(Punct::Comma)? {
                continue;
            }
            self.expect_line_end()?;
        }
        Ok(Stmt::Enum(EnumDecl {
            name,
            members,
            body,
            line,
        }))
    }

    fn parse_type_alias(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let name = self.expect_ident("type name")?;
        let generics = self.parse_generics()?;
        self.expect_punct(Punct::Assign, 398, "Missing = in :type")?;
        self.skip_newlines()?;
        let ty = self.parse_type()?;
        self.expect_line_end()?;
        Ok(Stmt::TypeAlias(TypeAliasDecl {
            name,
            generics,
            ty,
            line,
        }))
    }

    fn parse_import(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let autoload = self.eat_ident("autoload")?;
        self.skip_newlines()?;
        let path = self.parse_expression()?;
        let alias = if self.eat_ident("as")? {
            Some(self.expect_ident("import name")?)
        } else {
            None
        };
        self.expect_line_end()?;
        Ok(Stmt::Import(ImportDecl {
            autoload,
            path: Box::new(path),
            alias,
            line,
        }))
    }

    fn parse_export(&mut self) -> Result<Stmt> {
        let line = self.tok.line;
        self.bump()?;
        let word = match &self.tok.kind {
            TokenKind::Ident(word) => word.clone(),
            _ => {
                return Err(error(
                    codes::E1030,
                    "E1043: Invalid command after :export",
                    line,
                ))
            }
        };
        let item = match word.as_bytes() {
            b"def" => {
                let def = self.parse_def(false, false)?;
                Stmt::Def(def)
            }
            b"var" => self.parse_var_decl(VarKind::Var)?,
            b"final" => self.parse_var_decl(VarKind::Final)?,
            b"const" => self.parse_var_decl(VarKind::Const)?,
            b"class" => self.parse_class(false)?,
            b"abstract" => {
                self.bump()?;
                if !self.tok.kind.is_ident("class") {
                    return Err(error(
                        codes::E1030,
                        "E1043: Invalid command after :export",
                        line,
                    ));
                }
                self.parse_class(true)?
            }
            b"interface" => self.parse_interface()?,
            b"enum" => self.parse_enum()?,
            b"type" => self.parse_type_alias()?,
            _ => {
                return Err(error(
                    codes::E1030,
                    format!(
                        "E1043: Invalid command after :export: {}",
                        word.to_string_lossy()
                    ),
                    line,
                ))
            }
        };
        Ok(Stmt::Export(ExportDecl {
            item: Box::new(item),
            line,
        }))
    }

    // ---------------------------------------------------------------
    // Type expressions
    // ---------------------------------------------------------------

    fn parse_type(&mut self) -> Result<TypeExpr> {
        if self.eat_punct(Punct::Question)? {
            let inner = self.parse_type()?;
            return Ok(TypeExpr::Optional(Box::new(inner)));
        }
        let name = match &self.tok.kind {
            TokenKind::Ident(name) => name.clone(),
            _ => return Err(self.err(codes::E1004, "E1008: Missing type")),
        };
        self.bump()?;
        match name.as_bytes() {
            b"list" if self.tok.kind.is_punct(Punct::Lt) => {
                self.bump()?;
                let inner = self.parse_type()?;
                self.expect_punct(Punct::Gt, 1009, "E1009: Missing >")?;
                Ok(TypeExpr::List(Box::new(inner)))
            }
            b"dict" if self.tok.kind.is_punct(Punct::Lt) => {
                self.bump()?;
                let inner = self.parse_type()?;
                self.expect_punct(Punct::Gt, 1009, "E1009: Missing >")?;
                Ok(TypeExpr::Dict(Box::new(inner)))
            }
            b"tuple" if self.tok.kind.is_punct(Punct::Lt) => {
                self.bump()?;
                let mut items = Vec::new();
                let mut variadic = None;
                loop {
                    self.skip_newlines()?;
                    if self.tok.kind.is_punct(Punct::Gt) {
                        break;
                    }
                    if self.eat_punct(Punct::Ellipsis)? {
                        variadic = Some(Box::new(self.parse_type()?));
                    } else {
                        items.push(self.parse_type()?);
                    }
                    self.skip_newlines()?;
                    if self.eat_punct(Punct::Comma)? {
                        continue;
                    }
                    break;
                }
                self.expect_punct(Punct::Gt, 1009, "E1009: Missing >")?;
                Ok(TypeExpr::Tuple { items, variadic })
            }
            b"func" if self.tok.kind.is_punct(Punct::Colon) => {
                self.bump()?;
                self.skip_newlines()?;
                let ret = self.parse_type()?;
                Ok(TypeExpr::Func(Some(FuncType {
                    params: None,
                    varargs: None,
                    ret: Some(Box::new(ret)),
                })))
            }
            b"func" if self.tok.kind.is_punct(Punct::LParen) => {
                self.bump()?;
                let mut params = Some(Vec::new());
                let mut varargs = None;
                loop {
                    self.skip_newlines()?;
                    if self.eat_punct(Punct::RParen)? {
                        break;
                    }
                    if self.eat_punct(Punct::Ellipsis)? {
                        varargs = Some(Box::new(self.parse_type()?));
                    } else if let Some(params) = params.as_mut() {
                        params.push(self.parse_type()?);
                    }
                    self.skip_newlines()?;
                    if self.eat_punct(Punct::Comma)? {
                        continue;
                    }
                    self.expect_punct(Punct::RParen, codes::E110, "Missing )")?;
                    break;
                }
                let mut ret = None;
                if self.eat_punct(Punct::Colon)? {
                    self.skip_newlines()?;
                    ret = Some(Box::new(self.parse_type()?));
                }
                Ok(TypeExpr::Func(Some(FuncType {
                    params,
                    varargs,
                    ret,
                })))
            }
            b"func" => Ok(TypeExpr::Func(None)),
            _ => Ok(TypeExpr::Named(name)),
        }
    }

    // ---------------------------------------------------------------
    // Expressions
    // ---------------------------------------------------------------

    fn parse_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_binary(0)?;
        loop {
            if matches!(self.tok.kind, TokenKind::Newline) {
                let _ = self
                    .consume_newlines_before_punct(&[Punct::Question, Punct::QuestionQuestion])?;
            }
            if self.eat_punct(Punct::QuestionQuestion)? {
                self.skip_newlines()?;
                let right = self.parse_expression()?;
                left = Expr::Binary {
                    op: BinaryOp::Coalesce,
                    left: Box::new(left),
                    right: Box::new(right),
                };
                continue;
            }
            if self.eat_punct(Punct::Question)? {
                self.skip_newlines()?;
                let then_value = self.parse_expression()?;
                let _ = self.consume_newlines_before_punct(&[Punct::Colon])?;
                self.expect_punct(Punct::Colon, codes::E110, "Missing : in ternary")?;
                self.skip_newlines()?;
                let else_value = self.parse_expression()?;
                return Ok(Expr::Ternary {
                    cond: Box::new(left),
                    then_value: Box::new(then_value),
                    else_value: Box::new(else_value),
                });
            }
            break;
        }
        Ok(left)
    }

    fn parse_binary(&mut self, min_precedence: u8) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            if matches!(self.tok.kind, TokenKind::Newline) {
                let _ = self.consume_newlines_before_binary_op()?;
            }
            let Some((op, precedence)) = binary_op_of(&self.tok.kind) else {
                break;
            };
            if precedence < min_precedence {
                break;
            }
            self.bump()?;
            self.skip_newlines()?;
            let right = self.parse_binary(precedence + 1)?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Consume newlines when the next non-newline token is a binary operator.
    fn consume_newlines_before_binary_op(&mut self) -> Result<bool> {
        if !matches!(self.tok.kind, TokenKind::Newline) {
            return Ok(false);
        }
        let mut count = 0usize;
        loop {
            let token = self.peek_nth(count + 1)?;
            if matches!(token.kind, TokenKind::Newline) {
                count += 1;
                continue;
            }
            if binary_op_of(&token.kind).is_some() {
                for _ in 0..=count {
                    self.bump()?;
                }
                return Ok(true);
            }
            return Ok(false);
        }
    }

    /// Consume newlines when the next non-newline token is one of "puncts".
    fn consume_newlines_before_punct(&mut self, puncts: &[Punct]) -> Result<bool> {
        if !matches!(self.tok.kind, TokenKind::Newline) {
            return Ok(false);
        }
        let mut count = 0usize;
        loop {
            let token = self.peek_nth(count + 1)?;
            if matches!(token.kind, TokenKind::Newline) {
                count += 1;
                continue;
            }
            if let TokenKind::Punct(punct) = &token.kind {
                if puncts.contains(punct) {
                    for _ in 0..=count {
                        self.bump()?;
                    }
                    return Ok(true);
                }
            }
            return Ok(false);
        }
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        let op = match &self.tok.kind {
            TokenKind::Punct(Punct::Bang) => Some(UnaryOp::Not),
            TokenKind::Punct(Punct::Minus) => Some(UnaryOp::Negate),
            TokenKind::Punct(Punct::Plus) => Some(UnaryOp::Plus),
            _ => None,
        };
        if let Some(op) = op {
            self.bump()?;
            self.skip_newlines()?;
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                op,
                operand: Box::new(operand),
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.tok.kind, TokenKind::Newline)
                && !self.consume_newlines_before_punct(&[Punct::Dot, Punct::Arrow])?
            {
                break;
            }
            // Explicit generic type arguments: Name<T, U>(args).
            if self.tok.kind.is_punct(Punct::Lt)
                && !self.tok.space_before
                && self.generic_call_ahead()?
            {
                self.bump()?; // '<'
                let mut type_args = Vec::new();
                loop {
                    type_args.push(self.parse_type()?);
                    if self.eat_punct(Punct::Comma)? {
                        continue;
                    }
                    break;
                }
                self.expect_punct(Punct::Gt, codes::E488, "Missing >")?;
                let args = self.parse_call_args()?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    type_args,
                };
                continue;
            }
            match self.tok.kind {
                TokenKind::Punct(Punct::LParen) => {
                    let args = self.parse_call_args()?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        type_args: Vec::new(),
                    };
                }
                TokenKind::Punct(Punct::LBracket) => {
                    expr = self.parse_subscript(expr)?;
                }
                TokenKind::Punct(Punct::Dot) => {
                    self.bump()?;
                    let key = self.expect_member_name()?;
                    expr = Expr::Member {
                        base: Box::new(expr),
                        key,
                    };
                }
                TokenKind::Punct(Punct::Arrow) => {
                    self.bump()?;
                    let callee = self.parse_arrow_callee()?;
                    let args = if self.tok.kind.is_punct(Punct::LParen) {
                        self.parse_call_args()?
                    } else {
                        Vec::new()
                    };
                    expr = Expr::MethodCall {
                        receiver: Box::new(expr),
                        callee: Box::new(callee),
                        args,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn expect_member_name(&mut self) -> Result<VimString> {
        match &self.tok.kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.bump()?;
                Ok(name)
            }
            _ => Err(self.err(1127, "E1127: Missing name after .")),
        }
    }

    fn parse_arrow_callee(&mut self) -> Result<Expr> {
        match &self.tok.kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.bump()?;
                Ok(Expr::Ident { scope: None, name })
            }
            TokenKind::ScopedVar(scope, name) => {
                let scope = *scope;
                let name = name.clone();
                self.bump()?;
                Ok(Expr::Ident {
                    scope: Some(scope),
                    name,
                })
            }
            TokenKind::Punct(Punct::LParen) => self.parse_primary(),
            _ => Err(self.err(1085, "E1085: Missing name after ->")),
        }
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>> {
        self.expect_punct(Punct::LParen, codes::E110, "Missing (")?;
        self.skip_newlines()?;
        let mut args = Vec::new();
        if !self.tok.kind.is_punct(Punct::RParen) {
            loop {
                args.push(self.parse_expression()?);
                self.skip_newlines()?;
                if self.eat_punct(Punct::Comma)? {
                    self.skip_newlines()?;
                    if self.tok.kind.is_punct(Punct::RParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect_punct(Punct::RParen, codes::E110, "Missing )")?;
        Ok(args)
    }

    fn parse_subscript(&mut self, base: Expr) -> Result<Expr> {
        self.expect_punct(Punct::LBracket, codes::E111, "Missing [")?;
        self.skip_newlines()?;
        if self.eat_punct(Punct::Colon)? {
            self.skip_newlines()?;
            let end = if self.tok.kind.is_punct(Punct::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expression()?))
            };
            self.skip_newlines()?;
            self.expect_punct(Punct::RBracket, codes::E111, "Missing ]")?;
            return Ok(Expr::Slice {
                base: Box::new(base),
                start: None,
                end,
            });
        }
        let first = self.parse_expression()?;
        self.skip_newlines()?;
        if self.eat_punct(Punct::Colon)? {
            self.skip_newlines()?;
            let end = if self.tok.kind.is_punct(Punct::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expression()?))
            };
            self.skip_newlines()?;
            self.expect_punct(Punct::RBracket, codes::E111, "Missing ]")?;
            return Ok(Expr::Slice {
                base: Box::new(base),
                start: Some(Box::new(first)),
                end,
            });
        }
        self.expect_punct(Punct::RBracket, codes::E111, "Missing ]")?;
        Ok(Expr::Index {
            base: Box::new(base),
            index: Box::new(first),
        })
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self.tok.clone();
        match token.kind {
            TokenKind::Number(value) => {
                self.bump()?;
                Ok(Expr::Number(value))
            }
            TokenKind::Float(value) => {
                self.bump()?;
                Ok(Expr::Float(value))
            }
            TokenKind::String(value) => {
                self.bump()?;
                Ok(Expr::String(value))
            }
            TokenKind::Blob(value) => {
                self.bump()?;
                Ok(Expr::Blob(value))
            }
            TokenKind::InterpolatedString(parts) => {
                self.bump()?;
                let mut segments = Vec::new();
                for part in parts {
                    match part {
                        InterpPart::Text(text) => segments.push(InterpSegment::Text(text)),
                        InterpPart::Expr { source, line } => {
                            let expr = parse_expression_source(source.as_bytes(), line)?;
                            segments.push(InterpSegment::Value(Box::new(expr)));
                        }
                    }
                }
                Ok(Expr::Interpolated(segments))
            }
            TokenKind::Heredoc(heredoc) => {
                self.bump()?;
                Ok(Expr::Heredoc {
                    lines: heredoc.lines,
                    flags: heredoc.flags,
                })
            }
            TokenKind::Ident(name) => {
                self.bump()?;
                match name.as_bytes() {
                    b"true" => Ok(Expr::Bool(true)),
                    b"false" => Ok(Expr::Bool(false)),
                    _ if is_null_name(name.as_bytes()) => Ok(Expr::Special(name)),
                    _ => Ok(Expr::Ident { scope: None, name }),
                }
            }
            TokenKind::ScopedVar(scope, name) => {
                self.bump()?;
                Ok(Expr::Ident {
                    scope: Some(scope),
                    name,
                })
            }
            TokenKind::Option(name) => {
                self.bump()?;
                let (scope, name) = split_option_scope(&name);
                Ok(Expr::Option { scope, name })
            }
            TokenKind::Env(name) => {
                self.bump()?;
                Ok(Expr::Env(name))
            }
            TokenKind::Register(reg) => {
                self.bump()?;
                Ok(Expr::Register(reg))
            }
            TokenKind::Punct(Punct::LParen) => self.parse_paren_or_lambda(),
            TokenKind::Punct(Punct::LBracket) => self.parse_list_literal(),
            TokenKind::Punct(Punct::LBrace) => self.parse_dict_literal(),
            TokenKind::Newline | TokenKind::Eof => {
                Err(self.err(codes::E1097, "E1097: Line incomplete"))
            }
            _ => Err(self.err(
                codes::E488,
                format!("E488: Unexpected token: {}", self.line_snippet()),
            )),
        }
    }

    fn parse_paren_or_lambda(&mut self) -> Result<Expr> {
        if self.lambda_ahead()? {
            return self.parse_lambda();
        }
        self.expect_punct(Punct::LParen, codes::E110, "Missing (")?;
        self.skip_newlines()?;
        if self.tok.kind.is_punct(Punct::RParen) {
            return Err(self.err(15, "E15: Invalid expression: ()"));
        }
        let first = self.parse_expression()?;
        self.skip_newlines()?;
        if self.eat_punct(Punct::Comma)? {
            let mut items = vec![first];
            loop {
                self.skip_newlines()?;
                if self.tok.kind.is_punct(Punct::RParen) {
                    break;
                }
                items.push(self.parse_expression()?);
                self.skip_newlines()?;
                if self.eat_punct(Punct::Comma)? {
                    continue;
                }
                break;
            }
            self.expect_punct(Punct::RParen, codes::E110, "Missing )")?;
            Ok(Expr::Tuple(items))
        } else {
            self.expect_punct(Punct::RParen, codes::E110, "Missing )")?;
            Ok(first)
        }
    }

    /// True when the current "&lt;" starts explicit generic type arguments
    /// directly followed by a call: "First&lt;number&gt;(...)".
    fn generic_call_ahead(&mut self) -> Result<bool> {
        let mut depth = 0i32;
        let mut index = 1usize;
        loop {
            let token = self.peek_nth(index)?;
            match token.kind {
                TokenKind::Punct(Punct::Lt) => depth += 1,
                TokenKind::Punct(Punct::Gt) => {
                    if depth == 0 {
                        let after = self.peek_nth(index + 1)?;
                        return Ok(after.kind.is_punct(Punct::LParen));
                    }
                    depth -= 1;
                }
                TokenKind::Newline | TokenKind::Eof => return Ok(false),
                _ => {}
            }
            index += 1;
            if index > 512 {
                return Ok(false);
            }
        }
    }

    /// True when the current "(" starts a lambda parameter list, i.e. after
    /// the matching ")" comes "=&gt;" or ": {type} =&gt;".
    fn lambda_ahead(&mut self) -> Result<bool> {
        // The current token is the opening "(".
        let mut depth = 1i32;
        let mut index = 0usize;
        loop {
            let token = self.peek_nth(index + 1)?;
            match token.kind {
                TokenKind::Punct(Punct::LParen)
                | TokenKind::Punct(Punct::LBracket)
                | TokenKind::Punct(Punct::LBrace) => depth += 1,
                TokenKind::Punct(Punct::RParen)
                | TokenKind::Punct(Punct::RBracket)
                | TokenKind::Punct(Punct::RBrace) => {
                    depth -= 1;
                    if depth == 0 {
                        let after = self.peek_nth(index + 2)?;
                        if after.kind.is_punct(Punct::FatArrow) {
                            return Ok(true);
                        }
                        if after.kind.is_punct(Punct::Colon) {
                            // Lambda with an explicit return type: scan the
                            // type tokens for "=>".
                            let mut scan = index + 3;
                            loop {
                                let token = self.peek_nth(scan)?;
                                match token.kind {
                                    TokenKind::Punct(Punct::FatArrow) => return Ok(true),
                                    TokenKind::Newline | TokenKind::Eof => return Ok(false),
                                    _ => {}
                                }
                                scan += 1;
                                if scan > 4192 {
                                    return Ok(false);
                                }
                            }
                        }
                        return Ok(false);
                    }
                    if depth < 0 {
                        return Ok(false);
                    }
                }
                TokenKind::Newline => {
                    if depth > 0 {
                        index += 1;
                        continue;
                    }
                    return Ok(false);
                }
                TokenKind::Eof => return Ok(false),
                _ => {}
            }
            index += 1;
            if index > 4096 {
                return Ok(false);
            }
        }
    }

    fn parse_lambda(&mut self) -> Result<Expr> {
        let line = self.tok.line;
        self.expect_punct(Punct::LParen, codes::E110, "Missing (")?;
        let mut params = Vec::new();
        loop {
            self.skip_newlines()?;
            if self.eat_punct(Punct::RParen)? {
                break;
            }
            let param_line = self.tok.line;
            let varargs = self.eat_punct(Punct::Ellipsis)?;
            let name = self.expect_ident("lambda argument")?;
            let mut ty = None;
            if self.eat_punct(Punct::Colon)? {
                self.skip_newlines()?;
                ty = Some(self.parse_type()?);
            }
            params.push(Param {
                name,
                ty,
                default: None,
                optional: false,
                varargs,
                line: param_line,
            });
            self.skip_newlines()?;
            if self.eat_punct(Punct::Comma)? {
                continue;
            }
            self.expect_punct(Punct::RParen, codes::E110, "Missing )")?;
            break;
        }
        let mut ret = None;
        if self.eat_punct(Punct::Colon)? {
            self.skip_newlines()?;
            ret = Some(self.parse_type()?);
        }
        self.expect_punct(Punct::FatArrow, 1187, "Missing => in lambda")?;
        self.skip_newlines()?;
        let body = if self.tok.kind.is_punct(Punct::LBrace) {
            LambdaBody::Block(self.parse_lambda_block()?)
        } else {
            LambdaBody::Expr(Box::new(self.parse_expression()?))
        };
        Ok(Expr::Lambda(Lambda {
            params,
            ret,
            body,
            line,
        }))
    }

    /// A lambda with a statement block: "=&gt; { ... }".
    fn parse_lambda_block(&mut self) -> Result<Vec<Stmt>> {
        self.expect_punct(Punct::LBrace, codes::E116, "Missing {")?;
        let mut statements = Vec::new();
        loop {
            self.skip_newlines()?;
            if matches!(self.tok.kind, TokenKind::Eof) {
                return Err(self.err(codes::E116, "Missing }"));
            }
            if self.eat_punct(Punct::RBrace)? {
                break;
            }
            if !self.starts_language_statement()? {
                statements.push(self.capture_command()?);
                continue;
            }
            statements.push(self.parse_statement()?);
        }
        Ok(statements)
    }

    fn parse_list_literal(&mut self) -> Result<Expr> {
        self.expect_punct(Punct::LBracket, codes::E111, "Missing [")?;
        let mut items = Vec::new();
        loop {
            self.skip_newlines()?;
            if self.eat_punct(Punct::RBracket)? {
                break;
            }
            items.push(self.parse_expression()?);
            self.skip_newlines()?;
            if self.eat_punct(Punct::Comma)? {
                continue;
            }
            self.expect_punct(Punct::RBracket, codes::E111, "Missing ]")?;
            break;
        }
        Ok(Expr::List(items))
    }

    fn parse_dict_literal(&mut self) -> Result<Expr> {
        self.expect_punct(Punct::LBrace, codes::E116, "Missing {")?;
        let mut entries = Vec::new();
        loop {
            self.skip_newlines()?;
            if self.eat_punct(Punct::RBrace)? {
                break;
            }
            let key = match &self.tok.kind {
                TokenKind::Ident(name) => {
                    let name = name.clone();
                    self.bump()?;
                    DictKey::Literal(name)
                }
                TokenKind::String(value) => {
                    let value = value.clone();
                    self.bump()?;
                    DictKey::String(value)
                }
                TokenKind::Number(_) | TokenKind::Float(_) => {
                    let token = self.tok.clone();
                    let raw = VimString::from_bytes(self.source[token.start..token.end].to_vec());
                    self.bump()?;
                    DictKey::Literal(raw)
                }
                TokenKind::Punct(Punct::LBracket) => {
                    self.bump()?;
                    self.skip_newlines()?;
                    let expr = self.parse_expression()?;
                    self.skip_newlines()?;
                    self.expect_punct(Punct::RBracket, codes::E111, "Missing ]")?;
                    DictKey::Expr(Box::new(expr))
                }
                _ => return Err(self.err(720, "E720: Missing colon in Dictionary")),
            };
            self.skip_newlines()?;
            self.expect_punct(Punct::Colon, 720, "E720: Missing colon in Dictionary")?;
            self.skip_newlines()?;
            let value = self.parse_expression()?;
            entries.push((key, value));
            self.skip_newlines()?;
            if self.eat_punct(Punct::Comma)? {
                continue;
            }
            self.expect_punct(Punct::RBrace, codes::E116, "Missing }")?;
            break;
        }
        Ok(Expr::Dict(entries))
    }
}

fn binary_op_of(kind: &TokenKind) -> Option<(BinaryOp, u8)> {
    let (op, precedence) = match kind {
        TokenKind::Punct(Punct::OrOr) => (BinaryOp::Or, 1),
        TokenKind::Punct(Punct::AndAnd) => (BinaryOp::And, 2),
        TokenKind::Punct(Punct::EqEq) => (BinaryOp::Eq, 3),
        TokenKind::Punct(Punct::NotEq) => (BinaryOp::Ne, 3),
        TokenKind::Punct(Punct::Gt) => (BinaryOp::Gt, 3),
        TokenKind::Punct(Punct::Ge) => (BinaryOp::Ge, 3),
        TokenKind::Punct(Punct::Lt) => (BinaryOp::Lt, 3),
        TokenKind::Punct(Punct::Le) => (BinaryOp::Le, 3),
        TokenKind::Punct(Punct::Match) => (BinaryOp::Match, 3),
        TokenKind::Punct(Punct::NotMatch) => (BinaryOp::NotMatch, 3),
        TokenKind::Punct(Punct::EqEqIgnoreCase) => (BinaryOp::EqIgnoreCase, 3),
        TokenKind::Punct(Punct::NotEqIgnoreCase) => (BinaryOp::NeIgnoreCase, 3),
        TokenKind::Punct(Punct::MatchIgnoreCase) => (BinaryOp::MatchIgnoreCase, 3),
        TokenKind::Punct(Punct::NotMatchIgnoreCase) => (BinaryOp::NotMatchIgnoreCase, 3),
        TokenKind::Ident(name) if name.as_bytes() == b"is" => (BinaryOp::Is, 3),
        TokenKind::Ident(name) if name.as_bytes() == b"isnot" => (BinaryOp::IsNot, 3),
        TokenKind::Punct(Punct::DotDot) => (BinaryOp::Concat, 4),
        TokenKind::Punct(Punct::Plus) => (BinaryOp::Add, 4),
        TokenKind::Punct(Punct::Minus) => (BinaryOp::Sub, 4),
        TokenKind::Punct(Punct::Star) => (BinaryOp::Mul, 5),
        TokenKind::Punct(Punct::Slash) => (BinaryOp::Div, 5),
        TokenKind::Punct(Punct::Percent) => (BinaryOp::Rem, 5),
        _ => return None,
    };
    Some((op, precedence))
}

fn assign_op_of(kind: &TokenKind) -> Option<AssignOp> {
    match kind {
        TokenKind::Punct(Punct::Assign) => Some(AssignOp::Assign),
        TokenKind::Punct(Punct::PlusAssign) => Some(AssignOp::Add),
        TokenKind::Punct(Punct::MinusAssign) => Some(AssignOp::Sub),
        TokenKind::Punct(Punct::StarAssign) => Some(AssignOp::Mul),
        TokenKind::Punct(Punct::SlashAssign) => Some(AssignOp::Div),
        TokenKind::Punct(Punct::PercentAssign) => Some(AssignOp::Rem),
        TokenKind::Punct(Punct::DotDotAssign) => Some(AssignOp::Concat),
        _ => None,
    }
}

fn split_option_scope(name: &str) -> (Option<OptionScope>, String) {
    if let Some(rest) = name.strip_prefix("l:") {
        (Some(OptionScope::Local), rest.to_string())
    } else if let Some(rest) = name.strip_prefix("g:") {
        (Some(OptionScope::Global), rest.to_string())
    } else {
        (None, name.to_string())
    }
}

fn is_null_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"null"
            | b"null_blob"
            | b"null_channel"
            | b"null_class"
            | b"null_dict"
            | b"null_function"
            | b"null_job"
            | b"null_list"
            | b"null_object"
            | b"null_partial"
            | b"null_string"
            | b"null_tuple"
    )
}

fn trim_bytes(bytes: &[u8]) -> VimString {
    let mut start = 0;
    while start < bytes.len() && matches!(bytes[start], b' ' | b'\t') {
        start += 1;
    }
    let mut end = bytes.len();
    while end > start && matches!(bytes[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    VimString::from_bytes(bytes[start..end].to_vec())
}

/// A raw command line continues the previous command when it starts with a
/// backslash/bar (Ex syntax) or with an expression operator (Vim9 allows a
/// line break before a binary operator, including inside :echo arguments).
fn is_command_continuation(text: &[u8]) -> bool {
    if text.starts_with(b"\\") || text.starts_with(b"|") {
        return true;
    }
    if text.starts_with(b"..")
        || text.starts_with(b"->")
        || text.starts_with(b"&&")
        || text.starts_with(b"||")
    {
        return true;
    }
    matches!(
        text.first(),
        Some(b'+' | b'-' | b'*' | b'/' | b'%' | b'.' | b'?' | b':')
    )
}

fn trim_start_bytes(bytes: &[u8]) -> &[u8] {
    let mut index = 0;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
        index += 1;
    }
    &bytes[index..]
}

fn is_block_terminator_bytes(word: &[u8]) -> bool {
    matches!(
        word,
        b"enddef"
            | b"elseif"
            | b"else"
            | b"endif"
            | b"endwhile"
            | b"endfor"
            | b"catch"
            | b"finally"
            | b"endtry"
            | b"endclass"
            | b"endinterface"
            | b"endenum"
    )
}

fn unexpected_terminator(word: &str, line: u32) -> VimError {
    let (code, message) = match word {
        "endif" => (codes::E580, ":endif without matching :if"),
        "else" | "elseif" => (codes::E581, ":else without matching :if"),
        "endwhile" => (codes::E582, ":endwhile without matching :while"),
        "endfor" => (codes::E582, ":endfor without matching :for"),
        "endtry" => (codes::E584, ":endtry without matching :try"),
        "catch" | "finally" => (codes::E584, ":catch without matching :try"),
        "enddef" => (codes::E1098, ":enddef without matching :def"),
        "endclass" => (codes::E1065, ":endclass without matching :class"),
        "endinterface" => (codes::E1094, ":endinterface without matching :interface"),
        "endenum" => (codes::E1095, ":endenum without matching :enum"),
        _ => (codes::E488, "Unexpected block terminator"),
    };
    error(code, message, line)
}
