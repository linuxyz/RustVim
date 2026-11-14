//! Byte-oriented lexer for legacy Vim expressions.
//!
//! The lexer is shared by the expression parser and the `:let`/`:for`
//! statement parsers. Strings stay byte arrays, so NUL and non-UTF-8 data are
//! preserved.

use rvim_types::VimString;

use crate::error::{codes, Result, VimError};

/// A variable scope prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Buffer,
    Window,
    Tab,
    Local,
    Argument,
    Script,
    Vim,
}

impl Scope {
    pub fn prefix(self) -> &'static str {
        match self {
            Scope::Global => "g:",
            Scope::Buffer => "b:",
            Scope::Window => "w:",
            Scope::Tab => "t:",
            Scope::Local => "l:",
            Scope::Argument => "a:",
            Scope::Script => "s:",
            Scope::Vim => "v:",
        }
    }

    pub fn from_prefix(prefix: u8) -> Option<Self> {
        match prefix {
            b'g' => Some(Scope::Global),
            b'b' => Some(Scope::Buffer),
            b'w' => Some(Scope::Window),
            b't' => Some(Scope::Tab),
            b'l' => Some(Scope::Local),
            b'a' => Some(Scope::Argument),
            b's' => Some(Scope::Script),
            b'v' => Some(Scope::Vim),
            _ => None,
        }
    }
}

/// Scope of `&opt`, `&l:opt` and `&g:opt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionScope {
    Global,
    Local,
}

/// Binary comparison operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareKind {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Match,
    NotMatch,
}

/// Case-sensitivity suffix of a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseSense {
    /// No suffix: follows `'ignorecase'`.
    Default,
    /// `#` suffix.
    Sensitive,
    /// `?` suffix.
    Insensitive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CmpOp {
    pub kind: CompareKind,
    pub case: CaseSense,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    Is,
    IsNot,
    In,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Concat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Punct {
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Dot,
    DotDot,
    Bang,
    Question,
    Colon,
    Arrow,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Ellipsis,
    AndAnd,
    OrOr,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    DotAssign,
    DotDotAssign,
}

/// One piece of an interpolated string (`$"..."` / `$'...'`).
#[derive(Clone, Debug, PartialEq)]
pub enum InterpPart {
    /// Literal text.
    Text(VimString),
    /// Raw source of an embedded `{expr}`.
    Expression(VimString),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Number(i64),
    Float(f64),
    String(VimString),
    /// `$"..."` / `$'...'`: text and embedded expressions.
    InterpolatedString(Vec<InterpPart>),
    Blob(Vec<u8>),
    Ident(VimString),
    ScopedVar(Scope, VimString),
    Option(Option<OptionScope>, String),
    Env(String),
    Register(u8),
    Keyword(Keyword),
    Cmp(CmpOp),
    Punct(Punct),
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub pos: usize,
    pub line: u32,
}

pub fn tokenize(input: &[u8]) -> Result<Vec<Token>> {
    let mut lexer = Lexer {
        input,
        pos: 0,
        line: 1,
    };
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let eof = token.kind == TokenKind::Eof;
        tokens.push(token);
        if eof {
            break;
        }
    }
    Ok(tokens)
}

struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
    line: u32,
}

impl<'a> Lexer<'a> {
    fn peek(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek(0)?;
        self.pos += 1;
        if byte == b'\n' {
            self.line += 1;
        }
        Some(byte)
    }

    fn token(&self, kind: TokenKind, start: usize) -> Token {
        Token {
            kind,
            pos: start,
            line: self.line,
        }
    }

