//! Recursive-descent expression parser for legacy Vim script.
//!
//! Precedence follows Vim: ternary, `||`, `&&`, comparisons, `+ - . ..`,
//! `* / %`, unary operators, then subscripts/calls/member access.

use std::rc::Rc;

use rvim_types::VimString;

use crate::error::{codes, Result};
use crate::lexer::{
    tokenize, AssignOp, CmpOp, Keyword, OptionScope, Punct, Scope, Token, TokenKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
    Plus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Concat,
    Compare(CmpOp),
    Is,
    IsNot,
    And,
    Or,
}

/// An anonymous function body captured by a `{args -> expr}` lambda.
#[derive(Clone, Debug, PartialEq)]
pub struct LambdaBody {
    pub params: Vec<VimString>,
    pub varargs: bool,
    pub body: Expr,
    pub line: u32,
    pub source: VimString,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number(i64),
    Float(f64),
    String(VimString),
    Blob(Vec<u8>),
    List(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Var {
        scope: Option<Scope>,
        name: VimString,
    },
    Option {
        scope: Option<OptionScope>,
        name: String,
    },
    Env(String),
    Register(u8),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        base: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Member {
        base: Box<Expr>,
        key: VimString,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    MethodCall {
        receiver: Box<Expr>,
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Ternary {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    Lambda(Rc<LambdaBody>),
}

impl Expr {
    /// A bare variable name, when the expression is one.
    pub fn simple_name(&self) -> Option<&VimString> {
        match self {
            Expr::Var { scope: None, name } => Some(name),
            _ => None,
        }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    index: usize,
    source: Vec<u8>,
}

impl Parser {
    pub fn new(input: &[u8]) -> Result<Self> {
        Ok(Self {
            tokens: tokenize(input)?,
            index: 0,
            source: input.to_vec(),
        })
    }

    pub fn parse_complete(input: &[u8]) -> Result<Expr> {
        let mut parser = Self::new(input)?;
        let expression = parser.parse_expression()?;
        parser.expect_eof()?;
        Ok(expression)
    }

    pub fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len().saturating_sub(1))]
    }

    pub fn current_kind(&self) -> &TokenKind {
        &self.current().kind
    }

    /// Byte offset of the next unconsumed token in the parser input.
    pub fn current_pos(&self) -> usize {
        self.current().pos
    }

    pub fn at_eof(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Eof)
    }

    pub fn expect_eof(&self) -> Result<()> {
        if self.at_eof() {
            Ok(())
        } else {
            Err(codes::e488(&format!(
                "unexpected {:?}",
                self.current_kind()
            )))
        }
    }

    pub fn bump(&mut self) -> Token {
        let token = self.current().clone();
        if !self.at_eof() {
            self.index += 1;
        }
        token
    }

    pub fn check_punct(&self, punct: Punct) -> bool {
        matches!(self.current_kind(), TokenKind::Punct(found) if *found == punct)
    }

    pub fn eat_punct(&mut self, punct: Punct) -> bool {
        if self.check_punct(punct) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub fn expect_punct(&mut self, punct: Punct) -> Result<()> {
        if self.eat_punct(punct) {
            Ok(())
        } else {
            Err(codes::e15(&format!(
                "expected {:?}, found {:?}",
                punct,
                self.current_kind()
            )))
        }
    }

    pub fn eat_keyword(&mut self, keyword: Keyword) -> bool {
        if matches!(self.current_kind(), TokenKind::Keyword(found) if *found == keyword) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume an assignment operator, if the next token is one.
    pub fn take_assignment_op(&mut self) -> Option<AssignOp> {
        let op = match self.current_kind() {
            TokenKind::Punct(Punct::Assign) => AssignOp::Assign,
            TokenKind::Punct(Punct::PlusAssign) => AssignOp::Add,
            TokenKind::Punct(Punct::MinusAssign) => AssignOp::Sub,
            TokenKind::Punct(Punct::StarAssign) => AssignOp::Mul,
            TokenKind::Punct(Punct::SlashAssign) => AssignOp::Div,
            TokenKind::Punct(Punct::PercentAssign) => AssignOp::Rem,
            TokenKind::Punct(Punct::DotAssign | Punct::DotDotAssign) => AssignOp::Concat,
            _ => return None,
        };
        self.bump();
        Some(op)
    }

    pub fn parse_expression(&mut self) -> Result<Expr> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr> {
        let condition = self.parse_or()?;
        if self.eat_punct(Punct::Question) {
            let then_value = self.parse_expression()?;
            self.expect_punct(Punct::Colon)?;
            let else_value = self.parse_ternary()?;
            return Ok(Expr::Ternary {
                condition: Box::new(condition),
                then_value: Box::new(then_value),
                else_value: Box::new(else_value),
            });
        }
        Ok(condition)
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while self.eat_punct(Punct::OrOr) {
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_compare()?;
        while self.eat_punct(Punct::AndAnd) {
            let right = self.parse_compare()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.current_kind() {
                TokenKind::Cmp(compare) => {
                    let compare = *compare;
                    self.bump();
                    BinaryOp::Compare(compare)
                }
                TokenKind::Keyword(Keyword::Is) => {
                    self.bump();
                    BinaryOp::Is
                }
                TokenKind::Keyword(Keyword::IsNot) => {
                    self.bump();
                    BinaryOp::IsNot
                }
                _ => break,
            };
            let right = self.parse_additive()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.current_kind() {
                TokenKind::Punct(Punct::Plus) => BinaryOp::Add,
                TokenKind::Punct(Punct::Minus) => BinaryOp::Sub,
                TokenKind::Punct(Punct::Dot | Punct::DotDot) => BinaryOp::Concat,
                _ => break,
            };
            self.bump();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.current_kind() {
                TokenKind::Punct(Punct::Star) => BinaryOp::Mul,
                TokenKind::Punct(Punct::Slash) => BinaryOp::Div,
                TokenKind::Punct(Punct::Percent) => BinaryOp::Rem,
                _ => break,
            };
            self.bump();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        let op = match self.current_kind() {
            TokenKind::Punct(Punct::Bang) => UnaryOp::Not,
            TokenKind::Punct(Punct::Minus) => UnaryOp::Negate,
            TokenKind::Punct(Punct::Plus) => UnaryOp::Plus,
            _ => return self.parse_postfix(),
        };
        self.bump();
        let operand = self.parse_unary()?;
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
        })
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.eat_punct(Punct::LParen) {
                let args = self.parse_arguments()?;
                expression = Expr::Call {
                    callee: Box::new(expression),
                    args,
                };
                continue;
            }
            if self.eat_punct(Punct::LBracket) {
                expression = self.parse_subscript(expression)?;
                continue;
            }
            let member_key = self.is_member_dot();
            if member_key {
                self.bump();
                let key = match self.current_kind().clone() {
                    TokenKind::Ident(name) => {
                        self.bump();
                        name
                    }
                    _ => return Err(codes::e15("expected dictionary key after '.'")),
                };
                expression = Expr::Member {
                    base: Box::new(expression),
                    key,
                };
                continue;
            }
            if self.eat_punct(Punct::Arrow) {
                let callee = self.parse_primary()?;
                self.expect_punct(Punct::LParen)?;
                let args = self.parse_arguments()?;
                expression = Expr::MethodCall {
                    receiver: Box::new(expression),
                    callee: Box::new(callee),
                    args,
                };
                continue;
            }
            break;
        }
        Ok(expression)
    }

    fn is_member_dot(&self) -> bool {
        if !matches!(self.current_kind(), TokenKind::Punct(Punct::Dot)) {
            return false;
        }
        if !matches!(
            self.tokens.get(self.index + 1).map(|token| &token.kind),
            Some(TokenKind::Ident(_))
        ) {
            return false;
        }
        let previous = self
            .index
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index));
        let Some(previous) = previous else {
            return false;
        };
        let allowed = matches!(
            previous.kind,
            TokenKind::Ident(_)
                | TokenKind::ScopedVar(_, _)
                | TokenKind::Punct(Punct::RBracket | Punct::RParen | Punct::RBrace)
        );
        if !allowed {
            return false;
        }
        let position = self.current().pos;
        position > 0
            && !matches!(
                self.source.get(position - 1).copied(),
                Some(b' ' | b'\t' | b'\r' | b'\n')
            )
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expr>> {
        let mut arguments = Vec::new();
        if self.eat_punct(Punct::RParen) {
            return Ok(arguments);
        }
        loop {
            arguments.push(self.parse_expression()?);
            if self.eat_punct(Punct::Comma) {
                if self.eat_punct(Punct::RParen) {
                    break;
                }
                continue;
            }
            self.expect_punct(Punct::RParen)?;
            break;
        }
        Ok(arguments)
    }

    fn parse_subscript(&mut self, base: Expr) -> Result<Expr> {
        if self.eat_punct(Punct::Colon) {
            let end = if self.check_punct(Punct::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expression()?))
            };
            self.expect_punct(Punct::RBracket)?;
            return Ok(Expr::Slice {
                base: Box::new(base),
                start: None,
                end,
            });
        }
        let first = self.parse_expression()?;
        if self.eat_punct(Punct::Colon) {
            let end = if self.check_punct(Punct::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expression()?))
            };
            self.expect_punct(Punct::RBracket)?;
            return Ok(Expr::Slice {
                base: Box::new(base),
                start: Some(Box::new(first)),
                end,
            });
        }
        self.expect_punct(Punct::RBracket)?;
        Ok(Expr::Index {
            base: Box::new(base),
            index: Box::new(first),
        })
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(number) => {
                self.bump();
                Ok(Expr::Number(number))
            }
            TokenKind::Float(float) => {
                self.bump();
                Ok(Expr::Float(float))
            }
            TokenKind::String(string) => {
                self.bump();
                Ok(Expr::String(string))
            }
            TokenKind::Blob(bytes) => {
                self.bump();
                Ok(Expr::Blob(bytes))
            }
            TokenKind::Ident(name) => {
                self.bump();
                Ok(Expr::Var { scope: None, name })
            }
            TokenKind::ScopedVar(scope, name) => {
                self.bump();
                Ok(Expr::Var {
                    scope: Some(scope),
                    name,
                })
            }
            TokenKind::Option(scope, name) => {
                self.bump();
                Ok(Expr::Option { scope, name })
            }
            TokenKind::Env(name) => {
                self.bump();
                Ok(Expr::Env(name))
            }
            TokenKind::Register(name) => {
                self.bump();
                Ok(Expr::Register(name))
            }
            TokenKind::Punct(Punct::LParen) => {
                self.bump();
                let expression = self.parse_expression()?;
                self.expect_punct(Punct::RParen)?;
                Ok(expression)
            }
            TokenKind::Punct(Punct::LBracket) => {
                self.bump();
                self.parse_list_literal()
            }
            TokenKind::Punct(Punct::LBrace) => self.parse_brace(),
            _ => Err(codes::e15(&format!(
                "unexpected {:?} at line {}",
                token.kind, token.line
            ))),
        }
    }

    fn parse_list_literal(&mut self) -> Result<Expr> {
        let mut items = Vec::new();
        if self.eat_punct(Punct::RBracket) {
            return Ok(Expr::List(items));
        }
        loop {
            items.push(self.parse_expression()?);
            if self.eat_punct(Punct::Comma) {
                if self.eat_punct(Punct::RBracket) {
                    break;
                }
                continue;
            }
            self.expect_punct(Punct::RBracket)?;
            break;
        }
        Ok(Expr::List(items))
    }

    fn parse_brace(&mut self) -> Result<Expr> {
        let start_index = self.index;
        let line = self.current().line;
        let start_pos = self.current().pos;
        self.bump(); // '{'
        if self.looks_like_lambda() {
            let mut params = Vec::new();
            let mut varargs = false;
            if !self.check_punct(Punct::Arrow) {
                loop {
                    if self.eat_punct(Punct::Ellipsis) {
                        varargs = true;
                        break;
                    }
                    match self.current_kind().clone() {
                        TokenKind::Ident(name) => {
                            self.bump();
                            params.push(name);
                        }
                        other => {
                            return Err(codes::e15(&format!(
                                "invalid lambda parameter: {other:?}"
                            )));
                        }
                    }
                    if self.eat_punct(Punct::Comma) {
                        continue;
                    }
                    break;
                }
            }
            self.expect_punct(Punct::Arrow)?;
            let body = self.parse_expression()?;
            self.expect_punct(Punct::RBrace)?;
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
            })));
        }
        self.index = start_index + 1;
        self.parse_dict_literal()
    }

    fn looks_like_lambda(&self) -> bool {
        if self.check_punct(Punct::Arrow) {
            return true;
        }
        let mut index = self.index;
        while let Some(TokenKind::Ident(_)) | Some(TokenKind::Punct(Punct::Ellipsis)) =
            self.tokens.get(index).map(|token| &token.kind)
        {
            index += 1;
            match self.tokens.get(index).map(|token| &token.kind) {
                Some(TokenKind::Punct(Punct::Comma)) => index += 1,
                Some(TokenKind::Punct(Punct::Arrow)) => return true,
                _ => return false,
            }
        }
        false
    }

    fn parse_dict_literal(&mut self) -> Result<Expr> {
        let mut entries = Vec::new();
        if self.eat_punct(Punct::RBrace) {
            return Ok(Expr::Dict(entries));
        }
        loop {
            let (key, colon_consumed) = match self.current_kind().clone() {
                TokenKind::String(string) => {
                    self.bump();
                    (Expr::String(string), false)
                }
                TokenKind::Number(number) => {
                    self.bump();
                    (Expr::Number(number), false)
                }
                TokenKind::Ident(name) => {
                    self.bump();
                    (Expr::String(name), false)
                }
                TokenKind::ScopedVar(scope, name) if name.is_empty() => {
                    self.bump();
                    (
                        Expr::String(VimString::from(
                            scope.prefix().trim_end_matches(':').to_string(),
                        )),
                        true,
                    )
                }
                other => {
                    return Err(codes::e15(&format!("invalid dictionary key: {other:?}")));
                }
            };
            if !colon_consumed {
                self.expect_punct(Punct::Colon)?;
            }
            let value = self.parse_expression()?;
            entries.push((key, value));
            if self.eat_punct(Punct::Comma) {
                if self.eat_punct(Punct::RBrace) {
                    break;
                }
                continue;
            }
            self.expect_punct(Punct::RBrace)?;
            break;
        }
        Ok(Expr::Dict(entries))
    }
}

