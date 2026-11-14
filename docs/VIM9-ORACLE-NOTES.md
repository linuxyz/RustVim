# Vim9 language facts verified with a real Vim

These behaviours were checked against the local /usr/bin/vim (Vim 9.1) with
vim -Nu NONE -n -es -i NONE while implementing the lexer/parser.  Target is the
pinned Vim 9.2.1338 runtime; 9.1 is close enough for the language surface the
runtime uses, and anything that differs is noted.

## Declarations and blocks

- "vim9script" is the first command; "vim9script noclear" is valid.
- Interface methods are declarations without a body and **without** :enddef:
    interface Animal
      var name: string
      def Speak(): string
    endinterface
- Abstract class methods likewise: "abstract def Foo(): void" has no :enddef.
- Class generics are NOT implemented by Vim (vim9class.txt: "Generics for a
  class" is under "To be done later"): class Foo<T> is a syntax error.  The
  parser accepts it permissively but it is out of scope.
- Generic functions are supported: def Map<T>(x: T): T ... enddef.  Calling
  them requires explicit type arguments (Map<number>(1)); the parser does not
  handle call-site type arguments yet (not used by the 9.2 runtime).
- Enum members must be separated by commas (a trailing comma after the last
  member is fine):
    enum Color
      Red,
      Green,
    endenum
  Enum members cannot have values ("Red = 1" is E1123).
- type aliases: "type MyList = list<number>" works.  Generic type aliases
  (type MyList<T> = ...) are not accepted by Vim 9.1.
- export supports var/const/final/def/function/class/abstract class/
  interface/enum/type; anything else is E1043.
- import syntax is "import {expr} [as {name}]" where {expr} is evaluated (e.g.
  a string literal, an interpolated string, a variable).  The old
  "import Name from 'file'" form is obsolete.  "import autoload {expr}"
  postpones loading and resolves under the autoload directory by default.
  Import paths may be relative (./ ../), absolute, or searched in "import/"
  subdirectories of 'runtimepath'.
- Nested def inside def works in Vim 9.1+ and captures outer locals.  Defs
  inside script-level if/else blocks are allowed (runtime/syntax/2html.vim
  uses both patterns).

## Lexical details

- Comments use # only; # starts a comment at line start or after white space.
  "var x = 1#comment" is E488, not a comment.  "#{" at the start of a line is
  E1170 ("Cannot use #{ to start a comment").  "#{{{" fold markers are comments.
- Heredocs: the assignment is written "=<<" with no space between "=" and
  "<<"; "= << trim END" is a syntax error.  Flags "trim" and "eval" may follow.
  The end marker must be at column 1 (trim does not allow indenting the end
  marker).  The marker may be quoted: =<< trim "END" ... "END" works.
- The statement "echo F(1,2)" is rejected with E1069 (white space required
  after ","); Vim9 requires white space around operators/commas in many
  places.  The parser accepts both for now (strictness comes with the checker).
- Blobs and numbers are covered by the lexer tests; delegates should verify
  exact forms there.
- "not in" is not a Vim9 operator; "isnot" is valid.  "in" is a comparison.
- Tuples exist (Vim 9.1 patch 1232): (1, 2) is a tuple, (7,) is a one-item
  tuple, tuple<number, string> and variadic tuple<...list<T>> are types.
  Note the parser stores them as Expr::Tuple.

## Ex command lines inside Vim9

- Ex commands are still used inside def bodies (execute, echomsg, set,
  normal!, syntax, ...).  Multi-line Ex commands continue with a leading
  backslash or a leading "|" line; both are appended to the previous command
  by the parser.  A leading ":" is allowed (":silent! ...") and kept in the
  captured text.
- :catch patterns are regexes (catch /^Vim:Interrupt$/); the parser captures
  the rest of the line verbatim instead of tokenizing it.

## Vim 9.2 additions and edge cases found while parsing the runtime

Verified while making every runtime Vim9 file parse:

- "??" is the falsy operator (expr1 precedence, below ||, same level as the
  ternary): "x ?? default".  It is not a binary "between AND and OR"
  operator for the parser precedence table.
- "defer {expr}" is a statement (runtime uses "defer () => { ... }()").  It
  takes a call/expression argument like :eval.
- Functions can be global: "def g:Name()" is valid ("def s:Name()" is not);
  the parser accepts a scoped name after :def.  "def!" is rejected by Vim
  (E477).
- Single-line compound statements are valid and used in the runtime:
    if cond | finish | endif
    if cond | return | endif
    if cond | a ..= "x" | endif
    while cond | col = col + 1 | endwhile
  Also "if cond" newline "| cmd" newline "| endif" in command definitions.
  The parser has an inline bar mode for this.
- A standalone expression can continue with "->" on a following line, with
  comment lines in between:
    tags
      # comment
      ->filter(...)
  The parser treats such a line as a language statement, not a command.
- Enums can have constructor arguments and a member body:
    enum Planet
      Earth(1, false),
      Mars(2, false)
      var moons: number
      def new(this.moons, this.rings)
      enddef
    endenum
  Enum members require commas; "Earth = 1" is invalid (E1123).
- Heredoc with "trim" accepts a terminator indented with the exact leading
  white space of the assignment line (the lexer compares that prefix).  A
  quoted marker is part of the marker text, i.e. terminator must be "END"
  including the quotes.  Nothing (not even trailing blanks) may follow the
  marker on the assignment line or the terminator line; ":comment" after the
  marker on the assignment line is allowed.
- Runtime files sometimes end a compiled block with a legacy quote comment:
  "endif \"}}}" is accepted by Vim (but "enddef \"}}}" is E1173).  The
  lexer tolerates a quote comment after endif/endwhile/endfor/endtry only.
- vim9-mix: files can start with legacy quote comments or an
  "if !has('vim9script') ... finish ... endif" guard before "vim9script".
  The front end skips such a preamble; the script loader (Phase 3) must
  actually execute the legacy part for real vim9-mix files (runtime only
  uses comment-only preambles).
- One-line Ex command continuations with a leading "|" (command definitions)
  are appended to the previous raw command by the parser; the evaluator must
  process bar-separated Ex command lists in Phase 2.

## Open items for later phases

- Call-site generic type arguments (Map<number>(...)): not parsed yet.
- Tuple unpacking with typed items (var [a: string, b]: ...) works in the
  parser; the evaluator must honour the per-item types.
- Super/this are ordinary names to the parser (Member access); the evaluator
  gives them meaning.
- Legacy "\" comments after block ends are tolerated by the lexer only for
  endif/endwhile/endfor/endtry (Vim rejects them after enddef; E1173).
