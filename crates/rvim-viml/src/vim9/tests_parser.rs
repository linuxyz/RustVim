//! Unit tests for the Vim9 parser.

use super::ast::*;
use super::parser::parse_script;

fn parse(source: &str) -> Script {
    match parse_script(source.as_bytes()) {
        Ok(script) => script,
        Err(error) => panic!(
            "parse failed: {}\n--- source ---\n{}",
            error.error_text(),
            source
        ),
    }
}

fn parse_error(source: &str) -> String {
    match parse_script(source.as_bytes()) {
        Ok(script) => panic!("expected a parse error, got {:#?}", script),
        Err(error) => error.error_text(),
    }
}

fn def_of(stmt: &Stmt) -> &DefDecl {
    match stmt {
        Stmt::Def(def) => def,
        other => panic!("expected def, got {other:?}"),
    }
}

#[test]
fn header_and_comments() {
    let script = parse("# leading comment\nvim9script\n# another\nvar x = 1\n");
    assert!(script.vim9_header);
    assert_eq!(script.items.len(), 1);
    let script = parse("vim9script noclear\nvar x = 1\n");
    assert!(script.vim9_header);
    assert_eq!(script.items.len(), 1);
    let script = parse("var x = 1\n");
    assert!(!script.vim9_header);
}

