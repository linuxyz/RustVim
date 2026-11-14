# RVim

A Vim-compatible editor written in Rust, architected around a backend-neutral
cell UI, Vim's own runtime, and QuickJS as an extension language.

[中文说明](README.zh-CN.md)

**Status:** the 16-crate workspace builds a working TUI and headless editor.
P0 and P1 are complete: all 177 P0/P1 catalog features are implemented and
tested (editing, search/regex, VimL, syntax, runtime/help, windows/tabs,
mappings, autocommands, quickfix/tags/sessions and the daily-driver surface).
The P2 folding, signs, text-property and two-way diff subsystems are also
implemented. See [docs/STATUS.md](docs/STATUS.md) for the honest feature-by-
feature state, documented approximations and remaining P2/P3 work.

**Target:** Vim 9.2.1338 behavior.  
**Not:** Vi/Ex compatibility, GUI (interface only),
terminal emulator (descoped).

---

## What is here

| Document | What it answers |
|---|---|
| [docs/FEATURES.md](docs/FEATURES.md) | The complete, selectable feature list (346 features, IDs, phases, dependencies, scope). Start here. |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crate graph, state model, input/render pipeline, registries, risks, decisions. |
| [docs/UI-PROTOCOL.md](docs/UI-PROTOCOL.md) | The TUI/GUI/headless contract: frames, cells, attributes, events, capabilities. |
| [docs/WINDOWS.md](docs/WINDOWS.md) | Why windows live in the presentation layer; layout, viewports, tabs, mouse. |
| [docs/SCRIPTING.md](docs/SCRIPTING.md) | Legacy Vim script values, GC, parser, evaluator, host boundary. |
| [docs/QUICKJS.md](docs/QUICKJS.md) | QuickJS embedding, <code>vim</code> API, conversions, modules, promises, sandbox. |
| [docs/RUNTIME-COMPAT.md](docs/RUNTIME-COMPAT.md) | Vim runtime loading, filetype/ftplugin/indent/syntax/help, tiers, harness. |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestones M0-M6, gates, recommended first slice. |
| [docs/EXCLUSIONS.md](docs/EXCLUSIONS.md) | Vi/Ex, Vim9, GUI, terminal emulator, optional features and their contracts. |
| [docs/VIM9-PLAN.md](docs/VIM9-PLAN.md) | Vim9 script implementation plan, phase gates and progress (runtime-grade, feature enabled). |
| [docs/STATUS.md](docs/STATUS.md) | Implemented P0/P1 surface, tests and known gaps. |
| [docs/inventory/](docs/inventory/README.md) | Generated Vim command/option/function/key/event/feature inventories. |

## Quick start

~~~sh
cargo build --workspace
cargo test --workspace
cargo run -p rvim-cli -- README.md
~~~

`rvim --headless --keys 'ihello<Esc>:wq<CR>' file` drives the same editor core
without a terminal. Point `VIMRUNTIME` at an unpacked Vim 9.2.1338 runtime to
load its syntax, ftplugin, help and color files.

---

## Requirement mapping

| Requirement | Where it is designed |
|---|---|
| Core features with a TUI/GUI interface | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/UI-PROTOCOL.md](docs/UI-PROTOCOL.md) |
| tabline/statusline/cmdline not bound to the UI | [docs/UI-PROTOCOL.md](docs/UI-PROTOCOL.md) section 11; crate rule in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) section 3 |
| Window implementation combined with the UI layer | [docs/WINDOWS.md](docs/WINDOWS.md) |
| GUI deferred | [docs/EXCLUSIONS.md](docs/EXCLUSIONS.md) section 4; GUI seam in [docs/UI-PROTOCOL.md](docs/UI-PROTOCOL.md) section 14 |
| Vim9 documented limits | [docs/EXCLUSIONS.md](docs/EXCLUSIONS.md) section 3; VIM9-01..05 in [docs/FEATURES.md](docs/FEATURES.md) |
| QuickJS integrated | [docs/QUICKJS.md](docs/QUICKJS.md); JS-01..12 in [docs/FEATURES.md](docs/FEATURES.md) |
| Vim runtime compatibility | [docs/RUNTIME-COMPAT.md](docs/RUNTIME-COMPAT.md) |
| No Vi/Ex compatibility | [docs/EXCLUSIONS.md](docs/EXCLUSIONS.md) section 2 |
| Terminal emulator descoped | [docs/EXCLUSIONS.md](docs/EXCLUSIONS.md) section 5; [docs/ROADMAP.md](docs/ROADMAP.md) |

## The one constraint that shapes everything

The editor core must not know about terminals or GUI toolkits. It produces a
<code>Frame</code> of cells; backends rasterize it and send input events. Inside
that core:

- **tabline, statusline and cmdline** are view widgets in <code>rvim-view</code>.
  They evaluate Vim options/expressions and emit cells. They never import
  <code>rvim-tui</code> or a GUI crate.
- **windows** are viewports with cursors, folds and window-local options. They
  live in the presentation layer (<code>rvim-view</code>), between the buffer
  model and the backend. The buffer model has no concept of a window.
- **the TUI** is one backend. A GUI is another backend consuming the same
  frames. A headless backend makes CI deterministic.

This is enforced by crate dependencies and tested with golden frames, not by
convention alone.

---

## Quick start with the planning tools

Validate the feature catalog (IDs, dependencies, cycles, phases):

~~~sh
python3 tools/validate_catalog.py
~~~