    fn error(&self, message: impl Into<String>) -> VimError {
        codes::e15(&format!("{} (at byte {})", message.into(), self.pos))
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(0), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.bump();
        }
    }

    fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace();
        let start = self.pos;
        let Some(byte) = self.peek(0) else {
            return Ok(self.token(TokenKind::Eof, start));
        };
        if byte.is_ascii_digit() {
            return self.number(start);
        }
        match byte {
            b'#' if self.peek(1) == Some(b'{') => {
                self.pos += 2;
                Ok(self.token(TokenKind::Punct(Punct::LBrace), start))
            }
            b'\'' | b'"' => self.string(start, byte),
            b'$' if matches!(self.peek(1), Some(b'\'') | Some(b'"')) => {
                let quote = self.peek(1).unwrap_or(b'"');
                self.interpolated_string(start, quote)
            }
            b'$' => self.environment(start),
            b'@' => self.register(start),
            b'&' => {
                if self.peek(1) == Some(b'&') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::AndAnd), start));
                }
                self.option(start)
            }
            b'|' => {
                if self.peek(1) == Some(b'|') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::OrOr), start));
                }
                Err(self.error("unexpected '|'"))
            }
            b'<' if self.starts_with_ignore_case(b"<SID>") => {
                self.pos += 5;
                Ok(self.token(TokenKind::Ident(VimString::from("<SID>")), start))
            }
            b'(' => self.punct(TokenKind::Punct(Punct::LParen), start),
            b')' => self.punct(TokenKind::Punct(Punct::RParen), start),
            b'[' => self.punct(TokenKind::Punct(Punct::LBracket), start),
            b']' => self.punct(TokenKind::Punct(Punct::RBracket), start),
            b'{' => self.punct(TokenKind::Punct(Punct::LBrace), start),
            b'}' => self.punct(TokenKind::Punct(Punct::RBrace), start),
            b',' => self.punct(TokenKind::Punct(Punct::Comma), start),
            b';' => self.punct(TokenKind::Punct(Punct::Semicolon), start),
            b'?' => self.punct(TokenKind::Punct(Punct::Question), start),
            b':' => self.punct(TokenKind::Punct(Punct::Colon), start),
            b'!' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return self.comparison(CompareKind::NotEqual, start);
                }
                if self.peek(1) == Some(b'~') {
                    self.pos += 2;
                    return self.comparison(CompareKind::NotMatch, start);
                }
                self.punct(TokenKind::Punct(Punct::Bang), start)
            }
            b'=' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return self.comparison(CompareKind::Equal, start);
                }
                if self.peek(1) == Some(b'~') {
                    self.pos += 2;
                    return self.comparison(CompareKind::Match, start);
                }
                self.punct(TokenKind::Punct(Punct::Assign), start)
            }
            b'>' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return self.comparison(CompareKind::GreaterEqual, start);
                }
                self.pos += 1;
                self.comparison(CompareKind::Greater, start)
            }
            b'<' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return self.comparison(CompareKind::LessEqual, start);
                }
                self.pos += 1;
                self.comparison(CompareKind::Less, start)
            }
            b'+' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::PlusAssign), start));
                }
                self.punct(TokenKind::Punct(Punct::Plus), start)
            }
            b'-' => {
                if self.peek(1) == Some(b'>') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::Arrow), start));
                }
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::MinusAssign), start));
                }
                self.punct(TokenKind::Punct(Punct::Minus), start)
            }
            b'*' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::StarAssign), start));
                }
                self.punct(TokenKind::Punct(Punct::Star), start)
            }
            b'/' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::SlashAssign), start));
                }
                self.punct(TokenKind::Punct(Punct::Slash), start)
            }
            b'%' => {
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::PercentAssign), start));
                }
                self.punct(TokenKind::Punct(Punct::Percent), start)
            }
            b'.' => {
                if self.peek(1) == Some(b'.') && self.peek(2) == Some(b'.') {
                    self.pos += 3;
                    return Ok(self.token(TokenKind::Punct(Punct::Ellipsis), start));
                }
                if self.peek(1) == Some(b'.') {
                    if self.peek(2) == Some(b'=') {
                        self.pos += 3;
                        return Ok(self.token(TokenKind::Punct(Punct::DotDotAssign), start));
                    }
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::DotDot), start));
                }
                if self.peek(1) == Some(b'=') {
                    self.pos += 2;
                    return Ok(self.token(TokenKind::Punct(Punct::DotAssign), start));
                }
                self.punct(TokenKind::Punct(Punct::Dot), start)
            }
            _ if is_ident_start(byte) => self.identifier(start),
            _ => Err(self.error(format!("unexpected character '{}'", byte as char))),
        }
    }

    fn starts_with_ignore_case(&self, needle: &[u8]) -> bool {
        self.input
            .get(self.pos..self.pos + needle.len())
            .is_some_and(|slice| slice.eq_ignore_ascii_case(needle))
    }

    fn punct(&mut self, kind: TokenKind, start: usize) -> Result<Token> {
        self.pos += 1;
        Ok(self.token(kind, start))
    }

    fn comparison(&mut self, kind: CompareKind, start: usize) -> Result<Token> {
        let case = match self.peek(0) {
            Some(b'#') => {
                self.pos += 1;
                CaseSense::Sensitive
            }
            Some(b'?') => {
                self.pos += 1;
                CaseSense::Insensitive
            }
            _ => CaseSense::Default,
        };
        Ok(self.token(TokenKind::Cmp(CmpOp { kind, case }), start))
    }

    fn identifier(&mut self, start: usize) -> Result<Token> {
        // Scope prefixes: g: b: w: t: l: a: s: v:
        if self.peek(1) == Some(b':') {
            if let Some(scope) = Scope::from_prefix(self.peek(0).unwrap_or(b' ')) {
                if !self.peek(2).is_some_and(is_ident_continue) {
                    // g: used as a whole-scope dictionary or an empty name.
                    self.pos += 2;
                    return Ok(self.token(TokenKind::ScopedVar(scope, VimString::new()), start));
                }
                self.pos += 2;
                let name_start = self.pos;
                while self.peek(0).is_some_and(is_ident_continue) {
                    self.pos += 1;
                }
                let name = VimString::from(&self.input[name_start..self.pos]);
                return Ok(self.token(TokenKind::ScopedVar(scope, name), start));
            }
        }
        let mut end = self.pos;
        while end < self.input.len() && is_ident_continue(self.input[end]) {
            end += 1;
        }
        let text = VimString::from(&self.input[self.pos..end]);
        self.pos = end;
        // '#" is a valid identifier character (autoload names), so "is#" and
        // "isnot#" arrive here as a single identifier.
        let keyword = match text.as_bytes() {
            b"is" | b"is#" | b"is?" => Some(Keyword::Is),
            b"isnot" | b"isnot#" | b"isnot?" => Some(Keyword::IsNot),
            b"in" => Some(Keyword::In),
            _ => None,
        };
        if let Some(keyword) = keyword {
            return Ok(self.token(TokenKind::Keyword(keyword), start));
        }
        Ok(self.token(TokenKind::Ident(text), start))
    }

    fn environment(&mut self, start: usize) -> Result<Token> {
        self.pos += 1;
        let name_start = self.pos;
        while self
            .peek(0)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.pos += 1;
        }
        if self.pos == name_start {
            return Err(self.error("empty environment variable name"));
        }
        let name = String::from_utf8_lossy(&self.input[name_start..self.pos]).into_owned();
        Ok(self.token(TokenKind::Env(name), start))
    }

    fn register(&mut self, start: usize) -> Result<Token> {
        self.pos += 1;
        let Some(name) = self.bump() else {
            return Err(self.error("missing register name"));
        };
        Ok(self.token(TokenKind::Register(name), start))
    }

    fn option(&mut self, start: usize) -> Result<Token> {
        self.pos += 1;
        let scope = if self.peek(1) == Some(b':') {
            let scope = match self.peek(0) {
                Some(b'g') => OptionScope::Global,
                Some(b'l') => OptionScope::Local,
                _ => return Err(self.error("invalid option scope")),
            };
            self.pos += 2;
            Some(scope)
        } else {
            None
        };
        let name_start = self.pos;
        while self
            .peek(0)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.pos += 1;
        }
        if self.pos == name_start {
            return Err(self.error("missing option name"));
        }
        let name = String::from_utf8_lossy(&self.input[name_start..self.pos]).into_owned();
        Ok(self.token(TokenKind::Option(scope, name), start))
    }

    fn number(&mut self, start: usize) -> Result<Token> {
        // Blob literal 0z...
        if self.peek(0) == Some(b'0') && matches!(self.peek(1), Some(b'z' | b'Z')) {
            self.pos += 2;
            let mut digits = Vec::new();
            while let Some(byte) = self.peek(0) {
                if byte.is_ascii_hexdigit() {
                    digits.push(byte);
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if digits.len() % 2 != 0 {
                return Err(VimError::new(
                    973,
                    "Blob literal should have an even number of hex characters",
                ));
            }
            let mut bytes = Vec::with_capacity(digits.len() / 2);
            for pair in digits.chunks(2) {
                let text =
                    std::str::from_utf8(pair).map_err(|_| self.error("invalid blob literal"))?;
                let byte =
                    u8::from_str_radix(text, 16).map_err(|_| self.error("invalid blob literal"))?;
                bytes.push(byte);
            }
            return Ok(self.token(TokenKind::Blob(bytes), start));
        }

        let mut radix = 10u32;
        let mut digits_start = self.pos;
        if self.peek(0) == Some(b'0') {
            match self.peek(1) {
                Some(b'x' | b'X') => {
                    radix = 16;
                    self.pos += 2;
                    digits_start = self.pos;
                }
                Some(b'b' | b'B') => {
                    radix = 2;
                    self.pos += 2;
                    digits_start = self.pos;
                }
                Some(b'o' | b'O') => {
                    radix = 8;
                    self.pos += 2;
                    digits_start = self.pos;
                }
                _ => {
                    if self.peek(1) != Some(b'.') {
                        // Leading zero: octal (Vim's classic notation).
                        radix = 8;
                        digits_start = self.pos;
                        self.pos += 1;
                    }
                }
            }
        }
        if radix == 10 {
            while self.peek(0).is_some_and(|byte| byte.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.peek(0) == Some(b'.') && self.peek(1).is_some_and(|byte| byte.is_ascii_digit())
            {
                self.pos += 1;
                while self.peek(0).is_some_and(|byte| byte.is_ascii_digit()) {
                    self.pos += 1;
                }
                if matches!(self.peek(0), Some(b'e' | b'E')) {
                    let save = self.pos;
                    self.pos += 1;
                    if matches!(self.peek(0), Some(b'+' | b'-')) {
                        self.pos += 1;
                    }
                    let exponent_start = self.pos;
                    while self.peek(0).is_some_and(|byte| byte.is_ascii_digit()) {
                        self.pos += 1;
                    }
                    if self.pos == exponent_start {
                        self.pos = save;
                    }
                }
                let text = std::str::from_utf8(&self.input[start..self.pos])
                    .map_err(|_| self.error("invalid float literal"))?;
                let value = text
                    .parse::<f64>()
                    .map_err(|_| self.error("invalid float literal"))?;
                return Ok(self.token(TokenKind::Float(value), start));
            }
            let text = std::str::from_utf8(&self.input[start..self.pos])
                .map_err(|_| self.error("invalid number literal"))?;
            let value = parse_wrapping_decimal(text);
            return Ok(self.token(TokenKind::Number(value), start));
        }

        let mut value: u64 = 0;
        let mut any = false;
        while let Some(byte) = self.peek(0) {
            let Some(digit) = (byte as char).to_digit(radix) else {
                break;
            };
            any = true;
            value = value
                .wrapping_mul(u64::from(radix))
                .wrapping_add(u64::from(digit));
            self.pos += 1;
        }
        if !any {
            // "0" alone (or a prefix with no digits) is an error for 0x/0b/0o.
            if radix != 8 || self.pos == digits_start {
                return Err(self.error("missing digits in number literal"));
            }
        }
        if radix == 8 && matches!(self.peek(0), Some(b'8' | b'9')) {
            return Err(self.error("invalid octal number"));
        }
        Ok(self.token(TokenKind::Number(value as i64), start))
    }

    fn string(&mut self, start: usize, quote: u8) -> Result<Token> {
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(byte) = self.peek(0) else {
                return Err(VimError::new(115, "Missing quote"));
            };
            if byte == quote {
                self.pos += 1;
                if quote == b'\'' && self.peek(0) == Some(b'\'') {
                    out.push(b'\'');
                    self.pos += 1;
                    continue;
                }
                break;
            }
            match byte {
                b'\n' if quote == b'"' => {
                    self.pos += 1;
                    self.line += 1;
                    out.push(b'\n');
                }
                b'\\' if quote == b'"' => {
                    self.pos += 1;
                    self.escape_sequence(&mut out)?;
                }
                _ => {
                    out.push(byte);
                    self.pos += 1;
                    if byte == b'\n' {
                        self.line += 1;
                    }
                }
            }
        }
        Ok(self.token(TokenKind::String(VimString::from(out)), start))
    }

    /// Lex `$"..."` / `$'...'` with `{expr}` interpolation.
    ///
    /// `{{` and `}}` are literal braces (a lone `}` is an error); in double
    /// quoted form backslash escapes work as usual, including `\{` and
    /// `\}`.
    fn interpolated_string(&mut self, start: usize, quote: u8) -> Result<Token> {
        self.pos += 2; // '$' and the quote
        let mut parts: Vec<InterpPart> = Vec::new();
        let mut text: Vec<u8> = Vec::new();
        loop {
            let Some(byte) = self.peek(0) else {
                return Err(VimError::new(115, "Missing quote"));
            };
            if byte == quote {
                self.pos += 1;
                if quote == b'\'' && self.peek(0) == Some(b'\'') {
                    text.push(b'\'');
                    self.pos += 1;
                    continue;
                }
                break;
            }
            match byte {
                b'{' if self.peek(1) == Some(b'{') => {
                    text.push(b'{');
                    self.pos += 2;
                }
                b'{' => {
                    if !text.is_empty() {
                        parts.push(InterpPart::Text(VimString::from(std::mem::take(&mut text))));
                    }
                    let expression = self.interpolation_expression()?;
                    parts.push(InterpPart::Expression(VimString::from(expression)));
                }
                b'}' if self.peek(1) == Some(b'}') => {
                    text.push(b'}');
                    self.pos += 2;
                }
                b'}' => {
                    return Err(VimError::new(1279, "Single '}' in interpolated string"));
                }
                b'\n' if quote == b'"' => {
                    self.pos += 1;
                    self.line += 1;
                    text.push(b'\n');
                }
                b'\\' if quote == b'"' => {
                    self.pos += 1;
                    self.escape_sequence(&mut text)?;
                }
                _ => {
                    text.push(byte);
                    self.pos += 1;
                    if byte == b'\n' {
                        self.line += 1;
                    }
                }
            }
        }
        if !text.is_empty() {
            parts.push(InterpPart::Text(VimString::from(text)));
        }
        Ok(self.token(TokenKind::InterpolatedString(parts), start))
    }

    /// Capture the raw `{expr}` body inside an interpolated string.
    fn interpolation_expression(&mut self) -> Result<Vec<u8>> {
        self.pos += 1; // '{'
        let start = self.pos;
        let mut depth = 1usize;
        while let Some(byte) = self.peek(0) {
            match byte {
                b'{' => {
                    depth += 1;
                    self.bump();
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let expression = self.input[start..self.pos].to_vec();
                        self.bump(); // '}'
                        return Ok(expression);
                    }
                    self.bump();
                }
                b'\'' => {
                    self.bump();
                    while let Some(next) = self.peek(0) {
                        self.bump();
                        if next == b'\'' {
                            if self.peek(0) == Some(b'\'') {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                    }
                }
                b'"' => {
                    self.bump();
                    while let Some(next) = self.peek(0) {
                        if next == b'\\' {
                            self.bump();
                            self.bump();
                            continue;
                        }
                        self.bump();
                        if next == b'"' {
                            break;
                        }
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
        Err(VimError::new(1278, "Missing '}' in interpolated string"))
    }

    fn escape_sequence(&mut self, out: &mut Vec<u8>) -> Result<()> {
        let Some(byte) = self.bump() else {
            return Err(self.error("unexpected end of string escape"));
        };
        match byte {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'e' => out.push(0x1b),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'a' => out.push(0x07),
            b'v' => out.push(0x0b),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'0'..=b'7' => {
                let mut value = u32::from(byte - b'0');
                for _ in 0..2 {
                    if let Some(next) = self.peek(0) {
                        if (b'0'..=b'7').contains(&next) {
                            value = value * 8 + u32::from(next - b'0');
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                }
                out.push(value as u8);
            }
            b'x' => {
                let mut value: u32 = 0;
                let mut count = 0;
                while count < 2 {
                    if let Some(next) = self.peek(0) {
                        if let Some(digit) = (next as char).to_digit(16) {
                            value = value * 16 + digit;
                            self.pos += 1;
                            count += 1;
                            continue;
                        }
                    }
                    break;
                }
                if count == 0 {
                    return Err(self.error("invalid \\x escape"));
                }
                out.push(value as u8);
            }
            b'u' | b'U' => {
                let count = if byte == b'u' { 4 } else { 8 };
                let mut value: u32 = 0;
                for _ in 0..count {
                    let Some(next) = self.peek(0) else {
                        return Err(self.error("incomplete unicode escape"));
                    };
                    let Some(digit) = (next as char).to_digit(16) else {
                        return Err(self.error("invalid unicode escape"));
                    };
                    value = value * 16 + digit;
                    self.pos += 1;
                }
                let Some(character) = char::from_u32(value) else {
                    return Err(self.error("invalid unicode code point"));
                };
                let mut buffer = [0u8; 4];
                out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
            b'<' => {
                let name_start = self.pos;
                while let Some(next) = self.peek(0) {
                    if next == b'>' {
                        break;
                    }
                    self.pos += 1;
                }
                if self.peek(0) != Some(b'>') {
                    return Err(self.error("missing '>' in key notation"));
                }
                let name = &self.input[name_start..self.pos];
                self.pos += 1;
                match special_key(name) {
                    Some(bytes) => out.extend_from_slice(&bytes),
                    // Vim drops the backslash of an unrecognized "\<name>"
                    // and copies the remaining characters, which is how syntax
                    // files write regex atoms such as "\<done\>" in
                    // double-quoted strings.
                    None => {
                        out.push(b'<');
                        let mut index = 0;
                        while index < name.len() {
                            if name[index] == b'\\' && index + 1 < name.len() {
                                index += 1;
                            }
                            out.push(name[index]);
                            index += 1;
                        }
                        out.push(b'>');
                    }
                }
            }
            b'\n' => {
                self.line += 1;
            }
            other => out.push(other),
        }
        Ok(())
    }
}

fn special_key(name: &[u8]) -> Option<Vec<u8>> {
    let upper = name.to_ascii_uppercase();
    if upper.len() >= 3 && upper.starts_with(b"C-") {
        let key = upper[2];
        let byte = match key {
            b'?' => 0x7f,
            b'@' => 0,
            b'[' => 0x1b,
            b'\\' => 0x1c,
            b']' => 0x1d,
            b'^' => 0x1e,
            b'_' => 0x1f,
            b'A'..=b'Z' => key & 0x1f,
            _ => return None,
        };
        return Some(vec![byte]);
    }
    match upper.as_slice() {
        b"CR" => Some(vec![13]),
        b"NL" | b"LF" => Some(vec![10]),
        b"TAB" => Some(vec![9]),
        b"ESC" => Some(vec![27]),
        b"SPACE" => Some(vec![32]),
        b"BS" => Some(vec![8]),
        b"DEL" => Some(vec![127]),
        b"NUL" => Some(vec![0]),
        _ => None,
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'#'
}

fn parse_wrapping_decimal(text: &str) -> i64 {
    let mut value: u64 = 0;
    for byte in text.bytes() {
        if let Some(digit) = (byte as char).to_digit(10) {
            value = value.wrapping_mul(10).wrapping_add(u64::from(digit));
        }
    }
    value as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input.as_bytes())
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn numbers_and_floats() {
        assert_eq!(kinds("42"), vec![TokenKind::Number(42), TokenKind::Eof]);
        assert_eq!(kinds("0x2a"), vec![TokenKind::Number(42), TokenKind::Eof]);
        assert_eq!(
            kinds("0b101010"),
            vec![TokenKind::Number(42), TokenKind::Eof]
        );
        assert_eq!(kinds("0o52"), vec![TokenKind::Number(42), TokenKind::Eof]);
        assert_eq!(kinds("052"), vec![TokenKind::Number(42), TokenKind::Eof]);
        assert_eq!(kinds("1.5"), vec![TokenKind::Float(1.5), TokenKind::Eof]);
        assert_eq!(
            kinds("1.5e2"),
            vec![TokenKind::Float(150.0), TokenKind::Eof]
        );
        assert_eq!(
            kinds("0zDEAD"),
            vec![TokenKind::Blob(vec![0xde, 0xad]), TokenKind::Eof]
        );
    }

    #[test]
    fn strings_and_scopes() {
        assert_eq!(
            kinds(r#""a\n""#),
            vec![TokenKind::String(VimString::from("a\n")), TokenKind::Eof]
        );
        assert_eq!(
            kinds("'it''s'"),
            vec![TokenKind::String(VimString::from("it's")), TokenKind::Eof]
        );
        assert_eq!(
            kinds("s:name"),
            vec![
                TokenKind::ScopedVar(Scope::Script, VimString::from("name")),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("v:true"),
            vec![
                TokenKind::ScopedVar(Scope::Vim, VimString::from("true")),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("&l:shiftwidth"),
            vec![
                TokenKind::Option(Some(OptionScope::Local), "shiftwidth".to_string()),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("$HOME"),
            vec![TokenKind::Env("HOME".to_string()), TokenKind::Eof]
        );
        assert_eq!(kinds("@a"), vec![TokenKind::Register(b'a'), TokenKind::Eof]);
    }

    #[test]
    fn comparison_suffixes() {
        assert_eq!(
            kinds("==#"),
            vec![
                TokenKind::Cmp(CmpOp {
                    kind: CompareKind::Equal,
                    case: CaseSense::Sensitive
                }),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("!~?"),
            vec![
                TokenKind::Cmp(CmpOp {
                    kind: CompareKind::NotMatch,
                    case: CaseSense::Insensitive
                }),
                TokenKind::Eof
            ]
        );
    }
}
