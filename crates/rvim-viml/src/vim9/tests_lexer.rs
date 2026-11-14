//! Unit tests for the Vim9 lexer.

use super::lexer::tokenize;
use super::token::{HeredocFlags, InterpPart, Punct, TokenKind};

fn kinds(input: &str) -> Vec<TokenKind> {
    tokenize(input.as_bytes())
        .unwrap()
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

fn token_text(input: &str) -> Vec<String> {
    tokenize(input.as_bytes())
        .unwrap()
        .into_iter()
        .map(|token| format!("{:?}", token.kind))
        .collect()
}

#[test]
fn basic_tokens_and_positions() {
    let tokens = tokenize(b"var x = 1\n").unwrap();
    let kinds: Vec<TokenKind> = tokens.iter().map(|token| token.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Ident("var".into()),
            TokenKind::Ident("x".into()),
            TokenKind::Punct(Punct::Assign),
            TokenKind::Number(1),
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
    assert_eq!(tokens[0].line, 1);
    assert_eq!(tokens[0].col, 1);
    assert!(!tokens[0].space_before);
    assert!(tokens[1].space_before);
    assert_eq!(tokens[1].start, 4);
    assert_eq!(tokens[5].line, 2);
}

#[test]
fn blank_lines_and_comments() {
    let kinds = kinds("# comment\n\n# another\nvar x = 1 # trailing\n");
    assert_eq!(kinds[0], TokenKind::Newline);
    assert_eq!(kinds[1], TokenKind::Newline);
    assert_eq!(kinds[2], TokenKind::Newline);
    assert!(matches!(kinds[3], TokenKind::Ident(_)));
    assert!(kinds.contains(&TokenKind::Number(1)));
    // Fold markers and doubled braces are comments.
    assert!(tokenize(b"#{{{\ndef F(): void\nenddef #}}}\n").is_ok());
}

#[test]
fn comment_adjacency_rules() {
    // No white space: "false#" is a name, not a comment.
    let _ = kinds("var x = false#\n");
    // A comment starting with #{ is E1170.
    assert!(tokenize(b"#{ bad\n").is_err());
    // Trailing comment after code with white space is fine.
    assert!(tokenize(b"var x = 1 # ok\n").is_ok());
}

#[test]
fn numbers() {
    assert_eq!(kinds("42")[0], TokenKind::Number(42));
    assert_eq!(kinds("0755")[0], TokenKind::Number(755));
    assert_eq!(kinds("0x2a")[0], TokenKind::Number(42));
    assert_eq!(kinds("0o755")[0], TokenKind::Number(493));
    assert_eq!(kinds("0b101")[0], TokenKind::Number(5));
    assert_eq!(kinds("1'000")[0], TokenKind::Number(1000));
    assert_eq!(
        kinds("0z0011.2233")[0],
        TokenKind::Blob(vec![0, 0x11, 0x22, 0x33])
    );
    assert_eq!(kinds("0z")[0], TokenKind::Blob(vec![]));
    assert_eq!(kinds(".5")[0], TokenKind::Float(0.5));
    assert_eq!(kinds("1.5e+2")[0], TokenKind::Float(150.0));
    assert!(tokenize(b"0x").is_err());
    assert!(tokenize(b"0b2").is_err());
    assert!(tokenize(b"0z1").is_err());
}

#[test]
fn strings_and_escapes() {
    assert_eq!(kinds("'abc'")[0], TokenKind::String("abc".into()));
    assert_eq!(kinds("'a''b'")[0], TokenKind::String("a'b".into()));
    assert_eq!(kinds("\"a\\tb\"")[0], TokenKind::String("a\tb".into()));
    assert_eq!(kinds("\"a\\x41\"")[0], TokenKind::String("aA".into()));
    // Special key notation is decoded like legacy VimL strings.
    assert_eq!(
        kinds("\"\\<C-W>\"")[0],
        TokenKind::String(vec![0x17].into())
    );
    // Unknown key names lose the backslash, keeping '<name>'.
    assert_eq!(kinds("\"\\<done>\"")[0], TokenKind::String("<done>".into()));
    assert!(tokenize(b"'unterminated\n").is_err());
    assert!(tokenize(b"\"unterminated\n").is_err());
}

#[test]
fn interpolated_strings() {
    match &kinds("$'a{1 + 2}b{{c}}d'")[0] {
        TokenKind::InterpolatedString(parts) => {
            assert_eq!(parts.len(), 3);
            assert!(matches!(parts[0], InterpPart::Text(_)));
            assert!(matches!(parts[1], InterpPart::Expr { .. }));
            let InterpPart::Expr { source, .. } = &parts[1] else {
                unreachable!()
            };
            // Note: the expression is everything between the braces.
            assert_eq!(source.as_bytes(), b"1 + 2");
            match &parts[2] {
                InterpPart::Text(text) => assert_eq!(text.as_bytes(), b"b{c}d"),
                other => panic!("unexpected {other:?}"),
            }
        }
        other => panic!("unexpected {other:?}"),
    }
    // Literal text with doubled braces around the expression.
    match &kinds("$'x{{y}}z'")[0] {
        TokenKind::InterpolatedString(parts) => match &parts[0] {
            InterpPart::Text(text) => assert_eq!(text.as_bytes(), b"x{y}z"),
            other => panic!("unexpected {other:?}"),
        },
        other => panic!("unexpected {other:?}"),
    }
    // Nested braces in a list and a quoted string inside the expression.
    match &kinds("$'{[1, {\"a\": 2}][0]}'")[0] {
        TokenKind::InterpolatedString(parts) => {
            let expression = parts.iter().find_map(|part| match part {
                InterpPart::Expr { source, .. } => Some(source),
                InterpPart::Text(_) => None,
            });
            let expression = expression.expect("expected expression part");
            assert!(expression.as_bytes().contains(&b'{'));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(tokenize(b"$'a}b'\n").is_err());
    assert!(tokenize(b"$'a{1}b'\n").is_ok());
}

#[test]
fn heredocs() {
    let tokens = kinds("var lines =<< trim END\n  a\n  b\nEND\nvar after = 1\n");
    assert_eq!(tokens[0], TokenKind::Ident("var".into()));
    assert_eq!(tokens[2], TokenKind::Punct(Punct::Assign));
    match &tokens[3] {
        TokenKind::Heredoc(heredoc) => {
            assert_eq!(
                heredoc.flags,
                HeredocFlags {
                    trim: true,
                    eval: false
                }
            );
            assert_eq!(heredoc.lines.len(), 2);
            assert_eq!(heredoc.lines[0].as_bytes(), b"  a");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(tokens[4], TokenKind::Newline);
    assert!(tokens
        .iter()
        .any(|kind| matches!(kind, TokenKind::Number(1))));
    // trim allows the terminator to use the assignment line's indentation.
    assert!(tokenize(b"def F(): list<string>\n  var lines: list<string> =<< trim END\n    a\n  END\n  return lines\nenddef\n").is_ok());
    // eval flag.
    match &kinds("var x =<< eval END\n1 + 2\nEND\n")[3] {
        TokenKind::Heredoc(heredoc) => assert!(heredoc.flags.eval && !heredoc.flags.trim),
        other => panic!("unexpected {other:?}"),
    }
    // Missing terminator is an error.
    assert!(tokenize(b"var x =<< trim END\n  a\n").is_err());
}

#[test]
fn scoped_variables_and_dict_literal_keys() {
    assert_eq!(
        kinds("g:foo")[0],
        TokenKind::ScopedVar(crate::lexer::Scope::Global, "foo".into())
    );
    assert_eq!(
        kinds("v:true")[0],
        TokenKind::ScopedVar(crate::lexer::Scope::Vim, "true".into())
    );
    // Dict literal: "g" is a literal key, not the g: scope.
    let dict = kinds("{g: 1}");
    assert_eq!(dict[0], TokenKind::Punct(Punct::LBrace));
    assert_eq!(dict[1], TokenKind::Ident("g".into()));
    assert_eq!(dict[2], TokenKind::Punct(Punct::Colon));
    // Whole-scope reference "g:" used as a dictionary.
    assert_eq!(
        kinds("get(g:, 'x', 0)")[2],
        TokenKind::ScopedVar(crate::lexer::Scope::Global, "".into())
    );
}

#[test]
fn options_env_and_registers() {
    assert_eq!(kinds("&tabstop")[0], TokenKind::Option("tabstop".into()));
    assert_eq!(
        kinds("&l:shiftwidth")[0],
        TokenKind::Option("l:shiftwidth".into())
    );
    assert_eq!(kinds("&g:foo")[0], TokenKind::Option("g:foo".into()));
    assert_eq!(kinds("$HOME")[0], TokenKind::Env("HOME".into()));
    assert_eq!(kinds("$1")[0], TokenKind::Env("1".into()));
    assert_eq!(kinds("@a")[0], TokenKind::Register(b'a'));
    assert_eq!(kinds("@\"")[0], TokenKind::Register(b'"'));
}

#[test]
fn operators_longest_match() {
    let cases = [
        ("..", Punct::DotDot),
        ("..=", Punct::DotDotAssign),
        ("...", Punct::Ellipsis),
        ("->", Punct::Arrow),
        ("=>", Punct::FatArrow),
        ("==", Punct::EqEq),
        ("==#", Punct::EqEq),
        ("==?", Punct::EqEq),
        ("!=", Punct::NotEq),
        (">=", Punct::Ge),
        ("<=", Punct::Le),
        ("=~", Punct::Match),
        ("!~", Punct::NotMatch),
        ("&&", Punct::AndAnd),
        ("||", Punct::OrOr),
        ("|", Punct::Bar),
        ("??", Punct::QuestionQuestion),
        ("+=", Punct::PlusAssign),
        ("-= ", Punct::MinusAssign),
        ("*=", Punct::StarAssign),
        ("/=", Punct::SlashAssign),
        ("%=", Punct::PercentAssign),
    ];
    for (input, expected) in cases {
        let kind = kinds(input).remove(0);
        assert_eq!(kind, TokenKind::Punct(expected), "input {input:?}");
    }
    assert!(matches!(&kinds("isnot")[0], TokenKind::Ident(name) if name.as_bytes() == b"isnot"));
}

#[test]
fn newlines_and_crlf() {
    let kinds = kinds("a\r\nb\r\n");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Newline,
            TokenKind::Ident("b".into()),
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn backslash_continuation_is_joined() {
    // Vim's script reader joins a following physical line that starts with
    // blanks and a backslash (used by Ex commands such as :syntax).
    let kinds = kinds("syntax region X\n      \\ start='a'\n");
    let newlines = kinds
        .iter()
        .filter(|kind| matches!(kind, TokenKind::Newline))
        .count();
    assert_eq!(newlines, 1);
    assert!(kinds
        .iter()
        .any(|kind| matches!(kind, TokenKind::Ident(name) if name.as_bytes() == b"start")));
}

#[test]
fn legacy_quote_comment_after_block_end() {
    // Runtime files contain "endif "}}}" and Vim accepts it.
    let kinds = kinds("if 1\nendif \"}}}\nvar after = 1\n");
    assert!(kinds
        .iter()
        .any(|kind| matches!(kind, TokenKind::Number(1))));
    // But not after enddef.
    assert!(tokenize(b"def F(): void\nenddef \"}}}\n").is_err());
}

#[test]
fn defer_and_scope_names_are_plain_identifiers() {
    assert_eq!(kinds("defer")[0], TokenKind::Ident("defer".into()));
    assert_eq!(
        kinds("g:MyFunc")[0],
        TokenKind::ScopedVar(crate::lexer::Scope::Global, "MyFunc".into())
    );
    assert_eq!(
        kinds("s:foo")[0],
        TokenKind::ScopedVar(crate::lexer::Scope::Script, "foo".into())
    );
}

#[test]
fn token_text_is_stable() {
    // A smoke test for the Debug representation used by the example binary.
    let text = token_text("echo 'x' ?? 1");
    assert!(text[0].contains("echo"));
    assert!(text.iter().any(|item| item.contains("QuestionQuestion")));
}
