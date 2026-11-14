# Runtime compatibility matrix (RT-11)

RVim targets the unmodified Vim 9.2.1338 runtime tree.  Compatibility is
measured by executing that tree, not by reimplementing plugins.  The matrix is
produced by [`tools/runtime_compat.py`](../tools/runtime_compat.py) and is used as
a release gate in CI.

## Tiers

| Tier | Meaning | Gate |
|---|---|---|
| A | Core editing, help and syntax for common languages (rust, python, c/cpp, js, json, markdown, vim, sh, html, css, yaml, toml) | **required**: any Tier A failure fails the run |
| B | Common plugins, colorschemes and compiler definitions | reported, not gated |
| C | Every `syntax/*.vim` file in the pinned runtime | reported, not gated |

## Running

~~~sh
# The runtime tree is not vendored; point VIMRUNTIME at an unpacked v9.2.1338 tree.
cargo build -p rvim-cli
VIMRUNTIME=/path/to/vim-9.2.1338/runtime \
  python3 tools/runtime_compat.py --runtime "$VIMRUNTIME" --tiers a,b,c
~~~

The command writes:
- `target/compat/compat-report.json` - machine-readable matrix,
- `target/compat/compat-report.md` - the same data as Markdown (CI artifact).

`--gate` makes Tier A failures exit non-zero; CI runs
`--tiers a,b --gate` on every push to `main` and weekly against the pinned
runtime.  Tier C is run weekly as well (it takes a few minutes).

## Matrix schema

Each tier records `total`, `passed`, `failed` and a `failures` array of
`{check, error}` items.  Checks are named:

- `help syntax` - `:help syntax` finds the installed help tags;
- `syntax:<language>` - opening a sample file with `:syntax on` reports no
  error;
- `colorscheme:<name>` / `compiler:<name>` - the runtime file loads;
- `syntax-file:<name>` - `:source $VIMRUNTIME/syntax/<name>.vim` reports no
  error.

## Current snapshot

Local run against the pinned v9.2.1338 runtime (2026-11-14):

| Tier | Passed | Total | Failures |
|---|---|---|---|
| A | 14 | 14 | 0 |
| B | 47 | 48 | 1 |
| C | 770 | 790 | 20 |

Tier A is the CI gate; the remaining Tier B and Tier C failures are classified
below.

## Explicit gap list

The failures above are the current, honest gap list.  They cluster in:

1. **Vim9 script runtime files (19 Tier C checks plus compiler/context.vim in
   Tier B).** Vim 9.2 converted a number of runtime files to Vim9 script
   (`vim9script`, `def`, `import`), including 2html, context, csv, hare,
   haredoc, help, hitest, hlsl, mf, mp, odin, omnimark, tsv, java, antlr,
   javacc, aidl, sqlj and xpm, plus `compiler/context.vim`.  Vim9 script
   evaluation is not available yet, so these files report `E319`; the Vim9
   front end (lexer, AST, parser) is implemented and parses all 65 real Vim9
   runtime files - see [VIM9-PLAN.md](VIM9-PLAN.md).  The remaining work is
   the type checker/evaluator (Phase 2) and the `has('vim9script')` flip.
2. **Known harness artefact (1 check).** `syntax/glimmer.vim` requires
   `b:current_syntax` to have been set by a previously loaded JavaScript or
   TypeScript syntax file.  Sourcing it standalone fails in Vim 9.1 as well
   (exit status 1), so the check is environment dependent rather than an RVim
   gap.

Everything else in the pinned runtime loads without errors.  Vim 9.2
interpolated strings (`$'...'` / `$"..."`) and dynamic
`:let {expr}name` variable names are now implemented by the legacy VimL
engine and no longer appear as gaps.

Shared documented approximations behind these numbers:

- Regex collections containing multi-byte characters accept any UTF-8 byte of
  the character instead of matching the whole character, and contextual atoms
  (`\%#`, `\%V`, line/column atoms) compile but never match because the
  regex crate has no editor context.
- Folding, signs, text properties and diff are implemented (`has('folding')`,
  `has('signs')`, `has('textprop')` and `has('diff')` are 1), so runtime
  files that branch on those features now take the folding/sign/text-property/
  diff paths; the matrix above was re-run with them enabled. Concealment remains a P2 feature
  (`has('conceal')` is 0), and the conceal options are accepted and stored
  only so that runtime files load.

Tier C is explicitly *not* a release gate: P1 targets core editing, help and
syntax, and the gap list above tracks the remaining work towards Tier C.