#[test]
fn var_declarations() {
    let script =
        parse("var a = 1\nvar b: number\nvar c: list<string> = []\nvar d: dict<any> = {}\n");
    assert_eq!(script.items.len(), 4);
    match &script.items[1] {
        Stmt::Var(decl) => {
            assert_eq!(decl.kind, VarKind::Var);
            assert!(decl.ty.is_some());
            assert!(decl.init.is_none());
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn const_final_and_destructuring() {
    let script = parse(
        "const MAX: number = 10\nfinal list = [1, 2]\nvar [a, b] = [1, 2]\nvar [x, y; rest] = [1, 2, 3]\nvar [s: string, n: number] = ['a', 1]\n",
    );
    assert_eq!(script.items.len(), 5);
    match &script.items[2] {
        Stmt::Var(decl) => match &decl.pattern {
            VarPattern::List { items, rest } => {
                assert_eq!(items.len(), 2);
                assert!(rest.is_none());
            }
            other => panic!("unexpected pattern {other:?}"),
        },
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[3] {
        Stmt::Var(decl) => match &decl.pattern {
            VarPattern::List { items, rest } => {
                assert_eq!(items.len(), 2);
                assert!(rest.is_some());
            }
            other => panic!("unexpected pattern {other:?}"),
        },
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn expression_precedence() {
    let script = parse("var x = 1 + 2 * 3\nvar y = 'a' .. 'b' .. 'c'\nvar z = true ? 1 : 2\n");
    match &script.items[0] {
        Stmt::Var(VarDecl {
            init: Some(expr), ..
        }) => match expr {
            Expr::Binary {
                op: BinaryOp::Add,
                right,
                ..
            } => assert!(matches!(
                right.as_ref(),
                Expr::Binary {
                    op: BinaryOp::Mul,
                    ..
                }
            )),
            other => panic!("unexpected expression {other:?}"),
        },
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[1] {
        Stmt::Var(VarDecl {
            init: Some(expr), ..
        }) => {
            assert!(matches!(
                expr,
                Expr::Binary {
                    op: BinaryOp::Concat,
                    ..
                }
            ))
        }
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[2] {
        Stmt::Var(VarDecl {
            init: Some(expr), ..
        }) => {
            assert!(matches!(expr, Expr::Ternary { .. }))
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn method_chain_and_calls() {
    let script = parse("var x = 'a'->split(',')->len()\nvar y = get(g:, 'name', 0)\n");
    match &script.items[0] {
        Stmt::Var(VarDecl {
            init: Some(expr), ..
        }) => {
            assert!(matches!(expr, Expr::Call { .. } | Expr::MethodCall { .. }));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn lambda_forms() {
    let script = parse(
        "var a = (x) => x + 1\nvar b = (x: number): number => x + 1\nvar c = () => 0\nvar d = [1, 2]->map((_, v) => v * 2)\n",
    );
    assert_eq!(script.items.len(), 4);
    match &script.items[1] {
        Stmt::Var(VarDecl {
            init: Some(expr), ..
        }) => match expr {
            Expr::Lambda(lambda) => {
                assert_eq!(lambda.params.len(), 1);
                assert!(lambda.ret.is_some());
            }
            other => panic!("unexpected {other:?}"),
        },
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn list_dict_tuple_literals() {
    let script = parse(
        "var l = [1, 2, 3]\nvar d = {key: 1, 'quoted': 2, [1 + 1]: 3}\nvar t = (1, 'a')\nvar t1 = (7,)\n",
    );
    assert_eq!(script.items.len(), 4);
    match &script.items[1] {
        Stmt::Var(VarDecl {
            init: Some(expr), ..
        }) => match expr {
            Expr::Dict(entries) => {
                assert_eq!(entries.len(), 3);
                assert!(matches!(entries[0].0, DictKey::Literal(_)));
                assert!(matches!(entries[1].0, DictKey::String(_)));
                assert!(matches!(entries[2].0, DictKey::Expr(_)));
            }
            other => panic!("unexpected {other:?}"),
        },
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(
        &script.items[2],
        Stmt::Var(VarDecl { init: Some(Expr::Tuple(items)), .. }) if items.len() == 2
    ));
    assert!(matches!(
        &script.items[3],
        Stmt::Var(VarDecl { init: Some(Expr::Tuple(items)), .. }) if items.len() == 1
    ));
}

#[test]
fn subscripts_and_slices() {
    let script =
        parse("var a = l[0]\nvar b = l[1:2]\nvar c = l[:]\nvar d = l[2:]\nvar e = l[:1]\n");
    assert_eq!(script.items.len(), 5);
    assert!(matches!(
        &script.items[0],
        Stmt::Var(VarDecl {
            init: Some(Expr::Index { .. }),
            ..
        })
    ));
    assert!(matches!(
        &script.items[2],
        Stmt::Var(VarDecl {
            init: Some(Expr::Slice {
                start: None,
                end: None,
                ..
            }),
            ..
        })
    ));
}

#[test]
fn interpolated_strings_and_heredocs() {
    let script = parse("var s = $\"a{1 + 2}b{c}d\"\n");
    match &script.items[0] {
        Stmt::Var(VarDecl {
            init: Some(Expr::Interpolated(segments)),
            ..
        }) => {
            assert_eq!(segments.len(), 5);
        }
        other => panic!("unexpected {other:?}"),
    }
    let script = parse("const text: list<string> =<< trim END\n  a\n  b\nEND\nvar after = 1\n");
    assert_eq!(script.items.len(), 2);
    match &script.items[0] {
        Stmt::Var(VarDecl {
            init: Some(Expr::Heredoc { lines, flags }),
            ..
        }) => {
            assert_eq!(lines.len(), 2);
            assert!(flags.trim);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn if_elseif_else() {
    let script =
        parse("if x == 1\n  var a = 1\nelseif x == 2\n  var b = 2\nelse\n  var c = 3\nendif\n");
    match &script.items[0] {
        Stmt::If(stmt) => {
            assert_eq!(stmt.branches.len(), 2);
            assert!(stmt.else_body.is_some());
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn while_and_for() {
    let script = parse(
        "while i < 10\n  i += 1\nendwhile\nfor i in range(3)\n  echo i\nendfor\nfor [k, v] in items(d)\n  echo k\nendfor\n",
    );
    assert_eq!(script.items.len(), 3);
    match &script.items[1] {
        Stmt::For(stmt) => assert!(matches!(stmt.pattern, VarPattern::Name(_))),
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[2] {
        Stmt::For(stmt) => assert!(matches!(stmt.pattern, VarPattern::List { .. })),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn try_catch_finally() {
    let script = parse(
        "try\n  throw 'x'\ncatch /^x$/\n  echo v:exception\ncatch\n  echo 'other'\nfinally\n  echo 'done'\nendtry\n",
    );
    match &script.items[0] {
        Stmt::Try(stmt) => {
            assert_eq!(stmt.catches.len(), 2);
            assert!(stmt.catches[0].pattern.is_some());
            assert!(stmt.finally_body.is_some());
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn def_params_and_body() {
    let script = parse(
        "def Map<T>(value: T, count: number = 1, ...rest: list<any>): T\n  return value\nenddef\n",
    );
    let def = def_of(&script.items[0]);
    assert_eq!(def.name.as_bytes(), b"Map");
    assert_eq!(def.generics.len(), 1);
    assert_eq!(def.params.len(), 3);
    assert!(def.params[1].default.is_some());
    assert!(def.params[2].varargs);
    assert!(def.ret.is_some());
}

#[test]
fn nested_and_conditional_defs() {
    let script = parse(
        "if has('gui_running')\n  def Foo(): string\n    return 'gui'\n  enddef\nelse\n  def Foo(): string\n    return 'terminal'\n  enddef\nendif\ndef Outer(): number\n  var x = 1\n  def Inner(): number\n    return x + 1\n  enddef\n  return Inner()\nenddef\n",
    );
    assert_eq!(script.items.len(), 2);
    match &script.items[0] {
        Stmt::If(stmt) => {
            assert!(matches!(stmt.branches[0].body[0], Stmt::Def(_)));
            assert!(matches!(stmt.else_body.as_ref().unwrap()[0], Stmt::Def(_)));
        }
        other => panic!("unexpected {other:?}"),
    }
    let outer = def_of(&script.items[1]);
    assert!(outer.body.iter().any(|stmt| matches!(stmt, Stmt::Def(_))));
}

#[test]
fn class_members() {
    let script = parse(
        "class ProgressBar\n  public var title: string\n  public var max_value: number\n  var private_field: number\n  static var count: number = 0\n  def new(title: string, max_value: number)\n    this.title = title .. ' '\n    this.max_value = max_value\n  enddef\n  def Paint(): number\n    def CalculateTicks(len: number)\n      return len\n    enddef\n    return CalculateTicks(3)\n  enddef\nendclass\n",
    );
    match &script.items[0] {
        Stmt::Class(class) => {
            assert_eq!(class.name.as_bytes(), b"ProgressBar");
            assert_eq!(class.members.len(), 6);
            assert!(matches!(
                class.members[0],
                ClassMember::Var {
                    access: Access::Public,
                    ..
                }
            ));
            assert!(matches!(
                class.members[2],
                ClassMember::Var {
                    access: Access::Private,
                    ..
                }
            ));
            assert!(matches!(
                class.members[3],
                ClassMember::Var {
                    is_static: true,
                    ..
                }
            ));
            match &class.members[4] {
                ClassMember::Def { def, .. } => assert_eq!(def.name.as_bytes(), b"new"),
                other => panic!("unexpected {other:?}"),
            }
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn interface_without_enddef() {
    let script =
        parse("interface Animal\n  var name: string\n  def Speak(): string\nendinterface\n");
    match &script.items[0] {
        Stmt::Interface(interface) => {
            assert_eq!(interface.members.len(), 2);
            assert!(matches!(
                interface.members[1],
                ClassMember::Def {
                    def: DefDecl {
                        is_abstract: true,
                        ..
                    },
                    ..
                }
            ));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn enum_and_type_alias() {
    let script =
        parse("enum Color\n  Red,\n  Green,\n  Blue,\nendenum\ntype MyList = list<number>\n");
    match &script.items[0] {
        Stmt::Enum(value) => assert_eq!(value.members.len(), 3),
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[1] {
        Stmt::TypeAlias(value) => {
            assert!(matches!(value.ty, TypeExpr::List(_)));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn imports_and_exports() {
    let script = parse(
        "import 'dist/vimhelp.vim'\nimport autoload '../autoload/context.vim'\nimport autoload 'comment.vim' as comment\nexport def Expr(lnum = v:lnum): number\n  return lnum\nenddef\nexport var exported = 1\nexport const MAX = 2\n",
    );
    assert_eq!(script.items.len(), 6);
    match &script.items[1] {
        Stmt::Import(value) => assert!(value.autoload),
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[2] {
        Stmt::Import(value) => {
            assert!(value.autoload);
            assert!(value.alias.is_some());
        }
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[3] {
        Stmt::Export(value) => assert!(matches!(value.item.as_ref(), Stmt::Def(_))),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn expressions_in_class_and_function_context() {
    let script = parse(
        "def F(): any\n  var d = {\n    'a': [1, 2],\n    'b': {\n      'c': 3,\n    },\n  }\n  return d\nenddef\n",
    );
    let def = def_of(&script.items[0]);
    assert_eq!(def.body.len(), 2);
}

#[test]
fn ex_commands_are_captured_raw() {
    let script = parse(
        "set laststatus=2\nsyntax region IncSearch oneline start='Ex '\nexecute 'hi' .. 'gh'\ndef F(): void\n  setlocal nowrap\n  echomsg 'hello world'\n  normal! gg\nenddef\n",
    );
    assert_eq!(script.items.len(), 4);
    assert!(matches!(
        &script.items[0],
        Stmt::Command { text, .. } if text.as_bytes() == b"set laststatus=2"
    ));
    let def = def_of(&script.items[3]);
    assert_eq!(def.body.len(), 3);
    assert!(def
        .body
        .iter()
        .all(|stmt| matches!(stmt, Stmt::Command { .. })));
}

#[test]
fn command_continuations_are_merged() {
    let script = parse(
        "command! ShowFileLine if !empty(bufname())\n      | echo 'x'\n      | endif\nsyntax region X\n      \\ start='a'\n",
    );
    assert_eq!(script.items.len(), 2);
    match &script.items[0] {
        Stmt::Command { text, .. } => {
            let text = String::from_utf8_lossy(text.as_bytes());
            assert!(text.contains("ShowFileLine"));
            assert!(text.contains("echo 'x'"));
            assert!(text.contains("endif"));
        }
        other => panic!("unexpected {other:?}"),
    }
    match &script.items[1] {
        Stmt::Command { text, .. } => {
            let text = String::from_utf8_lossy(text.as_bytes());
            assert!(text.contains("syntax region X"));
            assert!(text.contains("start='a'"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn script_level_defs_in_2html_style() {
    let source = r#"vim9script
var whatterm = 'gui'
if whatterm == 'gui'
  def HtmlColor(color: string): string
    return color
  enddef
else
  def HtmlColor(color: string): string
    var cterm_color: dict<string> = {}
    if has_key(cterm_color, color)
      return cterm_color[color]
    else
      return ''
    endif
  enddef
endif
var fgc = HtmlColor('#ffffff')
"#;
    let script = parse(source);
    assert_eq!(script.items.len(), 3);
}

#[test]
fn missing_endif_is_an_error() {
    let text = parse_error("if x == 1\n  var a = 1\n");
    assert!(text.contains("E580") || text.contains("endif"), "{text}");
}

#[test]
fn missing_enddef_is_an_error() {
    let text = parse_error("def F(): void\n  var a = 1\n");
    assert!(text.contains("E1098") || text.contains("enddef"), "{text}");
}

#[test]
fn trailing_characters_are_an_error() {
    let text = parse_error("var x = 1 2\n");
    assert!(text.contains("E488"), "{text}");
}

#[test]
fn missing_endclass_is_an_error() {
    let text = parse_error("class A\n  var x: number\n");
    assert!(
        text.contains("E1065") || text.contains("endclass"),
        "{text}"
    );
}

#[test]
fn unterminated_string_is_an_error() {
    let text = parse_error("var x = 'abc\n");
    assert!(text.contains("E115") || text.contains("string"), "{text}");
}

#[test]
fn line_continutations_in_expressions() {
    let script = parse(
        "var text: string = 'V'\n  .. 'i'\n  .. 'm'\nvar version: number = 9 *\n  100\n  + 2\nvar latest = v:version == 902\n  ? 'current' :\n  'out-of-date'\nvar chain = 'a'\n  ->split(',')\n  ->len()\n",
    );
    assert_eq!(script.items.len(), 4);
}

#[test]
fn dict_with_literal_key_that_looks_like_scope() {
    let script = parse("var d = {g: 1, v: 2}\n");
    match &script.items[0] {
        Stmt::Var(VarDecl {
            init: Some(Expr::Dict(entries)),
            ..
        }) => {
            assert_eq!(entries.len(), 2);
            assert!(matches!(entries[0].0, DictKey::Literal(_)));
        }
        other => panic!("unexpected {other:?}"),
    }
}
