//! Vim9 script abstract syntax tree.
//!
//! The AST is deliberately close to the language surface, not to Vim's
//! bytecode: Vim9 static checks and evaluation are implemented over this tree.
//! Line numbers are kept on every statement so later phases can report
//! Vim-compatible error messages.

use rvim_types::VimString;

use crate::lexer::{OptionScope, Scope};

use super::token::HeredocFlags;

/// A parsed Vim9 script.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    /// True when the script starts with a "vim9script" command.
    pub vim9_header: bool,
    pub items: Vec<Stmt>,
}

/// A statement in a script or function body.
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// var / final / const declaration, including destructuring.
    Var(VarDecl),
    /// def ... enddef, possibly nested.
    Def(DefDecl),
    If(IfStmt),
    While(WhileStmt),
    For(ForStmt),
    Try(TryStmt),
    Return {
        value: Option<Expr>,
        line: u32,
    },
    Throw {
        value: Expr,
        line: u32,
    },
    Break {
        line: u32,
    },
    Continue {
        line: u32,
    },
    /// "defer {expr}": evaluate later when leaving the scope.
    Defer {
        expr: Expr,
        line: u32,
    },
    /// Assignment to an existing variable, option, register, env var or index.
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
        line: u32,
    },
    /// Expression statement, always a call or method call in valid Vim9.
    Expr {
        expr: Expr,
        line: u32,
    },
    Import(ImportDecl),
    /// "export" followed by a declaration.
    Export(ExportDecl),
    Class(ClassDecl),
    Interface(InterfaceDecl),
    Enum(EnumDecl),
    TypeAlias(TypeAliasDecl),
    /// An Ex command line captured verbatim (set, execute, echomsg, ...).
    Command {
        text: VimString,
        line: u32,
    },
}

impl Stmt {
    pub fn line(&self) -> u32 {
        match self {
            Stmt::Var(decl) => decl.line,
            Stmt::Def(decl) => decl.line,
            Stmt::If(stmt) => stmt.line,
            Stmt::While(stmt) => stmt.line,
            Stmt::For(stmt) => stmt.line,
            Stmt::Try(stmt) => stmt.line,
            Stmt::Return { line, .. }
            | Stmt::Throw { line, .. }
            | Stmt::Break { line }
            | Stmt::Continue { line }
            | Stmt::Defer { line, .. }
            | Stmt::Assign { line, .. }
            | Stmt::Expr { line, .. }
            | Stmt::Import(ImportDecl { line, .. })
            | Stmt::Export(ExportDecl { line, .. })
            | Stmt::Class(ClassDecl { line, .. })
            | Stmt::Interface(InterfaceDecl { line, .. })
            | Stmt::Enum(EnumDecl { line, .. })
            | Stmt::TypeAlias(TypeAliasDecl { line, .. })
            | Stmt::Command { line, .. } => *line,
        }
    }
}

/// Assignment operators valid in Vim9: "=", "+=", "-=", "*=", "/=", "%=" and
/// "..=".  (There is no "&lt;&lt;=" and no "==" assignment confusion: "==" is a
/// comparison token.)
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

/// Which keyword introduced a variable declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    Var,
    Final,
    Const,
}

/// A declared name with an optional type.
#[derive(Clone, Debug, PartialEq)]
pub struct NamePattern {
    pub scope: Option<Scope>,
    pub name: VimString,
    pub ty: Option<TypeExpr>,
}

/// The left hand side of a declaration or for loop.
#[derive(Clone, Debug, PartialEq)]
pub enum VarPattern {
    Name(NamePattern),
    /// List unpacking: "\[a, b; rest\]".
    List {
        items: Vec<VarPattern>,
        rest: Option<Box<VarPattern>>,
    },
}

/// var / final / const.
#[derive(Clone, Debug, PartialEq)]
pub struct VarDecl {
    pub kind: VarKind,
    pub pattern: VarPattern,
    pub ty: Option<TypeExpr>,
    pub init: Option<Expr>,
    pub line: u32,
}

/// A function parameter: "name: type = default", "...name: list&lt;any&gt;".
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: VimString,
    pub ty: Option<TypeExpr>,
    pub default: Option<Expr>,
    /// "?name: type": the argument may be omitted.
    pub optional: bool,
    /// "...name: list&lt;T&gt;": collects the remaining arguments.
    pub varargs: bool,
    pub line: u32,
}

/// def ... enddef.
#[derive(Clone, Debug, PartialEq)]
pub struct DefDecl {
    pub name: VimString,
    /// Generic type parameters: "def Map&lt;T&gt;(...)".
    pub generics: Vec<VimString>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    /// "abstract def" or an interface method declaration.
    pub is_abstract: bool,
    /// "static def" inside a class.
    pub is_static: bool,
    pub line: u32,
}