Select a dependency-closed backlog:

~~~sh
# Walking skeleton (P0)
python3 tools/select_features.py --bundle walking --with-deps

# Daily driver (P0 + P1)
python3 tools/select_features.py --bundle daily --with-deps

# Specific capabilities
python3 tools/select_features.py --ids EDT-10,REGEX-02,JS-08 --with-deps

# By area and phase
python3 tools/select_features.py --areas VIML,FUN,REGEX --max-phase P2 --format ids
~~~

Regenerate the Vim inventories from a runtime tree:

~~~sh
python3 tools/gen_inventory.py --runtime /path/to/vim/runtime \
  --label "Vim 9.2.1338" --out docs/inventory
~~~

The checked-in inventory is from the pinned Vim 9.2.1338 runtime tree (tag
`v9.2.1338`, archive SHA-256
`eeb4370a8db32041af1c3b05159bdeeea6b781f5a32d0f1f68bd0c51d45880f3`). Test
fixtures under `testdir/` are excluded. The inventory is generated, not
hand-maintained; if the tag or checksum changes, re-run the command and
classify the diff in [docs/FEATURES.md](docs/FEATURES.md).

---

## How to choose features

1. Read the area summaries in [docs/FEATURES.md](docs/FEATURES.md).
2. Pick IDs, whole areas, or a phase bundle.
3. Run the selection tool; it adds transitive dependencies and orders them.
4. Reply with the selected IDs (or the generated list). The next step is
   scaffolding only those crates/features.

Recommended first decision set:

- **Walking skeleton:** <code>--bundle walking</code> (29 P0 features).
- **Daily driver:** <code>--bundle daily</code> (P0 + P1).
- **Runtime Tier A:** the RT/TAG/SYN/QF subset; see
  [docs/RUNTIME-COMPAT.md](docs/RUNTIME-COMPAT.md).
- **QuickJS:** JS-01..JS-12, plus the VIML/FUN/JOB dependencies.

Terminal emulator features are not in the catalog. Jobs/channels remain, so
automation and <code>:make</code>/<code>:grep</code> still work.

---

## Scope summary

| Area | In this design |
|---|---|
| Vim Normal/Insert/Visual/Select/Command-line modes | Yes |
| Vim Ex command language | Yes |
| Legacy Vim script | Yes |
| Vim 9.2 runtime | Yes, tiered and measured |
| Regex, syntax, folds, signs, text properties, diff | Yes |
| QuickJS | Yes |
| Jobs/channels/timers | Yes |
| TUI | Yes |
| GUI | Interface only |
| Vim9 script | Runtime-grade (`has('vim9script')` is 1); documented limits in EXCLUSIONS section 3 |
| Images (<code>+image</code>, sixel/kitty) | Deferred (P4) |
| Vi/Ex compatibility mode | No |
| Terminal emulator (<code>:terminal</code>, <code>term_*</code>) | Descoped |

---

## Design highlights

### Feature catalog with stable IDs

346 features across 33 areas, each with phase, complexity, dependencies and an
acceptance-oriented scope. The catalog is validated for missing dependencies,
cycles and phase inversions on every change. The mechanical inventory covers
593 Ex commands, 695 mode keys, 456 option specs, 605 functions, 136 autocommand
events and 211 <code>has()</code> flags from the pinned 9.2.1338 runtime.

### UI protocol, not a UI framework

The core sends cells and receives semantic events. Attributes are resolved per
capability (mono/16/256/truecolor). Mouse hit regions map to semantic actions,
so statusline/tabline clicks work in any backend. A GUI can be added without
touching editor semantics.

### Runtime as the oracle

The Vim runtime is pinned and unmodified. Compatibility is measured per file
(plugins, ftplugins, indents, syntax, colors, compilers, help tags), not
asserted. Vim9 and terminal gaps are classified, not hidden.

### Two script engines, one boundary

Legacy VimL is the compatibility engine; QuickJS is the modern extension engine.
Both use the same editor host API, value conversions and error model, but JS
cannot silently replace VimL semantics.

### Explicit exclusions

Vi/Ex compatibility, GUI, images (deferred) and the terminal emulator are
documented with runtime contracts (<code>has()</code>, errors, skip policy) and
re-entry criteria. Unsupported features fail loudly or return honest flags.

---

## Planned repository layout

~~~text
rvim/
  Cargo.toml
  runtime.lock      pinned Vim tag, source URL and archive checksum
  crates/           rvim-types, rvim-ui-api, rvim-text, rvim-regex,
                    rvim-viml, rvim-ex, rvim-edit, rvim-syntax,
                    rvim-runtime, rvim-view, rvim-editor, rvim-tui,
                    rvim-headless, rvim-cli, rvim-js
  runtime/          pinned Vim 9.2.1338 runtime (not forked)
  docs/             catalog, architecture, protocol, compatibility
  tools/            inventory, validation, selection
  tests/            oracle, integration, fixtures
  benches/
~~~

The terminal-emulator crate is not in the layout; it would return only with a
new scope decision.

---

## License

The engine license is not finalized. The vendored Vim runtime retains the Vim
license and copyright notices; redistribution must preserve them. See
[docs/RUNTIME-COMPAT.md](docs/RUNTIME-COMPAT.md#2-pinning-and-integrity).

---

## Next step

Pick the features you want. The catalog, tooling and design are ready to turn
into a scoped implementation plan and then a Cargo workspace.
