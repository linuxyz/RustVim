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
| B | 45 | 48 | 3 |
| C | 461 | 790 | 329 |

## Explicit gap list

The failures above are the current, honest gap list.  They cluster in:

1. **Regex dialect edges in syntax files**: trailing backslashes
   (`E14: Trailing backslash`), `\z`/`\%` contextual atoms and `\(`
   groups used with Vim's magic rules need broader regex support
   (`unsupported atom`, `E54`).
2. **Script expression edges**: some syntax/compiler files use expressions
   (comparisons, `version`, autoload calls) that the legacy VimL interpreter
   evaluates strictly; `E015`, `E121`, `E117: Unknown function:
   executable`.
3. **Feature-gated options**: P2 features such as folds are absent, so
   `E518: Unknown option: foldmethod` appears in files that set fold options.
4. **Missing runtime files in user-local trees**: `E013` when a syntax file
   sources a component that does not exist in the pinned tree.

Tier C is explicitly *not* a release gate: P1 targets core editing, help and
syntax, and the gap list above tracks the remaining work towards Tier C.