/// One "if"/"elseif" branch.
#[derive(Clone, Debug, PartialEq)]
pub struct IfBranch {
    pub cond: Expr,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfStmt {
    pub branches: Vec<IfBranch>,
    pub else_body: Option<Vec<Stmt>>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WhileStmt {
    pub cond: Expr,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForStmt {
    pub pattern: VarPattern,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatchClause {
    /// Optional ":catch /pattern/" argument, raw text between the slashes.
    pub pattern: Option<VimString>,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TryStmt {
    pub body: Vec<Stmt>,
    pub catches: Vec<CatchClause>,
    pub finally_body: Option<Vec<Stmt>>,
    pub line: u32,
}

/// ":import \[autoload\] {expr} \[as {name}\]".
#[derive(Clone, Debug, PartialEq)]
pub struct ImportDecl {
    /// "import autoload": the script is loaded on first use.
    pub autoload: bool,
    /// Path expression (usually a string literal, but Vim evaluates it).
    pub path: Box<Expr>,
    /// Script-local namespace name, when "as {name}" is used.
    pub alias: Option<VimString>,
    pub line: u32,
}

/// "export" applied to a declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportDecl {
    pub item: Box<Stmt>,
    pub line: u32,
}

/// Class member visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Public,
    Private,
}

/// A class or interface member.
#[derive(Clone, Debug, PartialEq)]
pub enum ClassMember {
    Var {
        access: Access,
        /// "static var": class variable shared by all objects.
        is_static: bool,
        /// "public" is part of the declared API; abstract classes only.
        is_public: bool,
        pattern: NamePattern,
        ty: Option<TypeExpr>,
        init: Option<Expr>,
        line: u32,
    },
    Def {
        access: Access,
        def: DefDecl,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassDecl {
    pub name: VimString,
    pub generics: Vec<VimString>,
    pub is_abstract: bool,
    pub extends: Option<TypeExpr>,
    pub implements: Vec<TypeExpr>,
    pub members: Vec<ClassMember>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceDecl {
    pub name: VimString,
    pub generics: Vec<VimString>,
    pub extends: Vec<TypeExpr>,
    pub members: Vec<ClassMember>,
    pub line: u32,
}

/// One enum member, optionally with constructor arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct EnumMember {
    pub name: VimString,
    pub args: Vec<Expr>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: VimString,
    pub members: Vec<EnumMember>,
    /// Member declarations and the constructor after the member list.
    pub body: Vec<Stmt>,
    pub line: u32,
}

/// "type Name&lt;T&gt; = {type}".
#[derive(Clone, Debug, PartialEq)]
pub struct TypeAliasDecl {
    pub name: VimString,
    pub generics: Vec<VimString>,
    pub ty: TypeExpr,
    pub line: u32,
}

/// A type expression, as written in declarations.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeExpr {
    /// A named type: number, string, MyClass, T, list without parameter, ...
    Named(VimString),
    List(Box<TypeExpr>),
    Dict(Box<TypeExpr>),
    /// tuple&lt;T&gt; / tuple&lt;T1, T2&gt; / tuple&lt;...list&lt;T&gt;&gt; / tuple&lt;T, ...list&lt;U&gt;&gt;
    Tuple {
        items: Vec<TypeExpr>,
        /// Variadic tail: "...list&lt;T&gt;".
        variadic: Option<Box<TypeExpr>>,
    },
    /// Bare "func" when the payload is None.
    Func(Option<FuncType>),
    /// "?type": optional argument in a function signature.
    Optional(Box<TypeExpr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncType {
    /// None means "any arguments" (func: {type}); Some(vec!\[\]) means "no
    /// arguments" (func(): {type}).
    pub params: Option<Vec<TypeExpr>>,
    /// "...list&lt;T&gt;" tail.
    pub varargs: Option<Box<TypeExpr>>,
    /// None means "void" when written as "func(): void" and "any" when
    /// written as bare "func".  The parser stores the written type.
    pub ret: Option<Box<TypeExpr>>,
}

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
    And,
    Or,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Match,
    NotMatch,
    /// "==?" and "!=?" compare strings ignoring case.
    EqIgnoreCase,
    NeIgnoreCase,
    /// "=~?" and "!~?" match with 'ignorecase'.
    MatchIgnoreCase,
    NotMatchIgnoreCase,
    Is,
    IsNot,
    In,
    NotIn,
    /// Falsy operator: "left ?? right".
    Coalesce,
}

/// The body of a lambda: an expression or a statement block.
#[derive(Clone, Debug, PartialEq)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

/// A lambda: "(args) =&gt; expr" or "(args) =&gt; { statements }".
#[derive(Clone, Debug, PartialEq)]
pub struct Lambda {
    pub params: Vec<Param>,
    /// Optional explicit return type before "=&gt;".
    pub ret: Option<TypeExpr>,
    pub body: LambdaBody,
    pub line: u32,
}

/// One piece of an interpolated string expression.
#[derive(Clone, Debug, PartialEq)]
pub enum InterpSegment {
    Text(VimString),
    Value(Box<Expr>),
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number(i64),
    Float(f64),
    String(VimString),
    Blob(Vec<u8>),
    Bool(bool),
    /// null and the null_T values (null_string, null_list, ...), or a v:
    /// special value written via a scoped variable.
    Special(VimString),
    Heredoc {
        lines: Vec<VimString>,
        flags: HeredocFlags,
    },
    Interpolated(Vec<InterpSegment>),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    /// Dictionary literal.  Literal keys are stored as strings; computed keys
    /// as expressions.
    Dict(Vec<(DictKey, Expr)>),
    Ident {
        scope: Option<Scope>,
        name: VimString,
    },
    Option {
        scope: Option<OptionScope>,
        name: String,
    },
    Env(String),
    Register(u8),
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
        cond: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        /// Explicit generic type arguments: "First&lt;number&gt;(...)".
        type_args: Vec<TypeExpr>,
    },
    /// receiver-&gt;callee(args); callee is usually a name but can be any
    /// expression in parentheses.
    MethodCall {
        receiver: Box<Expr>,
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
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
    Lambda(Lambda),
}

/// A dictionary literal key.
#[derive(Clone, Debug, PartialEq)]
pub enum DictKey {
    /// "key:" literal key (stored without quotes; Vim normalizes later).
    Literal(VimString),
    /// 'quoted key': or "quoted key":
    String(VimString),
    /// \[expr\]: computed key.
    Expr(Box<Expr>),
}