/// Parse a complete expression, rejecting trailing tokens.
pub fn parse_expression(input: &[u8]) -> Result<Expr> {
    Parser::parse_complete(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Expr {
        parse_expression(input.as_bytes()).unwrap()
    }

    #[test]
    fn precedence_and_operators() {
        let expression = parse("1 + 2 * 3");
        assert!(matches!(
            expression,
            Expr::Binary {
                op: BinaryOp::Add,
                left: _,
                right: _
            }
        ));
        let expression = parse("1 . 2 .. 'x'");
        assert!(matches!(
            expression,
            Expr::Binary {
                op: BinaryOp::Concat,
                ..
            }
        ));
        assert!(matches!(parse("a ? b : c"), Expr::Ternary { .. }));
    }

    #[test]
    fn collections_and_slices() {
        assert!(matches!(parse("[1, 2, 3]"), Expr::List(items) if items.len() == 3));
        assert!(matches!(parse("{'a': 1, b: 2}"), Expr::Dict(entries) if entries.len() == 2));
        assert!(matches!(parse("l[0]"), Expr::Index { .. }));
        assert!(matches!(
            parse("l[1:2]"),
            Expr::Slice {
                start: Some(_),
                end: Some(_),
                ..
            }
        ));
        assert!(matches!(
            parse("l[:]"),
            Expr::Slice {
                start: None,
                end: None,
                ..
            }
        ));
        assert!(matches!(parse("0zDEAD"), Expr::Blob(bytes) if bytes == vec![0xde, 0xad]));
    }

    #[test]
    fn lambdas_calls_and_methods() {
        assert!(matches!(parse("{x -> x + 1}"), Expr::Lambda(_)));
        assert!(matches!(parse("{-> 1}"), Expr::Lambda(_)));
        assert!(matches!(parse("{x, y -> x + y}"), Expr::Lambda(_)));
        assert!(matches!(parse("f(1, 2)"), Expr::Call { .. }));
        assert!(matches!(
            parse("list->map('v:val')"),
            Expr::MethodCall { .. }
        ));
        assert!(matches!(parse("d.key[0]"), Expr::Index { .. }));
    }
}
