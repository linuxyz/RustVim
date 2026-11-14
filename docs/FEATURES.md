# RVim feature catalog

**Target:** Vim 9.2.1338 semantics, in Rust, with a terminal UI first and a
backend-agnostic presentation layer. GUI is designed for but not implemented.
Vim9 script is deferred. Vi/Ex compatibility is out of scope.

This catalog is the **selection surface**. Each row is a shippable capability
with a stable ID, phase, complexity, dependencies and scope. The exhaustive
per-item appendices are in [inventory/](inventory/README.md).

## How to select

1. Pick rows by ID (for example `EDT-10`, `REGEX-02`, `JS-08`), or pick whole
   areas/phases.
2. Run `python3 tools/select_features.py --ids ... --with-deps` to get a
   dependency-closed, topologically ordered backlog.
3. The phase is a dependency-aware recommendation, not a promise. P0 is the
   walking skeleton; P1 is a daily-driver editor; P2 is Vim parity for one
   user; P3 is ecosystem/runtime completeness; P4 is optional or deferred.

### Phase legend

| Phase | Meaning | Exit criterion |
|---|---|---|
| **P0** | Walking skeleton | Open a file in the TUI, move, insert, delete, undo, write, quit; frames are snapshot-tested headlessly. |
| **P1** | Daily driver | Splits/tabs, visual mode, search/substitute, mappings, VimL core, syntax, runtime loading, help and options are usable. |
| **P2** | Vim parity | Folds, signs, text properties, diff, quickfix, tags, sessions, completion and advanced display work. |
| **P3** | Ecosystem | Jobs/channels, QuickJS, spell, packages, runtime tiers, encryption, profiling and compatibility corpus. |
| **P4** | Optional/deferred | Printing, RTL, clientserver, legacy protocols, Vim9 script and platform exotica. |

### Complexity legend

| Cx | Rough size | Typical examples |
|---|---|---|
| **S** | under ~1k LOC | a listing command, an option validator |
| **M** | 1k–5k LOC | statusline engine, marks, quickfix model |
| **L** | 5k–20k LOC | VimL evaluator, window layout, syntax engine |
| **XL** | over 20k LOC, high risk | Vim regex, full VimL + builtins, syntax, spell |

### Scope rules

- **In scope:** Vim's Ex command language, Normal/Insert/Visual/Select
  modes, legacy Vim script, Vim 9.2 runtime, TUI, QuickJS.
- **Out of scope:** Vi/Ex compatibility mode, `-e`/`-E` Ex mode, `'compatible'`
  behavior, Vi defaults, Vim9 script (deferred), GUI toolkit, Neovim APIs.
  The terminal emulator (<code>:terminal</code>, Terminal-Job mode, term_*
  functions) is descoped. The <code>+image</code> API is deferred (MISC-12).
- **Channel host exception:** `--stdio-channel` implies silent `-es`; this is
  allowed only as a channel host, not an Ex compatibility layer.
- **UI rule:** tabline, tabpanel, statusline and cmdline are core view widgets rendered
  into cells. They must not import a terminal or GUI crate. Windows belong to
  the presentation layer, not the buffer model and not the backend.
- **Runtime rule:** never fork runtime files. Compatibility means executing
  Vim 9.2's own `runtime/` tree, with honest `has()`/`exists()` answers.
- **Honesty rule:** unsupported features fail with a documented error or a
  false `has()` value; they never silently corrupt state.

## Catalog at a glance

Total: **346 features**

| Phase | Features |
|---|---:|
| P0 | 29 |
| P1 | 148 |
| P2 | 103 |
| P3 | 52 |
| P4 | 14 |

| Area | Name | Features | P0 | P1 | P2 | P3 | P4 |
|---|---|---:|---:|---:|---:|---:|---:|
| FND | Foundation and process | 10 | 7 | 3 | 0 | 0 | 0 |
| OPT | Options | 9 | 2 | 5 | 2 | 0 | 0 |
| BUF | Buffers and files | 17 | 2 | 10 | 5 | 0 | 0 |
| UND | Undo and redo | 6 | 1 | 2 | 3 | 0 | 0 |
| WIN | Windows and layout | 12 | 1 | 6 | 4 | 1 | 0 |
| TAB | Tab pages | 8 | 0 | 3 | 4 | 1 | 0 |
| UI | UI protocol, compositor and TUI | 15 | 7 | 5 | 2 | 1 | 0 |
| STA | Statusline and tabline widgets | 8 | 1 | 4 | 1 | 2 | 0 |
| CMD | Command line | 7 | 1 | 6 | 0 | 0 | 0 |
| MSG | Messages, errors and prompts | 6 | 1 | 4 | 1 | 0 | 0 |
| EDT | Editing core | 25 | 2 | 18 | 5 | 0 | 0 |
| INS | Insert and replace modes | 12 | 1 | 7 | 4 | 0 | 0 |
| VIS | Visual and Select modes | 5 | 0 | 3 | 2 | 0 | 0 |
| EX | Ex command engine | 13 | 1 | 8 | 2 | 1 | 1 |
| MAP | Mappings and keys | 8 | 0 | 3 | 4 | 1 | 0 |
| AUC | Autocommands | 8 | 0 | 7 | 1 | 0 | 0 |
| VIML | Legacy Vim script | 18 | 0 | 14 | 3 | 1 | 0 |
| FUN | Built-in function families | 18 | 0 | 0 | 15 | 3 | 0 |
| REGEX | Vim regular expressions | 9 | 0 | 6 | 3 | 0 | 0 |
| SYN | Syntax and highlighting | 13 | 0 | 7 | 5 | 1 | 0 |
| DSP | Display features | 14 | 0 | 4 | 9 | 1 | 0 |
| QF | Quickfix and location lists | 8 | 0 | 1 | 7 | 0 | 0 |
| TAG | Tags and help | 8 | 0 | 5 | 3 | 0 | 0 |
| SPELL | Spell checking | 8 | 0 | 0 | 0 | 8 | 0 |
| JOB | Jobs, channels and timers | 8 | 0 | 0 | 1 | 6 | 1 |
| JS | QuickJS integration | 12 | 0 | 0 | 0 | 12 | 0 |
| RT | Vim runtime compatibility | 18 | 0 | 8 | 6 | 4 | 0 |
| ENC | Encoding and multibyte text | 8 | 1 | 3 | 3 | 1 | 0 |
| CRYPT | Encryption | 4 | 0 | 0 | 0 | 4 | 0 |
| SEC | Security, sandbox and limits | 6 | 0 | 2 | 2 | 2 | 0 |
| TST | Testing and quality | 8 | 1 | 1 | 5 | 1 | 0 |
| MISC | Platform and optional features | 12 | 0 | 3 | 1 | 1 | 7 |
| VIM9 | Deferred Vim9 script | 5 | 0 | 0 | 0 | 0 | 5 |

---

## FND — Foundation and process

Gate: a headless test binary can open a file, inject keys, run an Ex command,
render a frame and compare it to a golden snapshot.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| FND-01 | Cargo workspace and crate graph | P0 | S | — | Workspace with the crates in ARCHITECTURE.md, feature flags, CI on macOS/Linux, pinned MSRV, license notices. |
| FND-02 | Core IDs, errors and diagnostics | P0 | S | FND-01 | Newtypes for buffer/window/tab/script IDs; `VimError` with `E###` codes, message text and throwpoint; no panics across API boundaries. |
| FND-03 | Event loop and redraw scheduler | P0 | L | FND-01 | Single-threaded scheduler for input, timers, job events, screen resize and deferred redraw; nearest-deadline waits; no busy loop. |
| FND-04 | Registries for commands/options/functions/events | P0 | M | FND-02 | One metadata table per surface: name, abbreviation, arity, flags, feature gate, doc tag, handler. Drives parsing, completion, `:help` and the inventory audit. |
| FND-05 | Editor state ownership and arenas | P0 | M | FND-02 | `Editor` owns buffers, windows, tab pages, scripts, registers, marks and global variables; slotmap/indexmap arenas, explicit borrow-splitting for render. |
| FND-06 | Headless driver and golden frames | P0 | M | FND-01, UI-12 | Scriptable editor driver: feed keys, run Ex, inspect model, snapshot Frame; used by every subsystem test. |
| FND-07 | Platform abstraction | P0 | L | FND-01 | Filesystem, process spawn, PTY, signals, terminal size, clipboard, locale, environment; Unix first, Windows behind the same traits. |
| FND-08 | Benchmark and fuzz harness | P1 | M | FND-06 | Criterion-style benchmarks for startup, frame render, regex, syntax and large files; cargo-fuzz targets for parsers and input. |
| FND-09 | CLI and startup orchestrator | P1 | M | FND-04, FND-07 | Args (`-c`, `-S`, `-u`, `--clean`, `-p`, `-o`, `-O`, `-R`, `-r`, `--headless`, `--stdio-channel` handoff), init order, `--startuptime`, exit codes; `--headless` is an RVim extension. |
| FND-10 | Tracing and verbose diagnostics | P1 | S | FND-02 | Category-based logs (`input`, `ex`, `viml`, `render`, `runtime`, `job`, `js`), `'verbose'` levels, `:verbose` source tracing, crash report. |

## OPT — Options

Gate: all option specs load; `:set` round-trips every type/scope; modelines and
option events behave like Vim in differential tests.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| OPT-01 | Option registry and typed values | P0 | L | FND-04 | All Vim 9.2.1338 options with type, scope, default, flags, validator, short name and doc tag; boolean/number/string with typed storage, not stringly-typed; feature-gated options report availability honestly. |
| OPT-02 | `:set` parser, flags and abbreviations | P0 | M | OPT-01 | `set`/`setlocal`/`setglobal`, `?`, `&`, `&vim`, `!`, `inv`, `no`, `+=`/`-=`/`^=`, `<` reset, `all`, unambiguous abbreviations. |
| OPT-03 | Scopes, inheritance and global-local options | P1 | L | OPT-01 | Global, buffer, window and tab scopes; global-local options (`'statusline'`, `'errorformat'`, `'tags'`); `:setlocal` fallback semantics; option <-> window ownership. |
| OPT-04 | Option access from VimL | P1 | M | OPT-03, VIML-03 | `&opt`, `&l:opt`, `&g:opt`, `setbufvar()`/`setwinvar()` option keys, `getbufvar()` readback, option reset rules. |
| OPT-05 | Defaults and reset semantics | P1 | S | OPT-02 | Vim defaults (not Vi), `&vim`/`all&`, `--clean`, `-u DEFAULTS`, exclusion of `'compatible'` behavior, startup option files. |
| OPT-06 | Modelines | P1 | M | OPT-03 | `'modeline'`/`'modelines'`/`'modelinestrict'` parsing, first/last N lines, safe-option allowlist, secure filtering, OptionSet side effects and tests. |
| OPT-07 | TTY and `t_*` options | P2 | M | OPT-01, UI-05 | `:set termcap`, `t_Co`/`t_AB`-style key codes bridged to the TUI key table; capability reporting; never let UI code own Vim option semantics. |
| OPT-08 | `:options` window | P2 | M | OPT-03, BUF-09 | `optwin.vim` compatibility: synthetic buffer, per-option jump tags, toggling and `:set` round-trip. |
| OPT-09 | OptionSet event and side effects | P1 | M | OPT-03, AUC-01 | Fire `OptionSet` with `v:option_*`, apply side effects (wrap, numberwidth, syntax, filetype, foldmethod), avoid recursive set storms. |

## BUF — Buffers and files

Gate: byte-exact round-trip for unix/dos/mac, BOM and Latin-1/UTF-8 files;
swap recovery restores an edited buffer after a killed process.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| BUF-01 | Line buffer and byte indexing | P0 | L | ENC-01, FND-05 | Stable line IDs, byte-indexed access, NUL/NL fidelity, `changedtick`, O(log n) line insert/delete via chunked line tree, O(1) line count. |
| BUF-02 | Buffer list and lifecycle | P1 | L | BUF-01 | Listed/unlisted, loaded/unloaded, hidden, `:badd`/`:bufadd`/`:bufload`/`:bdelete`/`:bwipeout`, alternate buffer `#`, `CTRL-^`/`CTRL-6`, `BufAdd/BufDelete/BufUnload` events. |
| BUF-03 | File read/write core | P0 | L | BUF-01, FND-07 | `:edit`/`:write`/`:read`, address ranges, `:write !cmd`, `:read !cmd`, filters, permissions, `'writeany'`, partial writes, errors without data loss. |
| BUF-04 | File format, EOL and BOM | P1 | M | BUF-03 | `'fileformat'` unix/dos/mac, `'fileformats'` detection, `'endofline'`, `'fixendofline'`, `'bomb'`, mixed-EOL reporting, byte-exact rewrite. |
| BUF-05 | File encodings and iconv | P1 | L | BUF-03, ENC-02 | `'fileencoding'`, `'fileencodings'` probe order, BOM/UTF-16, iconv fallback, `:scriptencoding`; internal encoding is UTF-8 only. |
| BUF-06 | Backup, swap and recovery | P1 | L | BUF-03, FND-07 | `'backup'`/`'writebackup'`/`'backupcopy'`/`'backupskip'`, swap file naming/permissions/`'updatetime'`, `:preserve`, `:recover`, `-r`, `v:swapchoice`. |
| BUF-07 | Marks, jumplist, changelist | P1 | L | BUF-01 | `a-z`/`A-Z`/`0-9` marks, special marks `'` `.` `^` `[` `]` `<` `>`, mark adjustment on edits, jumplist `<C-o>/<C-i>`, changelist `g;/g,`, `getmarklist()`. |
| BUF-08 | Buffer-local variables and options | P1 | M | BUF-01, OPT-03 | `b:` variables, buffer options, `b:changedtick`, `getbufvar()`/`setbufvar()`, `:let b:...`; lifetime tied to buffer wipe. |
| BUF-09 | Special buffer types | P1 | M | BUF-02 | `'buftype'` nofile/nowrite/acwrite/quickfix/help/prompt, `'bufhidden'`, `'swapfile'`, `'modifiable'`/`'readonly'` enforcement and plugin hooks. |
| BUF-10 | Argument list | P1 | M | BUF-02 | Local/global arglists, `:args`/`:next`/`:prev`/`:rewind`/`:argument`/`:argadd`/`:argdelete`, `argadd()`/`argv()`, `:argdo`. |
| BUF-11 | Working directory and path handling | P1 | M | BUF-02, FND-07 | `:cd`/`:lcd`/`:tcd`/`:pwd`, `'autochdir'`, `'path'`, `findfile()`/`finddir()`, `DirChanged` events, filename modifiers and escaping. |
| BUF-12 | File-changed detection | P2 | M | BUF-03, AUC-01 | `'autoread'`, `:checktime`, `FileChangedShell`/`FileChangedShellPost`, timestamp/size checks, dialog and `v:swapchoice` handling. |
| BUF-13 | Large files and binary mode | P2 | L | BUF-01 | `-b`, `'binary'`, `'maxmem'`-style limits, memory-mapped/paged line store option, huge-line handling, no quadratically slow edits. |
| BUF-14 | Sessions and views | P2 | L | BUF-02, WIN-01, OPT-03 | `:mksession`/`:mkview`/`:loadview`, `'sessionoptions'`/`'viewoptions'`, `SessionLoadPre`, window layout, folds, marks and local options, `v:this_session`. |
| BUF-15 | Viminfo and Shada | P2 | L | BUF-07, FND-07 | `'viminfo'`/`'shada'` parsing, history/registers/marks/`'0`-`'9`/oldfiles persistence, `:rviminfo`/`:wviminfo`, locking and corruption handling. |
| BUF-16 | Buffer text listeners | P2 | M | BUF-01 | `listener_add()`/`listener_remove()`, change notifications with start/end/added/touched, callback safety and reentrancy guards. |
| BUF-17 | File identity, permissions and `:saveas` | P1 | S | BUF-03 | `:file`/`:saveas`, read-only file handling, ownership/mode preservation, `'readonly'` override with `!`. |

## UND — Undo and redo

Gate: undo/redo sequences across 10k edits match a model oracle; persistent
undo survives process restart and Vim-compatible `.un~` files load.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| UND-01 | Undo tree and undo blocks | P0 | L | BUF-01 | Branching undo tree, sequence numbers, time stamps, block boundaries for insert/operator/Ex edits, `'undolevels'` pruning, `undotree()` data. |
| UND-02 | `u`/`<C-r>` and counts | P1 | M | UND-01, EDT-01 | Counted undo/redo, cursor restore, `'undolevels'` negative/zero special cases, message reporting. |
| UND-03 | Ex undo navigation | P1 | M | UND-01, EX-01 | `:undo`, `:redo`, `:earlier`/`:later` with counts and time units (`s`, `m`, `h`, `d`, `f`), `g-`/`g+`. |
| UND-04 | `:undojoin` and undo state APIs | P2 | M | UND-01, VIML-14 | `:undojoin`, `undojoin` from VimL, cursor/visual state restore, `undotree()`, `wundo()`/`rundo()` entry points. |
| UND-05 | Persistent undo | P2 | L | UND-01, BUF-07 | `'undofile'`/`'undodir'`, `:wundo`/`:rundo`, Vim-compatible undo file format (versioned), permissions and cleanup. |
| UND-06 | Undo across reload and `'undoreload'` | P2 | M | UND-05, BUF-03 | Preserve undo when rereading unchanged files, `'undoreload'` limit, `:e!` semantics, stale undo detection. |

## WIN — Windows and layout

Gate: split trees match Vim geometry for a scripted sequence of `:split`,
`:vsplit`, `:resize`, `wincmd` and terminal resize; mouse separator drag
updates the same layout model.

Windows live in the presentation layer (`rvim-view`): they own viewports,
cursors, folds and window-local options, but only emit cells. The buffer model
never knows a window exists.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| WIN-01 | Split tree, layout and sizes | P0 | L | FND-05, UI-01 | Frame tree of horizontal/vertical splits, min sizes, `'equalalways'`/`'eadirection'`, resize propagation, deterministic geometry for tests. |
| WIN-02 | Viewport, cursor and scroll state | P1 | L | WIN-01, BUF-01 | Per-window `topline`/`leftcol`/`skipcol`/`curswant`, independent cursors for one buffer, scrolloff, cursor clamping, `winsaveview()`/`winrestview()`. |
| WIN-03 | Window-local options and variables | P1 | M | WIN-01, OPT-03 | `w:` variables, window options (`'number'`, `'wrap'`, `'foldmethod'`, `'signcolumn'`...), inheritance on split and `:setlocal`. |
| WIN-04 | Window commands and `:wincmd` | P1 | M | WIN-01, EX-01 | `:split`/`:vsplit`/`:new`/`:vnew`/`:close`/`:only`/`:hide`/`:quit`/`:resize`/`:vertical resize`, `CTRL-W` command matrix, `:wincmd`. |
| WIN-05 | Window focus, movement and constraints | P1 | M | WIN-01 | `CTRL-W h/j/k/l/w/W/t/b/p`, `:wincmd`, `'winminheight'`/`'winminwidth'`, focus follows close, alternate window, `WinEnter`/`WinLeave`. |
| WIN-06 | Preview window | P2 | M | WIN-01, TAG-02 | `:pedit`/`:pclose`/`:ppop`, `'previewheight'`, one preview per tab, tag/help preview integration, `CTRL-W z`. |
| WIN-07 | Window IDs and scripting APIs | P2 | M | WIN-01, VIML-15 | `win_getid()`/`win_id2win()`/`win_findbuf()`/`winlayout()`/`win_execute()`/`winrestcmd()`, stable IDs across reordering. |
| WIN-08 | Scrolling options and synchronized views | P2 | L | WIN-02, OPT-03 | `'scroll'`, `'scrolloff'`/`'scrolloffpad'`/`'sidescrolloff'`, `'scrollbind'`/`'cursorbind'`/`'scrollopt'`, `CTRL-E/Y/D/U/F/B`, `zz/zt/zb`, `'smoothscroll'`. |
| WIN-09 | Window size constraints | P2 | M | WIN-01 | `'winfixheight'`/`'winfixwidth'`/`'winfixbuf'`, `'winheight'`/`'winwidth'`, `:resize` with counts, `win_move_separator()`/`win_move_statusline()`. |
| WIN-10 | Window decorations and separators | P1 | L | WIN-02 | Window separators, statusline rows (height supplied by the statusline widget), border/click regions, correct width accounting, resize repaint; gutters are DSP-10. |
| WIN-11 | Mouse separator and statusline drag | P3 | M | WIN-01, UI-04 | Hit regions on separators/statusline, drag resize, double-click equalize, `'mouse'` gating, no UI-specific geometry in the model. |
| WIN-12 | Global statusline and `'laststatus'` | P1 | S | WIN-01, STA-01 | `'laststatus'` 0/1/2/3, global statusline, statusline cache invalidation on window/buffer changes. |

## TAB — Tab pages

Gate: `:tab*` command matrix matches Vim; tabline and tabpanel cells, click
regions and `'tabline'`/`'tabpanel'` expressions are produced without a backend;
sessions restore tabs.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| TAB-01 | Tab-page model | P1 | M | WIN-01 | Ordered tab pages, each with a window tree and current window; tab-local variables/options; closing the last window closes the tab. |
| TAB-02 | Tab commands and navigation | P1 | M | TAB-01, EX-01 | `:tabnew`/`:tabedit`/`:tabclose`/`:tabonly`/`:tabmove`/`:tabnext`/`:tabprevious`/`:tabfirst`/`:tablast`, `gt`/`gT`/`<C-PageUp>`, counts. |
| TAB-03 | `'showtabline'` and tabline integration | P1 | M | TAB-01, STA-05 | 0/1/2 modes, tabline row ownership, minimum one tab, resize interaction, tabline hidden when appropriate. |
| TAB-04 | Tab-local scope and APIs | P2 | S | TAB-01, OPT-03 | `t:` variables, tab options, `gettabvar()`/`settabvar()`, `gettabinfo()`, `tabpagenr()`/`tabpagebuflist()`/`tabpagewinnr()`. |
| TAB-05 | `'tabline'` expression and clicks | P2 | M | TAB-03, STA-06 | Evaluate `'tabline'`, `%1T` clickable labels, close buttons `%X`, highlighting, `'guitablabel'`/`'guitabtooltip'` stored for future GUI. |
| TAB-06 | Tab lifecycle events and IDs | P2 | M | TAB-01, AUC-01 | Stable tabpage IDs, `TabNew`/`TabEnter`/`TabLeave`/`TabClosed`, `:tabclose` with modified buffers, `:tabonly` confirmation. |
| TAB-07 | Session and tab restoration | P2 | S | TAB-01, BUF-14 | `:mksession` tab order, current tab, per-tab window layout, tab-local options and working directories. |
| TAB-08 | Tabpanel sidebar and API | P3 | M | TAB-01, STA-05, STA-06, VIML-14 | `'showtabpanel'` 0/1/2, `'tabpanel'`/`'tabpanelopt'` (`columns`, `align`, `vert`, `scrollbar`), vertical sidebar layout, `TabPanel`/`TabPanelSel`/`TabPanelFill`, wheel/drag scrolling, `tabpanel_getinfo()`/`tabpanel_scroll()`, `:redrawtabpanel`, backend-neutral cells and hit regions. |


## UI — UI protocol, compositor and TUI

Gate: the TUI and the headless backend consume the same Frame; statusline,
tabline, cmdline, popup menu and messages appear identically in both. No crate
below the backend imports a terminal or GUI library.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| UI-01 | Cell, grid, frame and attribute model | P0 | M | FND-02 | Backend-neutral `Frame` with grids, `Cell` (grapheme cluster + attr id), `Attr` (fg/bg/sp, bold, italic, underline/curl/double/dotted/dashed, strike, reverse, blink, conceal), cursor and metadata. |
| UI-02 | `UiBackend` trait and capabilities | P0 | M | UI-01 | Lifecycle trait: init, present, poll event, resize, title, clipboard, suspend/resume, shutdown; capability negotiation for color depth, mouse, paste, focus, kitty keyboard, synchronized output, in-band resize, IME, pixel metrics and images. |
| UI-03 | Compositor | P0 | L | UI-01 | Layer stack (base, popups, external overlays), z-order, clipping, dirty rectangles/rows, cursor occlusion, frame sequence numbers and diff-friendly output. |
| UI-04 | Hit regions and mouse routing | P0 | M | UI-01 | Core registers clickable/draggable regions with semantic `ActionId` (window border, statusline item, tab, sign, fold, popup item); backends send coordinates only. |
| UI-05 | TUI backend | P0 | L | UI-02, FND-07 | Raw mode, alternate screen, terminfo/termcap lookup, `SIGWINCH` resize, `'termresize'` in-band resize, `'termsync'` synchronized output, title, bell, cursor shape, panic-safe terminal restore, `:set t_*` bridge. |
| UI-06 | TTY input decoding | P0 | L | UI-02, FND-07 | Function/arrow keys, modifiers, xterm/SGR/urxvt/X10 mouse, bracketed paste, focus events, `modifyOtherKeys` and kitty keyboard protocol; documented ESC/Alt ambiguity policy tied to `'ttimeoutlen'`. |
| UI-07 | Color negotiation and fallback | P1 | M | UI-02, SYN-02 | Mono/16/256/truecolor, `t_Co`, `'termguicolors'`, nearest-color mapping for cterm, GUI truecolor path, stable `AttrId` palette per frame. |
| UI-08 | Cursor, focus and paste events | P1 | M | UI-06 | Cursor style per mode, blink policy, focus in/out, bracketed-paste payloads, `'paste'` handoff, kitty key release/repeat filtering. |
| UI-09 | Suspend, resume and signals | P1 | M | UI-05, FND-07 | `SIGTSTP`/`SIGCONT`/`SIGINT`/`SIGTERM`, `:stop`/`:suspend`, terminal state save/restore around shell commands, `VimSuspend`/`VimResume`. |
| UI-10 | Clipboard providers | P1 | M | FND-07 | macOS `pbcopy/pbpaste`, Wayland `wl-copy/wl-paste`, X11 `xclip/xsel`, optional OSC 52; core owns `'clipboard'`/registers, provider only moves bytes. |
| UI-11 | GUI extension contract | P2 | S | UI-02 | Documented optional traits for pixel metrics, IME preedit, native menus, scrollbars, tab drag, fonts/ligatures and decorations. No GUI implementation in this project phase. |
| UI-12 | Headless snapshot backend | P0 | S | UI-02 | Deterministic 80x24 (configurable) cell dump, JSON snapshots, `screenchar()`-style inspection, diffable frames for CI. |
| UI-13 | Screen introspection functions | P2 | M | UI-01, VIML-14 | `screenchar()`/`screenchars()`/`screenstring()`/`screenpos()`/`screenattr()`/`screencol()`/`screenrow()`/`getbgcolor()`, cell-width aware. |
| UI-14 | Redraw control | P1 | M | UI-03 | `:redraw`/`:redraw!`, `'lazyredraw'`, `'redrawtime'`, partial updates, message/flush ordering, no flicker on large frames. |
| UI-15 | Redraw listeners | P3 | S | UI-03, VIML-05 | `redraw_listener_add()`/`redraw_listener_remove()`, `on_start`/`on_end` Funcref callbacks, listener IDs, main-thread ordering, sandbox and reentrancy errors, callbacks cannot trigger recursive redraw. |

## STA — Statusline and tabline widgets

Gate: statusline and tabline are produced entirely in `rvim-view`; a unit test
renders them with the headless backend and no terminal dependency. `'statusline'`
expressions see `g:statusline_winid` and errors are reported as Vim does.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| STA-01 | Statusline layout engine | P0 | M | UI-01, WIN-01 | Left/center/right groups (`%=`), truncation (`%<`), minimum widths, highlight run merging, wide-character width, click region capture. |
| STA-02 | Statusline item expansion | P1 | M | STA-01, VIML-11 | `%f %F %t %m %M %r %R %h %H %w %W %y %Y %l %L %c %C %v %V %o %O %p %P %a %{...} %#hl# %1* %0* %= %< %%%`, `%[FuncName]`/`%[]` click regions, `%@` newline and modifier flags. |
| STA-03 | Statusline expression evaluation and caching | P1 | M | STA-02, SYN-01 | Evaluate `'statusline'` per window with `g:statusline_winid`, sandbox errors into messages, cache by buffer/window/changedtick, invalidate on relevant events. |
| STA-04 | Mode, ruler and command feedback | P1 | M | STA-01, OPT-03 | `'laststatus'`, `'ruler'`/`'rulerformat'`, `'showcmd'`/`'showcmdloc'`, `'showmode'`, `'shortmess'`, counts, pending operator display and `CTRL-G` file info. |
| STA-05 | Tabline layout and items | P1 | M | TAB-01, UI-01 | Default tab labels, tab numbers, modified marker, `%1T` clickable tabs, `%X` close, `%#HL#`, `%{...}`, alignment/truncation, `'showtabline'`. |
| STA-06 | Statusline and tabline click actions | P2 | M | STA-01, UI-04 | Mouse press/double-click/wheel on statusline and tabline, semantic actions (focus window, close tab, scroll), `%[Func]` callback dispatch with `minwid`/`nclicks`/`button`/`mods`/`winid`/`area`/`tabnr`, `has('statusline_click')`, drag separation deferred to WIN-11. |
| STA-07 | GUI tabline hooks | P3 | S | UI-11, STA-05 | Metadata for native tab bars/tooltips/close buttons and drag reorder; design-only until a GUI backend exists. |
| STA-08 | Multi-line statusline | P3 | M | STA-01, WIN-10 | `'statuslineopt'` `fixedheight`/`maxheight:N`, `%@` newlines, per-window statusline height, global-local option semantics, layout/cache/hit-region updates, interaction with `'laststatus'` and window resize. |

## CMD — Command line

Gate: ex, search and expression prompts share one editable state machine;
completion produces the same matches as Vim for a scripted corpus; the
command-line window is a normal window over a synthetic buffer.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| CMD-01 | Command-line state machine and editor | P0 | L | UI-01, EX-01 | `:`, `/`, `?` and `=` prompts; insert/overwrite, cursor motions, word/line kill, `<C-r>`, `<C-v>`, digraphs, `<C-f>`/`<C-b>`, history recall, prompt type. |
| CMD-02 | Command-line history | P1 | M | CMD-01 | `'history'`, `:history`, `histadd()`/`histdel()`/`histget()`/`histnr()`, `@:` and `@@`, search history, duplicate suppression, persistence via BUF-15. |
| CMD-03 | Command-line completion engine | P1 | L | CMD-01, BUF-02 | Context detection and all completion types: commands, files/dirs, buffers, tags, options, functions, variables, mappings, autocmds, highlights, syntax, compiler, environment; `getcompletion()`. |
| CMD-04 | Wildmenu and popup completion | P1 | M | CMD-03, UI-01 | `'wildmenu'`, `'wildmode'`, `'wildoptions'` (fuzzy, pum, tagfile), `'pumheight'`/`'pumwidth'`, `'pumopt'`/`'pumborder'` borders, match highlighting, `wildmenumode()`, cancel semantics. |
| CMD-05 | Command-line window | P1 | M | CMD-01, BUF-09, WIN-01 | `q:`/`q/`/`q?`, `'cedit'`, `'cmdwinheight'`, execute on `<CR>`, cancel on `<C-c>`, editable history, `getcmdwintype()`. |
| CMD-06 | Prompt functions and raw input | P1 | M | CMD-01, MSG-01 | `input()`/`inputlist()`/`inputsecret()`/`confirm()`, `getchar()`/`getcharstr()`/`getcharmod()`, mouse in prompts, cancel/timeout return values. |
| CMD-07 | Search prompt and incremental search | P1 | M | CMD-01, REGEX-01 | `'incsearch'`, `'hlsearch'`, search offsets, `<C-g>/<C-t>`, cancel restores view, `v:searchforward`, pattern delimiter and escaping. |

## MSG — Messages, errors and prompts

Gate: `:echo`/`:echomsg`/`:echoerr`/`:redir`/`execute()` behave like Vim in
headless tests; hit-enter does not deadlock scripts; errors carry E codes.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| MSG-01 | Message pipeline | P0 | M | UI-01, FND-04 | `:echo`/`:echon`/`:echomsg`/`:echoerr`/`:echowindow`, message kinds and highlight groups, `v:statusmsg`/`v:errmsg`, expression-list echo, no message truncation of data. |
| MSG-02 | Hit-enter and message history | P1 | M | MSG-01 | `'more'`, hit-enter prompt, `:messages` window, message ring buffer, `'cmdheight'`=0 behaviour and scrollback. |
| MSG-03 | Capturing output | P1 | M | MSG-01, EX-01 | `:redir` to variable/file/register, `execute()` capture, `:silent`/`:silent!`/`:unsilent`, `:filter`, ordering with nested commands. |
| MSG-04 | Error formatting and throwpoints | P1 | M | MSG-01, FND-02 | E-code registry, `v:errmsg`/`v:exception`/`v:throwpoint`, source/line context, `:verbose` suffix, translation lookup, `:silent!` suppression. |
| MSG-05 | Intro, version and help output | P1 | S | MSG-01 | `:intro`, `:version`, `--version`, feature matrix, startup screen, `'shortmess'` flags, `:help`-less usage. |
| MSG-06 | Dialogs and waits | P2 | S | MSG-01, CMD-06 | `:confirm`, `:browse` fallback, `:sleep` with interrupt, `confirm()` choices, dialog text without GUI dependency. |

## EDT — Editing core

Gate: a differential key-sequence corpus (Normal/Visual/Operator-pending)
produces the same buffer text, registers and cursor as Vim 9.2 for the
supported feature set.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| EDT-01 | Normal-mode dispatch and counts | P0 | L | FND-03 | Mode stack, counts, register prefix, operator-pending state, `v:count`/`v:count1`, ambiguous key resolution, error/beep policy, `<Esc>` cancellation. |
| EDT-02 | Basic cursor motions | P0 | M | EDT-01, BUF-01 | `h j k l`, arrows, `<Space>`, `<BS>`, `+ - _ <CR>`, cursor clamping, counts, `'startofline'` interplay. |
| EDT-03 | Word and WORD motions | P1 | M | EDT-02, ENC-04 | `w W b B e E ge gE` with `'iskeyword'`, counts, end-of-line crossing rules, multibyte correctness. |
| EDT-04 | Line and column motions | P1 | S | EDT-02 | `0 ^ $ g_`, bar motion, `Home/End`, `g0/g^/g$/gm`, `gj/gk`, `'startofline'`, virtual columns and `curswant`. |
| EDT-05 | Paragraph, sentence, section and bracket motions | P1 | M | EDT-02, ENC-04 | `{ } ( ) [[ ]] [] ][ [{ ]} [( ])`, `%` and `'matchpairs'`, `'paragraphs'`/`'sections'`, comments and preprocessor blocks. |
| EDT-06 | Character search motions | P1 | S | EDT-02 | `f F t T ; ,`, counts, `'ignorecase'`? (char search is exact), repeat direction, multibyte character boundaries. |
| EDT-07 | Search motions | P1 | L | EDT-02, REGEX-01, CMD-07 | `/ ? n N * # g* g#`, `gn/gN`, `'wrapscan'`/`'ignorecase'`/`'smartcase'`, `'incsearch'`/`'hlsearch'`, search offsets, `v:searchforward`. |
| EDT-08 | Marks, jumps and change positions | P1 | M | EDT-02, BUF-07 | Apostrophe and backtick mark motions, `[ ] < >` marks, `<C-o>/<C-i>`, `g;`/`g,`, `:jumps`/`:changes`/`:marks`, jump vs change list semantics. |
| EDT-09 | Screen motions and scrolling keys | P1 | M | EDT-02, WIN-02 | `H M L`, `<C-e>/<C-y>`, `<C-d>/<C-u>`, `<C-f>/<C-b>`, `zz/zt/zb/z./z-/z+`, `'scroll'` and cursor keep. |
| EDT-10 | Operator engine | P1 | L | EDT-01, UND-01 | `d c y < > = ! gq gw g~ gu gU g? zf g@`, motion/text-object composition, counts, linewise/charwise/blockwise, register selection, abort/undo block. |
| EDT-11 | Text objects | P1 | L | EDT-10, ENC-04 | `iw aw iW aW is as ip ap i( a( i[ a[ i{ a{ i< a< it at i` `a` `i'` `a'` `i"` `a"`, `ib/ab/iB/aB`, nesting, whitespace rules, block virtual edit. |
| EDT-12 | Registers, yank and put | P1 | L | EDT-10, UI-10 | Unnamed, `0-9`, `a-z/A-Z`, `-`, `"`, `.`, `:`, `%`, `#`, `=`, `_`; `p/P/gp/gP/]p/[p`, `:put`/`:yank`, `getreg()`/`setreg()`/`getreginfo()`, `TextPutPre`/`TextPutPost` with `v:event`, `'clipboard'`. |
| EDT-13 | Join and split lines | P1 | S | EDT-10, INS-01 | `J`/`gJ`, `:join` with ranges, `'joinspaces'`, insert-mode newline, line splitting at cursor with indent preservation. |
| EDT-14 | Replace and virtual replace | P1 | M | EDT-01 | `R`, `gR`, replace stack/backspace restore, `'revins'` deferred with RTL, virtual replace at tab/wide-char boundaries. |
| EDT-15 | Visual modes | P1 | L | EDT-01, WIN-02 | `v/V/<C-v>`, `o/O` swap ends, `gv`, `:visual`, `'selection'`, blockwise virtual corners, visual marks `'<`/`'>`, mode messages. |
| EDT-16 | Select mode and mouse selection | P2 | M | EDT-15, UI-04 | `'selectmode'`, `'keymodel'`, select-mode entry keys, mouse drag, shift-selection, typing replaces selection. |
| EDT-17 | Dot repeat and redo register | P1 | M | EDT-10 | `.` with counts, insert replay, operator replay, `".` register, `'report'`, changes that do not set dot, undo interaction. |
| EDT-18 | Macros and `:normal` | P1 | M | EDT-01, EX-06 | `q{reg}`, `@{reg}`/`@@`, counts, recursive macro abort, `:normal[!]` over ranges, register pollution rules, `q:` integration. |
| EDT-19 | Increment and decrement | P1 | S | EDT-02 | `<C-a>/<C-x>`, `g<C-a>/g<C-x>` in Visual mode, `'nrformats'` (binary/octal/hex/alpha), leading zeros, overflow. |
| EDT-20 | Case, shift, format and indent operators | P1 | L | EDT-10, INS-07 | `~ g~ gu gU g?`, `>> <<`, `=`/`==`, `gq/gw`, `'formatoptions'`, `'textwidth'`, `'shiftwidth'`/`'expandtab'`/`'shiftround'`. |
| EDT-21 | Virtual editing and wrap keys | P2 | M | EDT-02, OPT-03 | `'virtualedit'` (all/block/insert/onemore), `'whichwrap'`, `'keymodel'`, `'selection'`, cursor beyond EOL, tab/wide-char columns. |
| EDT-22 | Change list | P2 | S | EDT-08, BUF-07 | `g;`/`g,`, `:changes`, change number preservation across edits, `getchangelist()`. |
| EDT-23 | Paste mode and bracketed paste | P1 | S | EDT-01, UI-06 | `'paste'`, `:set paste`, bracket-paste insertion without mappings/abbreviations/autoindent, paste register behaviour. |
| EDT-24 | Custom operator `g@` | P2 | M | EDT-10, VIML-16 | `g@{motion}`, `'operatorfunc'`, `'[`/`']` marks, visual `g@`, errors and return to Normal mode. |
| EDT-25 | Count display `g<C-g>` | P2 | S | EDT-15 | Selected lines/words/chars/bytes, visual block dimensions, Vim-compatible message. |

## INS — Insert and replace modes

Gate: insert/edit key corpus matches Vim; indentation and completion hooks use
the same option callbacks as Vim; abbreviation expansion is mode-correct.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| INS-01 | Insert, append and open-line | P0 | M | EDT-01 | `i I a A o O gi gI`, insert position restoration, counts, `'insertmode'` basics, mode message, undo block boundaries. |
| INS-02 | Insert-mode special keys | P1 | M | INS-01 | `<BS> <C-h> <C-w> <C-u> <C-t> <C-d> <C-e> <C-y> <C-r> <C-o> <C-v> <C-k> <C-@> <C-]>`, `'backspace'`, re-indent keys in `'cinkeys'`/`'indentkeys'`. |
| INS-03 | Digraphs | P1 | M | INS-01, ENC-01 | `<C-k>`, `'digraph'`, `:digraphs`, RFC 1345 table, `digraph_get()`/`digraph_set()`, insert and cmdline use. |
| INS-04 | Abbreviations | P1 | M | INS-01, EX-01 | `:abbreviate`/`:iabbrev`/`:cabbrev`, `<buffer>`, `<expr>`, prefixes `i`/`c`/`!`, `'paste'` bypass, expansion before/after cursor, `<C-]>`. |
| INS-05 | Insert completion sources | P2 | L | INS-01, CMD-03, TAG-01 | `<C-n>/<C-p>`, `'complete'` (`.`, `w`, `b`, `u`, `k`, `kspell`, `s`, `i`, `d`, `t`, `]`), `'completeopt'`, `'completefunc'`, `'omnifunc'`, `'thesaurusfunc'`, `'tagfunc'`, dictionary/thesaurus. |
| INS-06 | Completion popup and selection | P2 | L | INS-05, CMD-04 | Popup menu items, info preview, `<C-n>/<C-p>/<C-y>/<C-e>`, `'pumheight'`/`'pumwidth'`/`'pumblend'` (stored), `CompleteDone`, `v:completed_item`, `complete_info()`. |
| INS-07 | Indentation engines | P1 | XL | INS-01, REGEX-01 | `'autoindent'`, `'smartindent'`, `'cindent'` with `'cinoptions'`/`'cinkeys'`, `'lisp'`, `indent()`, `cindent()`, `shiftwidth()`; expression indentation is INS-12. |
| INS-12 | Expression indentation | P2 | M | INS-07, VIML-16 | `'indentexpr'`/`'indentkeys'` callbacks, `'formatexpr'` interaction, error containment, per-line caching, Vim-compatible `v:lnum`. |
| INS-08 | Format options and automatic wrapping | P2 | L | INS-01, OPT-03 | `'formatoptions'` (t c r o q a n 2 v b j), `'textwidth'`, `'wrapmargin'`, comment leaders `'comments'`/`'commentstring'`, `'formatlistpat'`. |
| INS-09 | Insert-mode mappings and paste | P1 | M | MAP-01, INS-01 | `:imap` family, `<buffer>`/`<expr>`/`<nowait>`, `'paste'`, bracket paste, `CTRL-O` one-shot Normal command, mapping failure/beep. |
| INS-10 | Start/stop insert and one-shot commands | P1 | S | INS-01 | `:startinsert`/`:stopinsert`, `gi`, `<C-o>`, insert-mode `<C-r>` register insertion, returning to Insert after command. |
| INS-11 | Repeat of insert changes | P1 | M | EDT-17, INS-01 | Dot-repeat of inserted text, `<C-a>` in insert (last inserted text), undo block grouping, replace-mode repeat. |

## VIS — Visual and Select modes

Gate: character/line/block operations and block insert/append match Vim;
visual marks and `gv` survive buffer/window changes.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| VIS-01 | Visual selection model | P1 | L | EDT-01 | Char/line/block modes, anchor and cursor, `o/O`, `gv`, `:visual`, per-window visual state, reselect after operator, `'selection'`. |
| VIS-02 | Visual operators and block insert | P1 | L | VIS-01, EDT-10 | `d c y < > = ! gq gw g~ gu gU g? J p P r~ u U`, block `I A c $`, blockwise put, virtual block corners, `'virtualedit'`. |
| VIS-03 | Visual marks and ex integration | P1 | M | VIS-01, EX-01 | `'<`/`'>`, `:'<,'>` ranges, visual `:normal`/`:!`/`:vimgrep`, `gv` after command, `'<`/`'>` update rules. |
| VIS-04 | Select mode | P2 | M | VIS-01, UI-04 | `'selectmode'`, `'keymodel'`, typing replaces selection, mouse drag, shift-arrows, select-mode entry keys, mode messages. |
| VIS-05 | Visual search and counts | P2 | S | VIS-01, EDT-07 | `*`/`#` in Visual mode, `gn/gN`, `g<C-g>` counts, `v:count`, selection-aware replace. |


## EX — Ex command engine

Gate: ranges, addresses, modifiers, `:global`, `:substitute`, `:normal` and
user commands match Vim on a command corpus; nested commands preserve
errors/redir/autocmd semantics.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| EX-01 | Ex parser, ranges and addresses | P0 | L | FND-04 | Command abbreviation resolution, ranges (`.`, `$`, `%`, marks, search offsets, `;`), counts, bang/register/args, comments, command separators, modifier prefixes. |
| EX-02 | Command dispatch and user commands | P1 | L | EX-01 | Built-in command table, `:command` attributes (`-nargs`, `-range`, `-count`, `-register`, `-bang`, `-bar`, `-buffer`, `-complete`, `-addr`, `-keepscript`), replacement placeholders `<args>`/`<q-args>`/`<f-args>`/`<line1>`/`<line2>`/`<count>`/`<reg>`/`<bang>`/`<mods>`. |
| EX-03 | `:global` and `:vglobal` | P1 | M | EX-01, REGEX-01 | Mark matching lines, nested command execution, `:g/pat/normal ...`, delete/append interaction, error continuation, recursion limits. |
| EX-04 | `:substitute` | P1 | L | EX-01, REGEX-01 | `:s`/`:sno`/`:smagic`/`:snomagic`, flags `g c e i I n p # l r`, count, `&`/`:&&`/`g&`, replacement `\0-\9`, `~`, `\u \U \l \L \e \E`, `\=` expression, multiline. |
| EX-05 | Line and range commands | P1 | M | EX-01, EDT-12 | `:delete`/`:yank`/`:put`/`:join`/`:move`/`:copy`/`:t`/`:sort`/`:retab`/`:left`/`:right`/`:center`/`:z`/`:print`/`:number`/`:list`/`:undolist`. |
| EX-06 | `:normal` and `:execute` | P1 | M | EX-01, EDT-01 | `:normal[!]` with ranges and counts, `:execute` with expression lists and nested commands, `:eval`, error propagation and abort rules. |
| EX-07 | Multi-file and multi-window iteration | P2 | L | EX-02, BUF-10, WIN-04 | `:argdo`/`:bufdo`/`:windo`/`:tabdo`/`:cdo`/`:cfdo`/`:ldo`/`:lfdo`, abort on error, buffer/window switching and modified-buffer policy. |
| EX-08 | Shell and filter integration | P1 | L | FND-07, BUF-03 | `:!`, `:r !`, `:w !`, `:!!`, `:shell`, filter ranges, `'shell'`/`'shellcmdflag'`/`'shellquote'`/`'shellredir'`/`'shellxquote'`/`'shelltemp'`, exit status `v:shell_error`. |
| EX-09 | Command modifiers | P1 | M | EX-01 | `:silent`/`:silent!`/`:unsilent`, `:verbose`, `:noautocmd`, `:keepalt`, `:keepjumps`, `:keepmarks`, `:lockmarks`, `:keeppatterns`, `:sandbox`, `:tab`, `:vertical`, `:aboveleft`/`:belowright`/`:leftabove`/`:rightbelow`/`:topleft`/`:botright`, `:hide`, `:noswapfile`, `:confirm`, `:filter`. |
| EX-10 | Listing and introspection commands | P1 | M | EX-02 | `:ls`/`:buffers`/`:files`/`:args`/`:marks`/`:registers`/`:jumps`/`:changes`/`:map`/`:abbreviate`/`:autocmd`/`:highlight`/`:syntax`/`:scriptnames`/`:messages`/`:options`/`:command`/`:function`/`:set` with Vim-compatible text and redir capture. |
| EX-11 | Build, grep and cscope bridges | P2 | M | QF-05 | `:make`/`:grep`/`:vimgrep`/`:vimgrepadd`/`:cscope`/`:lmake`/`:lgrep`/`:lvimgrep`, `'makeprg'`/`'grepprg'`/`'errorformat'`, `:compiler` loading. |
| EX-12 | Menus | P3 | M | UI-04 | `:menu` family, `:emenu`, `:popup` menus? (popup is separate), priority/accelerators, mode-specific menus, `'wildmenu'` interaction, `menu_info()`. |
| EX-13 | Platform-specific Ex quirks | P4 | S | EX-01 | `:behave`, `:simalt`, `:winpos`/`:winsize`, obsolete commands kept as clear errors; documented per platform. |

## MAP — Mappings and keys

Gate: mapping expansion, timeouts, `<expr>` and `<Plug>` resolution match Vim
for a key corpus; no mapping fires inside paste or while `'paste'` is set.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| MAP-01 | Mapping command matrix | P1 | L | EDT-01, CMD-01 | `:map`/`:nmap`/`:vmap`/`:xmap`/`:smap`/`:omap`/`:imap`/`:lmap`/`:cmap`/`:tmap`, `!` variants, `:noremap`/`:unmap`/`:mapclear`, mode-specific tables. |
| MAP-02 | Key notation and special keys | P1 | L | MAP-01 | `<C-...>`, `<S-...>`, `<M-...>`, `<D-...>`, `<F1>`-`<F12>`, `<Plug>`, `<SID>`, `<script>`, `<expr>`, `<unique>`, `<buffer>`, `<nowait>`, `<silent>`, `<special>`, key code normalization. |
| MAP-03 | Mapping expansion and timeouts | P1 | L | MAP-01, OPT-03 | Recursive vs non-recursive expansion, `'maxmapdepth'`, `'timeout'`/`'ttimeout'`/`'timeoutlen'`/`'ttimeoutlen'`, ambiguous prefix handling, remap stack and error messages. |
| MAP-04 | Expression and command mappings | P2 | M | MAP-03, VIML-05 | `<expr>` return-value parsing, `<cmd>`/`<ScriptCmd>` execution, `v:char`/`v:count` during expansion, errors and abort. |
| MAP-05 | Mapping introspection | P2 | M | MAP-01, VIML-14 | `maparg()`/`mapcheck()`/`mapset()`/`maplist()`/`hasmapto()`, `:map` listing, mode flags, script-local restoration. |
| MAP-06 | Language mappings | P3 | M | MAP-01, ENC-01 | `'langmap'`, `:lmap`, `'iminsert'`/`'imsearch'`, keymap interplay, command-line and insert mode. |
| MAP-07 | Cmdline mappings | P2 | M | MAP-01, CMD-01 | `:cmap`, cmdline mapping expansion, `'cedit'` interaction, paste bypass, timeout and cancel semantics. |
| MAP-08 | TTY key-code bridge | P2 | M | OPT-07, UI-06 | `t_*` key options, `:set termcap`, terminfo capabilities to Vim key notation, unknown sequence fallback and logging. |

## AUC — Autocommands

Gate: all documented events fire at the right time with the right `<amatch>`,
`<afile>` and `v:event`; `:autocmd` listing is parseable and round-trips;
`++once`/`++nested` and buffer-local groups are correct.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| AUC-01 | Event registry and firing | P1 | L | FND-04 | Event definitions and ordering, per-event pattern rules, `v:event` payloads, fast/slow event policy, recursion guard; includes `TextPutPre`/`TextPutPost` and `SessionLoadPre`. |
| AUC-02 | `:autocmd` and `:augroup` | P1 | L | AUC-01, EX-02 | Add/remove/list, groups, `au!`, `++once`, `++nested`, `<buffer=N>`, event/pattern completion, `:doautocmd`/`:doautoall`. |
| AUC-03 | Pattern matching and match variables | P1 | M | AUC-02 | `*`/`?`/`[...]` patterns, comma-separated patterns, `<amatch>`/`<afile>`/`<abuf>`/`<aevent>`, `expand('<amatch>')`. |
| AUC-04 | Execution control | P1 | M | AUC-02 | `'eventignore'`/`'eventignorewin'`, `:noautocmd`, nested-event suppression, errors inside autocommands, abort/continue semantics. |
| AUC-05 | Event execution and error handling | P1 | L | AUC-02, VIML-13 | Script context (`<sfile>`/`<stack>`), exceptions, hit-enter avoidance, `v:event` lifetime, reentrancy and `:silent!`. |
| AUC-06 | Buffer-local autocommands | P1 | M | AUC-02, BUF-02 | `<buffer>` registration, automatic cleanup on wipe, buffer pattern matching, `autocmd_get()` filtering. |
| AUC-07 | Filetype, syntax and option event wiring | P1 | M | AUC-01, RT-05 | `BufRead/BufNewFile/FileType/Syntax/OptionSet` integration, `:filetype`/`:syntax` loading hooks, secure event ordering. |
| AUC-08 | Autocommand scripting API | P2 | M | AUC-02, VIML-14 | `autocmd_add()`/`autocmd_delete()`/`autocmd_get()`, group creation, event dictionaries, round-trip with `:autocmd` text. |

## VIML — Legacy Vim script

Gate: legacy Vim script functions and expressions in the runtime corpus
evaluate with Vim-compatible values, scopes, errors and side effects. Vim9
syntax is rejected with a clear, documented gate.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| VIML-01 | Value model and garbage collector | P1 | XL | FND-05 | Number (64-bit), Float, byte String (NUL-safe), List, Dict, Blob, Funcref, Partial, Bool, Special, Channel/Job; mark-sweep heap with cycles, roots and deterministic collection points. |
| VIML-02 | Expression and script parser | P1 | XL | VIML-01 | Pratt/recursive-descent expression grammar, `:let`/`:call`/`:execute` forms, function bodies, line continuation, comments, heredoc, string escapes, error recovery with line numbers. |
| VIML-03 | Variables and scopes | P1 | L | VIML-02 | `g: b: w: t: s: l: a: v:`, `:let`/`:const`/`:unlet`/`:unlet!`, `:lockvar`/`:unlockvar`, environment `$VAR`, options `&opt`, registers `@r`, destructuring. |
| VIML-04 | Function definitions and calls | P1 | L | VIML-02 | `:function`/`:endfunction`, `:call`, `:return`, `:delfunction`, default arguments, varargs `.../a:000/a:0/a:1`, dict functions, recursion, `'maxfuncdepth'`. |
| VIML-05 | Funcref, Partial, lambda and closures | P1 | L | VIML-04 | `function()`/`funcref()`, `Partial` with bound args/dict, `{x -> ...}`, closures over outer locals, `->` method chains, `call()`, garbage-collected function values. |
| VIML-06 | Control flow and exceptions | P1 | L | VIML-02 | `:if/:elseif/:else/:endif`, `:while/:for`, `:break/:continue`, `:try/:catch/:finally/:endtry/:throw`, `:finish`, exception patterns and cleanup ordering. |
| VIML-07 | Literals, collections and slicing | P1 | L | VIML-01 | Number formats (decimal/octal/hex/binary), Float, strings and escapes, List/Dict/Blob literals, indexing and slicing, negative indices, `v:null`/`v:none`/`v:true`/`v:false`. |
| VIML-08 | Operators, coercion and comparison | P1 | M | VIML-07 | Arithmetic, bitwise, boolean short-circuit, string/list/dict comparison, `==#`/`==?`/`is`/`isnot`/`=~`/`!~`, type coercion table, division by zero errors. |
| VIML-09 | Script-local scope and script identity | P1 | M | VIML-03, RT-04 | `s:` variables, `<SID>`, `<script>`, `:scriptnames`, `:scriptversion`, script IDs stable across reload, `getscriptinfo()`. |
| VIML-10 | Autoload functions | P1 | L | VIML-04, RT-02 | `foo#bar#baz()` naming, lazy sourcing of `autoload/foo/bar.vim`, `#` escaping, load-once semantics, error attribution to the loaded script. |
| VIML-11 | `:execute`, `eval()` and capture | P1 | M | VIML-02, MSG-03 | `:execute` with expression lists, `eval()`, `:redir` integration, nested command strings, newline handling, evaluation errors and abort. |
| VIML-12 | Exceptions and throwpoints | P1 | M | VIML-06 | `v:exception`, `v:throwpoint`, `:throw`, `:catch /pat/`, rethrow, error conversion, `try` around Ex commands, cleanup with `:finally`. |
| VIML-13 | Predefined `v:` variables | P1 | M | VIML-03 | `v:count/count1/register`, `v:lnum`, `v:val/key`, `v:char`, `v:completed_item`, `v:event`, `v:option_*`, `v:errmsg/statusmsg/warningmsg`, `v:version/versionlong`, `v:vim_did_enter`, `v:argv`, `v:shell_error`, `v:termresponse`, `v:imagebackend`, `v:testing`, `v:exiting`, type constants. |
| VIML-14 | Built-in function framework | P2 | XL | VIML-02 | Arity/type metadata, pure vs context functions, lazy arguments, overloads, `exists('*f')`, `has()`, dispatch table, error messages and doc links; pure function families implemented here. |
| VIML-15 | Editor-context built-in functions | P2 | XL | VIML-14 | Buffer/window/tab/option/cursor/mark/search/syntax/fold/sign/textprop/quickfix/tag functions; borrowed editor context with reentrancy checks, no dangling references. |
| VIML-16 | Option and callback functions | P2 | L | VIML-05, OPT-03 | `'statusline'`/`'tabline'`/`'foldtext'`/`'indentexpr'`/`'omnifunc'`/`'completefunc'`/`'tagfunc'`/`'formatexpr'`/`'includeexpr'`/`'balloonexpr'` callbacks, error containment and caching. |
| VIML-17 | Profiling and debugging | P3 | L | VIML-04, MSG-02 | `:profile`/`:profdel`/`profile_func()`/`profile_file()`, `:breakadd`/`:breakdel`/`:breaklist`/`:debug`, `'debug'` option, source breakpoints. |
| VIML-18 | Script sourcing semantics | P1 | M | VIML-02, RT-04 | `:source`, line continuation rules, `:finish`, `:scriptencoding`, `:scriptversion`, modeline-free script header, nesting and error reporting. |

## FUN — Built-in function families

Gate: the function inventory is implemented or explicitly gated; `exists('*f')`
and `has()` are honest; every family has a Vim differential corpus.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| FUN-01 | String, byte and character functions | P2 | L | VIML-14 | `strlen strchars strwidth strdisplaywidth strpart strcharpart strgetchar str2list list2str char2nr nr2char strtrans tr escape shellescape fnameescape printf split join tolower toupper` and friends. |
| FUN-02 | Number, math and float functions | P2 | M | VIML-14 | `abs ceil floor round trunc fmod pow sqrt log log10 exp sin cos tan asin acos atan atan2 sinh cosh tanh isinf isnan float2nr str2float rand srand and or xor invert`. |
| FUN-03 | List, dict, blob and functional helpers | P2 | L | VIML-14 | `add insert remove extend extendnew copy deepcopy map mapnew filter reduce forEach sort uniq reverse range repeat slice flatten flattennew index count get has_key empty len keys values items`, Blob conversion. |
| FUN-04 | Type, reflection and existence | P2 | M | VIML-14 | `type()`, `typename()`, `exists()`, `has()`, `has_key()`, `get()`, `empty()`, `v:t_*` constants, gated `test_null_*`. |
| FUN-05 | Buffer, window and tab information | P2 | L | VIML-15, WIN-07 | `bufnr bufname bufloaded buflisted bufexists getbufinfo getbufoneline getbufline setbufline appendbufline deletebufline winbufnr winnr win_getid winlayout tabpagewinnr getwininfo gettabinfo`. |
| FUN-06 | Cursor, position, marks and jumps | P2 | M | VIML-15, BUF-07 | `cursor getcurpos setpos getpos getcursorcharpos setcursorcharpos getcharpos setcharpos virtcol virtcol2col charcol charidx byteidx byteidxcomp line col wincol winline getmarklist getjumplist getchangelist`. |
| FUN-07 | Search, regex and substitution | P2 | L | VIML-15, REGEX-01 | `search searchpos searchpair searchpairpos searchcount match matchend matchlist matchstr matchstrpos substitute matchfuzzy matchfuzzypos glob2regpat`. |
| FUN-08 | Files, paths, system, environment and time | P2 | L | VIML-14, FND-07 | `fnamemodify expand glob globpath readdir readdirex getcwd chdir mkdir delete rename copy getftype getfsize getftime filereadable filewritable isdirectory resolve simplify tempname writefile readfile readblob writeblob system systemlist executable exepath getenv setenv environ hostname localtime strftime strptime reltime reltimestr reltimefloat`. |
| FUN-09 | Serialization, encoding and hashing | P2 | M | VIML-14 | `json_encode json_decode base64_encode base64_decode sha256 blob2list list2blob blob2str str2blob str2list list2str strutf16len utf16idx`. |
| FUN-10 | Input, UI, screen and completion | P2 | L | VIML-15, CMD-06 | `input inputlist inputsecret confirm getchar getcharstr getcharmod getmousepos screenchar screenchars screenstring screenpos screenattr screencol screenrow complete complete_add complete_check complete_info pumvisible wildmenumode state mode visualmode getcmdline getcmdpos setcmdline setcmdpos getcmdtype getcmdcompltype getcmdscreenpos getbgcolor preinserted`. |
| FUN-11 | Syntax, highlight, folds, signs and properties | P2 | L | VIML-15, SYN-09 | `synID synIDattr synIDtrans synstack hlID hlexists hlget hlset diff_hlID foldclosed foldclosedend foldlevel foldtext foldtextresult sign_define sign_place sign_unplace sign_getplaced sign_getdefined sign_jump prop_type_add prop_type_change prop_type_delete prop_type_get prop_add prop_add_list prop_remove prop_list prop_find`. |
| FUN-12 | Quickfix, tags and help | P2 | M | VIML-15, QF-04 | `getqflist setqflist getloclist setloclist taglist tagfiles gettagstack settagstack getcompletion('help')`, help tag lookup. |
| FUN-13 | Jobs, channels and timers | P3 | L | VIML-15, JOB-04 | `job_start job_stop job_status job_info job_getchannel ch_open ch_listen ch_close ch_sendraw ch_sendexpr ch_evalexpr ch_evalraw ch_read ch_readraw ch_readblob ch_status ch_info ch_getbufnr ch_getjob ch_log timer_start timer_stop timer_pause timer_info`. |
| FUN-14 | Spell and sound | P3 | M | VIML-14, SPELL-06 | `spellbadword spellsuggest soundfold sound_playfile sound_clear`, feature-gated stubs otherwise. |
| FUN-15 | Popup, menu and balloon | P3 | M | VIML-15, DSP-07 | `popup_create popup_atcursor popup_menu popup_notification popup_dialog popup_close popup_hide popup_show popup_move popup_setoptions popup_settext popup_getpos popup_getoptions popup_clear popup_list popup_findinfo popup_findpreview popup_filter_menu popup_filter_yesno menu_info balloon_show balloon_gettext`. |
| FUN-16 | Testing, assertions and profiling | P2 | M | VIML-14 | `assert_equal assert_notequal assert_true assert_false assert_match assert_notmatch assert_inrange assert_fails assert_exception assert_beeps assert_nobeep assert_report test_* profile_*`, `v:errors`. |
| FUN-17 | Meta-call functions | P2 | M | VIML-11 | `call()`, `function()`, `funcref()`, `eval()`, `execute()`, `win_execute()`, argument evaluation order, errors and return coercion. |
| FUN-18 | Listeners and introspection helpers | P2 | S | FUN-05 | `listener_add listener_remove getscriptinfo getinfo getcellwidths setcellwidths getpid version`, `has()` edge cases. |

## REGEX — Vim regular expressions

Gate: Vim's regex test corpus and syntax/search samples produce identical match
spans for the supported atoms; pathological patterns are interruptible and
bounded; byte offsets match Vim's `col()` semantics.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| REGEX-01 | Parser, AST and magic modes | P1 | XL | FND-02 | `\v \m \M \V` magic switching, atoms, groups `\%(`, alternation, quantifiers `* \+ \= \{n,m} \{-}`, character classes, escapes, delimiter handling, syntax errors with positions. |
| REGEX-02 | Backtracking VM and captures | P1 | XL | REGEX-01 | Byte-oriented matching with capture groups `\1-\9`, greedy/lazy quantifiers, backreferences, leftmost-first semantics, recursion/step limits, match spans. |
| REGEX-03 | Lookaround and boundary atoms | P1 | L | REGEX-02 | `\@= \@! \@<= \@<! \@>`, `\zs \ze`, `\%^ \%$ \%# \%V \%'m \%l \%c`, word-boundary atoms, zero-width repetition rules. |
| REGEX-04 | Multiline and buffer-context matching | P2 | L | REGEX-02, BUF-01 | `\_` line atoms, `\_^ \_$`, `\n`, `\%V` visual, cursor atom, mark atom, line/column atoms, bounded context and performance. |
| REGEX-05 | Character classes and case folding | P1 | L | REGEX-01, ENC-04 | `\i \I \k \K \f \F \p \P \s \S \d \D \w \W \a \A \l \L \u \U \x \X \o \O \h \H`, POSIX classes, `[[=a=]]` equivalence, `\c \C`, `'ignorecase'`/`'smartcase'`. |
| REGEX-06 | Engine integration | P1 | L | REGEX-02 | Search (`/ ? n N * #`), `:substitute`, `:global`, `match()` family, syntax engine, filetype patterns, quickfix errorformat, tag search; shared compiled-pattern cache. |
| REGEX-07 | NFA/PikeVM acceleration | P2 | L | REGEX-02 | Literal prefix scan, first-byte bitmap, NFA simulation for patterns without backrefs/lookaround, `'regexpengine'` selection, fallback to backtracking. |
| REGEX-08 | Timeouts, interrupts and limits | P1 | M | REGEX-02, FND-03 | Step budget, `got_int` checks, `'redrawtime'`/`'matchtime'`, backtracking limit errors, resumable matching for huge files. |
| REGEX-09 | Differential regex corpus | P2 | L | REGEX-02, TST-02 | Extract patterns from Vim docs/runtime/syntax and compare match spans against Vim 9.2; fuzz pattern parser and VM. |


## SYN — Syntax and highlighting

Gate: every syntax file in the pinned runtime loads without error on a sample
buffer; highlight attributes and syntax items match Vim; incremental edits
invalidate only affected lines/regions.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| SYN-01 | Highlight table and default groups | P1 | M | UI-01 | Global/local highlight groups, default links, `Normal`/`Comment`/`Constant` families, `Popup`/`PopupBorder`/`PopupTitle`/`VertSplitNC`, `:hi clear`, `:highlight` with no args, group redefinition and `hlID()`. |
| SYN-02 | Highlight attributes, colors and links | P1 | L | SYN-01 | `cterm`/`gui` attributes, `ctermfg/bg`, `guifg/bg`, `guisp`, `font`, `blend` (stored), `NONE`, `:hi link`/`:hi def link`/`:hi!`, `'winhighlight'` window-local remapping, color names and `#RRGGBB`. |
| SYN-03 | Background and color resolution | P1 | M | SYN-02, UI-07 | `'background'` light/dark, `'termguicolors'`, `t_Co`, `v:colornames`, cterm nearest-color fallback, `ColorScheme` event ordering. |
| SYN-04 | Colorschemes | P1 | M | SYN-02, RT-01 | `:colorscheme`, `colors/*.vim` search order, `g:colors_name`, `:hi clear` then reload, errors in colorschemes, `after/colors`. |
| SYN-05 | Syntax engine core | P1 | XL | REGEX-06, BUF-01 | `:syn keyword/match/region/cluster/include`, start/end/skip patterns, priorities, keyword boundaries, case options, per-line state and highlight spans. |
| SYN-06 | Syntax containment and linking | P1 | XL | SYN-05 | `contains`/`containedin`/`contained`, `nextgroup`, `matchgroup`, `transparent`, `extend`/`keepend`, `oneline`, `excludenl`, offset modifiers `ms=`/`me=`/`hs=`/`he=`/`rs=`/`re=`. |
| SYN-07 | Syntax synchronization and invalidation | P2 | L | SYN-05 | `syn sync fromstart/minlines/maxlines/linebreaks`, ccomment, invalidation on edit/undo, region stack cache, bounded re-parse and incremental redraw. |
| SYN-08 | `:syntax` commands and Syntax event | P1 | M | SYN-05, AUC-07 | `:syntax on/off/enable/clear`, `'syntax'`, `Syntax` event, `syntax/syntax.vim` loading, `:runtime! syntax/<ft>.vim` order, `syntax_cmd` handling. |
| SYN-09 | Syntax and highlight introspection | P2 | M | SYN-05, VIML-15 | `synID()`/`synIDattr()`/`synIDtrans()`/`synstack()`, `hlget()`/`hlset()`, `diff_hlID()`, `synconcealed()`, stable IDs across reload. |
| SYN-10 | Concealment | P2 | M | SYN-05, DSP-08 | `conceal`/`concealends`/`cchar`, `'conceallevel'`/`'concealcursor'`, cursor-line reveal, wide cchar, interaction with wrapping and visual block. |
| SYN-11 | Match highlighting | P2 | M | SYN-01, REGEX-06 | `:match`/`:2match`/`:3match`, `matchadd()`/`matchaddpos()`/`matchdelete()`/`getmatches()`/`matcharg()`, window-local matches, priority and `'matchpairs'`/`'showmatch'`. |
| SYN-12 | Runtime syntax corpus compatibility | P2 | XL | SYN-06, RT-07 | Load all pinned `syntax/*.vim` files, apply to representative samples, compare highlight traces with Vim, track per-file pass/fail and regressions. |
| SYN-13 | Syntax profiling | P3 | S | SYN-07 | `:syntime on/off/report/clear`, per-pattern time/count, no overhead when off. |

## DSP — Display features

Gate: folds, signs, text properties, popups and diff render correctly in
headless frames; fold/sign/property APIs survive edits and undo; diff output
matches the internal diff oracle.

Implemented so far: DSP-01, DSP-02, DSP-03 (folding), DSP-04 (signs),
DSP-05 (text properties, including inline virtual text) and the two-way core of
DSP-11/12 (internal Myers diff, fillers, highlighting, diff folding,
:diffthis/:diffoff/:diffupdate/:diffsplit, do/dp and :diffget/:diffput,
[c/]c, diff_filler()/diff_hlID()); see [STATUS.md](STATUS.md) for the
implemented surface and the documented approximations. DSP-06 (virtual lines),
DSP-07 (popups), three-way merge/:diffpatch and external diffexpr remain P2
work.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| DSP-01 | Fold engine and methods | P2 | L | BUF-01, REGEX-06 | `'foldmethod'` manual/indent/expr/marker/syntax/diff, fold levels, `'foldignore'`/`'foldnestmax'`/`'foldminlines'`, fold text, per-window fold state. |
| DSP-02 | Fold commands and API | P2 | L | DSP-01, WIN-02 | `zo zc za zR zM zr zm zj zk`, `[z`/`]z`, `:foldopen/foldclose/folddo/foldd`, `foldclosed()`/`foldclosedend()`/`foldlevel()`/`foldtext()`/`foldtextresult()`, `'foldopen'`/`'foldclose'`/`'foldlevel'`/`'foldenable'`. |
| DSP-03 | Fold display and column | P2 | M | DSP-01, WIN-10 | `'foldcolumn'` (number/auto), fold markers, fold text rendering, click regions, interaction with signs and numbers, `'foldtext'` errors. |
| DSP-04 | Signs | P2 | L | BUF-01, SYN-01 | `:sign define/place/unplace/list/jump`, groups/priorities, `sign_place()`/`sign_unplace()`/`sign_getplaced()`/`sign_getdefined()`/`sign_jump()`, `'signcolumn'` auto/number, sign clicks, placement across edits/undo. |
| DSP-05 | Text properties | P2 | XL | BUF-01, SYN-01 | `prop_type_add/change/delete/get`, `prop_add()`/`prop_add_list()`/`prop_remove()`/`prop_list()`/`prop_find()`, `highlight`/`text`/`virt_text`/`start_incl`/`end_incl`/`priority`/`combine`/`repeat`, text alignment, property lifetime across edits. |
| DSP-06 | Virtual text and virtual lines | P2 | XL | DSP-05, WIN-10 | `virt_text`/`virt_text_pos` (inline/overlay/eol/right_align), `virt_lines`/`virt_lines_above`/`virt_lines_leftcol`, width/priority, cursor and mouse interactions, redraw cost control. |
| DSP-07 | Popup and floating windows | P2 | XL | WIN-01, UI-03 | `popup_create()`/`popup_atcursor()`/`popup_menu()`/`popup_notification()`/`popup_dialog()`/`popup_close()`/`popup_move()`/`popup_setoptions()`/`popup_settext()`/`popup_getpos()`, border/title/scrollbar/mask/zindex, filters and callbacks, `:popup` command; image attributes deferred to MISC-12. |
| DSP-08 | List chars, fill chars and break indent | P1 | M | WIN-10 | `'list'`/`'listchars'`, `'fillchars'`, `'showbreak'`, `'breakindent'`/`'breakindentopt'`, tab/space/trail/nbsp/eol/extends/precedes, `'display'` lastline/truncate/uhex. |
| DSP-09 | Wrapping and smooth scrolling | P1 | L | WIN-02 | `'wrap'`/`'nowrap'`, `'linebreak'`/`'breakat'`, `'sidescroll'`/`'sidescrolloff'`, `'smoothscroll'`, `gj/gk`/`g0/g$/gm`, wrapped-line cursor and mouse mapping, skipcol. |
| DSP-10 | Gutters and screen columns | P1 | M | WIN-10 | `'number'`/`'relativenumber'`/`'numberwidth'`/`'numberformat'`, `'colorcolumn'`, `'signcolumn'`, `'foldcolumn'`, `'cursorline'`/`'cursorlineopt'`, `'cursorcolumn'`, column width and click regions. |
| DSP-11 | Diff engine | P2 | XL | BUF-01, WIN-02, DSP-01 | `:diffthis`/`:diffoff`/`:diffupdate`, internal Myers diff vs external `'diffexpr'`, `'diffopt'`/`'diffanchors'`, diff blocks, filler lines, fold integration, `diff_filler()`/`diff_hlID()`. |
| DSP-12 | Diff commands and navigation | P2 | L | DSP-11, EX-01 | `:diffsplit`/`:diffpatch`/`:diffget`/`:diffput`/`:diffg`/`:diffp`, `[c`/`]c`, `do`/`dp`, `'diff'` per window, `:diffoff!`, three-way merge basics. |
| DSP-13 | Cursor line, column and conceal interaction | P1 | S | SYN-01, WIN-10 | `'cursorline'`/`'cursorcolumn'`/`'colorcolumn'` priority, conceal reveal on cursor line, cursor screen position with virtual text, no flicker. |
| DSP-14 | Balloon evaluation | P3 | M | UI-02, VIML-16 | `'ballooneval'`/`'balloonevalterm'`/`'balloonexpr'`/`'balloondelay'`, `balloon_show()`/`balloon_gettext()`, TUI fallback (message/echo) and GUI hook. |

## QF — Quickfix and location lists

Gate: `'errorformat'` parses compiler/grep corpora; quickfix and location list
stacks, navigation and window rendering match Vim; functions round-trip.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| QF-01 | Quickfix and location list model | P1 | M | FND-05 | Entries (filename, lnum, col, text, type, valid, nr, module, pattern), list stacks/IDs, titles, context, `'switchbuf'`, per-window location lists. |
| QF-02 | `'errorformat'` parser | P2 | L | QF-01, REGEX-06 | `%f %l %c %m %t %n %o %p %v %s`, prefixes `%+`/`%-`, multiline `%A/%C/%Z`, `%G` ignore, directory/file stacks, `'efm'` comma escaping. |
| QF-03 | Quickfix window and navigation | P2 | L | QF-01, BUF-09 | `:copen`/`:cclose`/`:cwindow`, `:cnext`/`:cprev`/`:cfirst`/`:clast`/`:cc`, `:lopen` family, error highlighting, `'quickfixtextfunc'`, `QuickFixCmdPre/Post`, window height. |
| QF-04 | Quickfix and location list functions | P2 | M | QF-01, VIML-15 | `getqflist()`/`setqflist()`/`getloclist()`/`setloclist()`, `:cexpr`/`:caddexpr`/`:cbuffer`/`:cgetexpr`/`:cgetbuffer`, IDs and titles. |
| QF-05 | Make, grep, vimgrep and cscope | P2 | L | QF-02, EX-08 | `:make`/`:grep`/`:vimgrep`/`:lmake`/`:lgrep`/`:lvimgrep`, `'makeprg'`/`'grepprg'`, `'shellpipe'`, synchronous execution first (JOB-05 adds async), `:cscope` gated by feature. |
| QF-06 | Multi-file quickfix iteration | P2 | M | QF-03, EX-07 | `:cdo`/`:cfdo`/`:ldo`/`:lfdo`, error abort, buffer loading policy, modified buffers, progress reporting. |
| QF-07 | Quickfix presentation and callbacks | P2 | M | QF-03, VIML-16 | `'quickfixtextfunc'`, qf syntax highlighting, `QuickFixLine`, cursor/statusline integration and quickfix event payloads. |
| QF-08 | Compiler plugin runtime | P2 | M | QF-05, RT-07 | `:compiler`/`:compiler!`, `compiler/*.vim` loading, `:make` integration, `b:current_compiler`, `after/compiler`. |

## TAG — Tags and help

Gate: Vim's shipped `doc/tags` resolves; tag stack navigation, preview and
`:help` work on the real runtime; `:helptags` produces Vim-compatible tags.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| TAG-01 | Tag file parser and search | P1 | M | FND-07 | `tags` format, sorted/binary search, duplicate tags, `'tags'`/`'tagrelative'`/`'taglength'`/`'showfulltag'`/`'tagsecure'` (reject URL file fields with E1576), Emacs tags optional, encoding and caching. |
| TAG-02 | Tag commands and stack | P1 | L | TAG-01, WIN-01 | `:tag`/`:tselect`/`:tjump`/`:tnext`/`:tprev`/`:tfirst`/`:tlast`/`:trewind`/`:pop`, `CTRL-]`/`CTRL-t`/`g]`, tag stack, preview hooks, tag pattern execution. |
| TAG-03 | Tag scripting APIs | P2 | M | TAG-01, VIML-15 | `taglist()`/`tagfiles()`/`gettagstack()`/`settagstack()`, tag completion, `'tagfunc'` integration metadata. |
| TAG-04 | Tag function and ctags integration | P2 | M | TAG-02, VIML-16 | `'tagfunc'` callback, `taglist()` fallback, ctags invocation guidance, async tags, error handling when tags missing. |
| TAG-05 | Help buffer and navigation | P1 | L | TAG-01, BUF-09 | `:help`/`:h`, `'buftype=help'`, `'iskeyword'`/`'modifiable'`, bar-delimited tag links, `CTRL-]`/`CTRL-t`/`CTRL-o`, `:helpclose`, `'helpheight'`. |
| TAG-06 | Helptags generation and doc parsing | P1 | M | TAG-05, FND-07 | `:helptags` over `doc/*.txt`, `*tag*` extraction, duplicate/error reporting, `doc/tags` format, `:helptags ALL`. |
| TAG-07 | Keyword lookup | P1 | S | TAG-02, EX-08 | `K`/`'keywordprg'`, `g]` tag select, `CTRL-W ]`/`CTRL-W }`, `:ptag`/`:ptselect`, man page fallback, shell quoting. |
| TAG-08 | Help grep and search | P2 | M | TAG-05, QF-05 | `:helpgrep`, help error list, `:lhelpgrep`, match context, jump to tag and close help. |

## SPELL — Spell checking

Gate: bundled `.spl` dictionaries load; bad-word detection and suggestions
match a Vim oracle on a corpus; `:mkspell` output is readable by Vim.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| SPELL-01 | Spell file reader | P3 | XL | FND-07 | Vim `.spl`/`.sug` binary formats (version 1/2), region/affix trees, word flags, case folding, memory-mapped loading, corruption diagnostics. |
| SPELL-02 | Affix and word-list tooling | P3 | XL | SPELL-01 | `.aff`/`.dic` parsing, `:mkspell` writer compatible with Vim, compression, `:spelldump`, incremental updates, `spell/*.vim` tooling. |
| SPELL-03 | Spell checker and suggestions | P3 | XL | SPELL-01, REGEX-06 | Bad-word detection, compound/region rules, suggestion generation/ranking, `'spellsuggest'`, `'spellcapcheck'`, `'spelloptions'`, performance bounds. |
| SPELL-04 | Spell commands and options | P3 | L | SPELL-03, OPT-03 | `:spellgood`/`:spellwrong`/`:spellrare`/`:spellundo`, `zg/zw/zG/zW`, `'spelllang'`/`'spellfile'`/`'spell'`, per-window spelling. |
| SPELL-05 | Spell display and navigation | P3 | M | SPELL-03, WIN-10 | `]s`/`[s`/`]S`/`[S`, `z=`/`1z=`, `SpellBad/SpellCap/SpellRare/SpellLocal` highlighting, insert-mode spell completion. |
| SPELL-06 | Spell scripting API | P3 | M | SPELL-03, VIML-14 | `spellbadword()`/`spellsuggest()`/`soundfold()`, region/case arguments, guest language dictionaries. |
| SPELL-07 | Spell in insert and syntax | P3 | M | SPELL-03, INS-05 | `'complete'` `kspell`, `:syn spell` options, spell checking in comments/strings, `'spell'` per buffer, performance under syntax. |
| SPELL-08 | Runtime spell corpus tests | P3 | L | SPELL-01, TST-02 | Load every bundled `spell/*.spl`, compare bad-word/suggestion output with Vim, test `:mkspell` round-trip and malformed files. |


## JOB — Jobs, channels and timers

Gate: asynchronous jobs never block the editor; callbacks run in the main
thread; channels speak raw/JSON/LSP modes, including the stdio channel; timers
fire on schedule and survive system suspend/resume.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| JOB-01 | Process spawn, pipes and PTYs | P3 | L | FND-07 | Nonblocking fork/exec, stdin/stdout/stderr pipes, PTY allocation, environment/cwd, process groups, reaping, zombie prevention, Windows abstraction. |
| JOB-02 | Job options and callbacks | P3 | L | JOB-01, VIML-05 | `job_start()` options (`in_io`, `out_io`, `err_io`, `out_mode`, `err_mode`, `pty`, `noblock`, `timeout`, `stoponexit`, `cwd`, `env`, `clear_env`, `waittime`, callback/close/exit/out/err), `job_status()`/`job_info()`/`job_stop()`/`job_setoptions()`/`job_getchannel()`. |
| JOB-03 | Channels | P3 | L | JOB-01, FND-03 | `ch_open()` raw/JSON/nl/lsp modes, `ch_listen()` server-side loopback/Unix channels with accept callback, `--stdio-channel` and the `stdio` address (channel startup, not an Ex-mode compatibility layer), TCP IPv4/IPv6 and Unix sockets, `ch_sendraw()`/`ch_sendexpr()`/`ch_evalexpr()`/`ch_evalraw()`/`ch_read()`/`ch_readraw()`/`ch_readblob()`/`ch_close()`/`ch_status()`/`ch_info()`/`ch_getbufnr()`/`ch_getjob()`, callbacks and timeouts, `ch_log()`. |
| JOB-04 | Timers | P2 | M | FND-03 | `timer_start()`/`timer_stop()`/`timer_pause()`/`timer_info()`, repeat counts, paused timers, timer callbacks and errors, `'updatetime'`/`:sleep` integration. |
| JOB-05 | Build and grep plumbing | P3 | M | JOB-01, EX-08, QF-05 | `:make`/`:grep` async execution, `'shellpipe'`, `QuickFixCmdPre/Post`, output capture, exit status, cancellation, no shell injection. |
| JOB-06 | Prompt buffers | P3 | M | JOB-03, BUF-09 | `prompt_setprompt()`/`prompt_setcallback()`/`prompt_setinterrupt()`/`prompt_getprompt()`, `'buftype=prompt'`, editable input, callback cancellation, channel wiring. |
| JOB-07 | Wait, signals and shutdown | P3 | M | JOB-02 | `wait()`, `job_stop()` signals, `:sleep` interruptibility, shutdown order (jobs, channels, PTYs), timeout and orphan prevention. |
| JOB-08 | Client-server and remote control | P4 | L | JOB-03 | `--servername`/`--remote*`, `serverlist()`/`remote_expr()`/`remote_send()`/`remote_peek()`, X11/socket transport, `+clientserver` honesty; optional platform feature. |

## JS — QuickJS integration

Gate: `:js` and `:jsfile` run ES2023 code; `vim.cmd`/`vim.eval`/`vim.fn` and
typed API calls work; JS functions are first-class Vim callbacks; promises,
timers and jobs interleave without blocking; limits and sandbox are enforced.

QuickJS is an RVim extension, not a Vim feature. It must not perturb Vim
script semantics or `has()` answers for Vim-only features.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| JS-01 | QuickJS engine embedding | P3 | L | FND-01 | quickjs-ng (or maintained fork) vendored build behind a `quickjs` Cargo feature, reproducible build, macOS/Linux targets, no runtime dependency on a system library. |
| JS-02 | Runtime lifecycle, limits and interrupts | P3 | M | JS-01, FND-03 | One runtime/context per editor, create/destroy ordering, memory limit, max stack, `setInterruptHandler` tied to `got_int`, no JS execution on background threads. |
| JS-03 | `:js`, `:jsdo` and `:jsfile` | P3 | M | JS-02, RT-04 | Evaluate expressions/statements, range command, file source with script IDs, errors with file/line, `:jsversion`, no implicit global leakage between files. |
| JS-04 | `vim` global API | P3 | L | JS-03, VIML-11 | `vim.cmd()` (string or array), `vim.eval()`, `vim.fn.name(...)` dynamic proxies, `vim.call()`, `vim.api.*` typed calls, `vim.g/b/w/t/v` proxies, `vim.version`, `vim.in_fast_event()`, `vim.notify()`. |
| JS-05 | Type conversion and function bridge | P3 | L | JS-02, VIML-01 | Number/BigInt bounds, string/NUL, Array/List, Object/Dict, Uint8Array/Blob, null/undefined/Special, JS function to `Funcref` persistent handle, Vim funcref to JS callable, rooting across both GCs, deep-copy boundary. |
| JS-06 | Module loader | P3 | L | JS-03, RT-01 | ES modules, `import` resolution from `$VIMRUNTIME/js/`, `<rtp>/js/`, relative paths and `pack/*/start/*/js/`, deterministic resolver, cache by resolved path, no `node_modules`. |
| JS-07 | JS plugins and autoload | P3 | M | JS-06, RT-03 | Source `plugin/**/*.js` in runtimepath order, `g:loaded_<name>` guard convention, `:jsfile` entry points and modules, `has('quickjs')`, load only when the feature is compiled in. |
| JS-08 | Promises, microtasks and event loop | P3 | L | JS-02, JOB-04 | Drain QuickJS job queue after callbacks/commands, resolve promises from timer/job callbacks, `vim.defer()` scheduling, no blocking `await` on the main thread, shutdown drains safely. |
| JS-09 | Errors, exceptions and console | P3 | M | JS-03, MSG-01 | JS throw to Vim exception (`v:exception`/`v:throwpoint`), Vim error to JS `Error` with E code, stack traces, `console.log/info/warn/error` to messages, `vim.notify()` levels. |
| JS-10 | VimL-to-JS bridge functions | P3 | M | JS-05, VIML-14 | `js_eval()`, `js_call()`, `js_function()`, `js_version()`, conversion errors, reentrancy guard, callbacks from `timer_start()`/`job_start()`. |
| JS-11 | Sandbox and restricted mode | P3 | M | JS-06, SEC-01 | No `std`/`os` modules by default, module allowlist, no direct filesystem/process access except through Vim APIs, `:sandbox`/`-Z` enforcement, memory/stack limits. |
| JS-12 | JS conformance and performance tests | P3 | M | JS-08, TST-01 | ES2023 subset tests (test262 selections), API contract tests, promise/job interleaving, memory-limit and interrupt tests, startup and call overhead benchmarks. |

## RT — Vim runtime compatibility

Gate: Tier A runtime loads cleanly; `:help` resolves shipped tags; filetype,
ftplugin, indent, syntax, colors and compiler files execute; compatibility is
measured per file and per tier, not asserted.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| RT-01 | `'runtimepath'` discovery and order | P1 | L | FND-07 | Unix/macOS/Windows defaults, `$VIMRUNTIME`/`$VIM`, user config dirs, `after/` last, `:set rtp` parsing, path normalization and escaping. |
| RT-02 | `:runtime` and `:runtime!` | P1 | M | RT-01 | Glob expansion, all-matches ordering, `after` semantics, missing-file errors, `:runtime` with `!`, source errors with script attribution. |
| RT-03 | Plugin and package loading | P2 | L | RT-02, EX-02 | `plugin/**/*.vim` ordering, `pack/*/start`, `pack/*/opt`, `:packadd`/`:packadd!`/`:packloadall`, `'packpath'`, load guards, `:scriptnames` visibility. |
| RT-04 | Sourcing and script identity | P1 | M | EX-01, FND-07 | `:source`, script IDs, `:scriptnames`, `<sfile>`/`<slnum>`/`<stack>`, `:finish`, nested source, error line mapping, BOM/shebang handling. |
| RT-05 | Filetype detection | P1 | L | RT-02 | `filetype.vim`/`scripts.vim` loading, `:setf`/`:setfiletype`, `'filetype'`, extension/pattern/shebang detection, `BufNewFile/BufRead` ordering, `:filetype on/off`. |
| RT-06 | ftplugin and indent loading | P2 | L | RT-05, VIML-04 | `ftplugin/<ft>.vim`/`ftplugin/<ft>_*.vim`, `indent/<ft>.vim`, `b:did_ftplugin`/`b:did_indent`, `b:undo_ftplugin`/`b:undo_indent`, `:filetype plugin indent on`. |
| RT-07 | Syntax, colors, compiler and keymap loading | P2 | L | RT-05, SYN-08 | `syntax/<ft>.vim` via `Syntax`, `colors/<name>.vim`, `compiler/<name>.vim`, `keymap/<name>.vim`, `after/` overrides, load ordering and failure isolation. |
| RT-08 | Defaults and startup modes | P1 | M | RT-01, OPT-05 | `defaults.vim`, `vimrc_example.vim`, `-u NONE`/`-u NORC`/`-u DEFAULTS`, `--clean`, `--noplugin`, `vimrc`/`gvimrc` order, `v:vim_did_enter` and `VimEnter`. |
| RT-09 | Runtime documentation and tags | P1 | M | RT-01, TAG-06 | `doc/tags`, `:help` on shipped docs, `'helplang'`, doc file encoding, `:helptags` regeneration, missing-tag diagnostics. |
| RT-10 | Runtime smoke harness | P2 | L | RT-03, TST-01, VIML-10 | Source plugins/autoloads in isolated processes, record errors/`v:errmsg`, classify expected GUI/Vim9 gaps, produce per-file report and regression diff. |
| RT-11 | Compatibility tiers and matrix | P1 | M | RT-01 | Tier A (core editing/help/syntax), Tier B (common plugins/languages), Tier C (everything), explicit gap list, CI dashboard and release gate. |
| RT-12 | Vim9 runtime skip policy | P2 | M | RT-02, EX-01 | Detect `vim9script` headers (including `noclear`), skip with a single verbose warning by default, `--vim9=error` mode, `has('vim9script')` honesty, inventory of skipped files. |
| RT-13 | Runtime pinning, update and licensing | P1 | S | FND-01 | Pin Vim 9.2.1338 as a vendored subtree or downloaded artifact, `runtime.lock` tag/archive SHA-256, checksum, Vim license notices, reproducible update script, no local patches to runtime files. |
| RT-14 | Common runtime plugin subset | P3 | L | RT-06, TAG-05 | `matchparen`, `matchit`, `netrw` core, `spellfile`, `tar`/`zip`/`gzip`, `vimball`, `tohtml`; feature-gate or defer individual plugins with documented gaps. |
| RT-15 | Menu, option and legacy scripts | P3 | M | EX-12, RT-07, OPT-08 | `menu.vim`, `synmenu.vim`, `optwin.vim`, `mswin.vim`, `evim.vim`, `delmenu.vim`; TUI-safe fallbacks, no GUI assumptions, `'guioptions'` ignored honestly. |
| RT-16 | Keymap runtime | P3 | M | ENC-05, EX-01 | `:loadkeymap`, `keymap/*.vim` syntax, `'keymap'`, insert/cmdline translation, `'iminsert'`/`'imsearch'`, load errors. |
| RT-17 | Language and menu translations | P3 | M | MSG-04, RT-01 | `lang/*.vim`/`.mo` lookup, `:language`, `'langmenu'`, `:menutranslate`, `v:lang`, fallback to English, encoding. |
| RT-18 | Runtime test-suite adaptation | P2 | XL | TST-02, RT-10 | Port selected Vim `src/testdir` tests to the headless driver, classify unsupported (GUI/Vim9/Vi) cases, run per-commit with a compatibility score. |

## ENC — Encoding and multibyte text

Gate: UTF-8/UTF-16/UTF-32/Latin-1 round-trip; byte and character APIs match
Vim; display widths for CJK, emoji, combining marks and overrides are correct.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| ENC-01 | UTF-8 internal model | P0 | L | FND-02 | Byte strings, char boundaries, char/byte index conversion, invalid byte preservation, NUL-in-string handling, `strchars()`/`charidx()`/`byteidx()` semantics. |
| ENC-02 | File encodings and conversion | P1 | L | ENC-01, BUF-03 | `'fileencoding'`/`'fileencodings'`, UTF-8/UTF-16/UTF-32/Latin-1, BOM detection, iconv integration, replacement/error policy, `:scriptencoding`. |
| ENC-03 | Unicode display width | P1 | L | ENC-01, UI-01 | East Asian width, ambiguous-width option, emoji presentation, variation selectors, combining marks, grapheme clusters in cells, `'delcombine'`. |
| ENC-04 | Character classes and case folding | P1 | L | ENC-01, OPT-03 | `'iskeyword'`/`'isident'`/`'isprint'`/`'isfname'`, `\k`/`\i` classes, Unicode case folding, `'ignorecase'`/`'smartcase'`, locale collation basics. |
| ENC-05 | Input methods and keymaps | P2 | M | ENC-01 | Digraphs, `:loadkeymap`, `'keymap'`, `'iminsert'`/`'imsearch'`, keymap file format, IME commit/preedit path for future GUI; language mappings are MAP-06. |
| ENC-06 | Cell width overrides and display APIs | P2 | M | ENC-03 | `setcellwidths()`/`getcellwidths()`, `strdisplaywidth()`/`virtcol()`/`virtcol2col()`, `ga`/`g8` character info, tab expansion, `'ambiwidth'`/`'emoji'`, screen column conversions. |
| ENC-07 | Locale and language variables | P3 | M | FND-07, MSG-04 | `v:ctype`/`v:lang`/`v:lc_time`/`v:collate`, `:language`, message translation hooks, no dependence on process locale for internal encoding. |
| ENC-08 | Invalid bytes and display fallback | P2 | M | ENC-01, UI-01 | `'display'` uhex, replacement glyphs, non-printable notation, truncation at boundaries, no panics on malformed input. |

## CRYPT — Encryption

Gate: Vim-encrypted files decrypt/encrypt byte-compatibly for zip/blowfish/
blowfish2; wrong keys fail cleanly; swap/undo encryption does not leak plaintext.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| CRYPT-01 | Vim-compatible file encryption | P3 | M | BUF-03 | `'cryptmethod'` zip/blowfish/blowfish2, key derivation/IV/block format, detection of encrypted files, random source, constant-time comparison, streaming. |
| CRYPT-02 | Encryption commands and prompts | P3 | M | CRYPT-01, CMD-06 | `:X`, `'key'`, `:set key=` to clear, confirmation prompts, key caching, no key in history/messages, `v:swapchoice` implications. |
| CRYPT-03 | Encrypted swap and undo | P3 | M | CRYPT-01, BUF-06 | Encrypt swap and persistent undo with the same key, no plaintext temp files, cleanup on key change, recovery flow. |
| CRYPT-04 | Cryptography test vectors | P3 | M | CRYPT-01, TST-01 | Known-answer vectors, interop with Vim-generated files, wrong-key/corruption tests, fuzzing of decrypt paths. |

## SEC — Security, sandbox and limits

Gate: untrusted modelines/plugins cannot execute shell commands without the
documented opt-in; `:sandbox`/`-Z` block filesystem/process access; adversarial
inputs terminate with errors rather than hangs.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| SEC-01 | Restricted mode and sandbox | P2 | M | EX-01, FND-07 | `-Z`, `:sandbox`, `'secure'`, blocked commands (`:!`, `:write` to other files, `system()`), error messages, per-command trust state. |
| SEC-02 | Modeline and exrc restrictions | P1 | M | OPT-06 | `'modeline'`/`'modelines'`/`'secure'`, allowlisted options, `'exrc'`, `:set exrc`, sandboxed modeline execution, no shell expansion. |
| SEC-03 | Shell invocation and quoting | P1 | L | EX-08 | `'shell'`/`'shellcmdflag'`/`'shellquote'`/`'shellredir'`/`'shellxquote'`/`'shellslash'`/`'shelltemp'`, argument construction, injection tests, temp-file permissions. |
| SEC-04 | Resource limits and timeouts | P2 | M | REGEX-08, VIML-01 | `'maxfuncdepth'`/`'maxmapdepth'`/`'maxmempattern'`, regex step budget, script recursion, memory ceilings, graceful `E` errors. |
| SEC-05 | QuickJS sandbox | P3 | M | JS-11 | No std/os modules, no arbitrary file/process access, module allowlist, `:sandbox` enforcement through `vim` API, memory/stack limits. |
| SEC-06 | Untrusted plugin containment | P3 | M | SEC-01, AUC-05 | Autocommand/mapping trust model, `:sandbox` propagation into callbacks, clear diagnostics, no privilege escalation through modelines or tags. |

## TST — Testing and quality

Gate: every subsystem has unit, differential and (where visual) golden-frame
tests; CI runs the runtime smoke suite and fuzz targets nightly; no known
data-corruption bug ships.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| TST-01 | Unit and integration harness | P0 | M | FND-06 | Test fixtures, in-memory editor, key injection, Ex runner, assertions for text/registers/options/messages, deterministic time and randomness. |
| TST-02 | Vim oracle and differential tests | P2 | L | TST-01 | Spawn pinned Vim in headless mode, canonicalize output, compare buffer/cursor/registers/messages; skip documented version/feature gaps; golden corpus stored as text. |
| TST-03 | Frame golden tests | P1 | M | UI-12, TST-01 | 80x24 and 120x40 snapshots, cell attributes, cursor position, wide chars, popup menu, statusline/tabline/cmdline; update tooling with review. |
| TST-04 | Fuzzing | P2 | L | TST-01 | cargo-fuzz for regex parser/VM, Ex parser, VimL parser/GC, input decoder, modeline, undo, JS bridge; corpus minimization and CI smoke. |
| TST-05 | Runtime compatibility dashboard | P2 | L | RT-11, TST-02 | Per-file pass/fail for plugins, ftplugins, indents, syntax, colors, compilers; trend charts; block release on regressions. |
| TST-06 | Performance benchmarks | P2 | M | FND-08 | Startup, 1MB/100MB file open, insert at start/end, scroll, syntax redraw, regex, undo depth, JS call overhead; budgets and regression thresholds. |
| TST-07 | Vim test-suite adaptation | P2 | XL | TST-02 | Port selected `src/testdir` tests; classify GUI/Vim9/Vi/Nvim gaps; harness translation for `:redir` and assert functions. |
| TST-08 | Security and DoS tests | P3 | M | SEC-04, TST-04 | Regex bombs, deep recursion, huge modelines, malicious tags/help, input escape floods, JS memory exhaustion, timeout guarantees. |

## MISC — Platform and optional features

These are tracked so the catalog is complete. Most are gated by `has()` and do
not block the core milestones.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| MISC-01 | Right-to-left and Arabic shaping | P4 | L | ENC-03, WIN-10 | `'rightleft'`/`'rightleftcmd'`, Hebrew/Arabic shaping, bidi cursor movement, display reordering; optional, clearly gated. |
| MISC-02 | Printing and `:hardcopy` | P4 | L | BUF-03, UI-01 | `:hardcopy`, `'printoptions'`/`'printdevice'`/`'printexpr'`/`'printheader'`/`'printfont'`/`'printencoding'`, PostScript/PDF pipeline, platform print dialogs absent. |
| MISC-03 | Client-server | P4 | L | JOB-08 | X11/Unix-socket remote commands, `--remote*`, `serverlist()`/`remote_expr()`, `+clientserver` honesty; optional. |
| MISC-04 | NetBeans and Sun Workshop | P4 | S | — | Legacy integrations explicitly unsupported; `has('netbeans_intg')` false, commands error with a clear message. |
| MISC-05 | Sound | P4 | S | FND-07 | `sound_playfile()`/`sound_clear()`, `+sound` gating, no failure when audio unavailable. |
| MISC-06 | Title, icon and toolbar options | P2 | S | UI-05, OPT-01 | `'title'`, `'titleold'`, `'titlestring'`, `'icon'`/`'iconstring'`, `'toolbar'`/`'toolbariconsize'` stored; GUI-only parts inert in TUI. |
| MISC-07 | Terminfo/termcap fallback | P1 | M | UI-05 | `-T`, `'term'`, terminfo database lookup, builtin xterm fallback, missing-capability diagnostics, `:set termcap`. |
| MISC-08 | Platform quirks | P4 | M | FND-07 | macOS option-key handling, Windows ConPTY/path/drive semantics, Linux console mouse, WSL, Android/Termux detection, exit codes and signal names. |
| MISC-09 | Startup timing and diagnostics | P1 | S | FND-09 | `--startuptime` output, `--cmd`/`-c` ordering, crash log location, `-V` verbose levels. |
| MISC-10 | Translation and gettext | P3 | M | MSG-04 | Message catalog lookup, `:language`, `$LANG`, plural forms, untranslated fallback, no encoding mismatches. |
| MISC-11 | Version and feature matrix | P1 | S | FND-04 | `:version`/`--version`, `has()` table, patch level `v:versionlong`, honest false values for GUI/Vim9/unsupported features, including `statusline_click`, `image*`, `android` and `termux`. |
| MISC-12 | Images and image protocols | P4 | L | DSP-07, UI-02, UI-13 | Optional `+image`: `image_add()`/`image_info()`/`image_discard()`, popup `image` attribute, `'imageprotocol'` sixel/kitty, `v:imagebackend`, `getbgcolor()`, `has('image*')` reporting; deferred, default build reports false and rejects image attributes with a clear error. |

## VIM9 — Deferred Vim9 script

Everything in this area is **deferred** by project decision. The default build
does not implement Vim9 script; it detects and skips Vim9 runtime files with a
clear warning. `has('vim9script')` returns false. This area exists so the
decision is visible and can be reversed without re-planning the rest.

| ID | Feature | Ph | Cx | Deps | Scope |
|---|---|---|---|---|---|
| VIM9-01 | `vim9script` syntax and semantics | P4 | XL | VIML-18, RT-12 | Script header, `:var`/`:final`/`:const`, expression syntax, type inference, whitespace rules, `:legacy`/`:vim9cmd` modifiers. |
| VIM9-02 | Compiled `:def` functions | P4 | XL | VIM9-01 | `:def`/`:enddef`, argument/return types, compiled execution, `:defcompile`, `:disassemble`, performance targets. |
| VIM9-03 | Classes, interfaces, enums and types | P4 | XL | VIM9-02 | `:class`/`:interface`/`:enum`/`:type`, inheritance, object lifetime/GC, method dispatch. |
| VIM9-04 | `:import`/`:export` and autoload imports | P4 | L | VIM9-01 | Module resolution, `import autoload`, exported symbols, `import/` directory, cycle detection. |
| VIM9-05 | Vim9 tooling | P4 | M | VIM9-02 | `:disassemble`, `:defcompile`, compiler diagnostics, Vim9 profiler integration. |


## Recommended selections

The catalog is intentionally larger than any first milestone. These bundles are
dependency-closed starting points; every bundle can be narrowed by area or ID.

### A. Walking skeleton — P0 (29 features)

    python3 tools/select_features.py --bundle walking --with-deps

This is the smallest build that proves the architecture: a headless editor, a
cell/frame UI contract, a TUI backend, buffer/file basics, Normal/Insert modes,
one Ex command path, a cmdline widget and a statusline widget. It should open a
file, edit, undo, write and quit, with golden frames in CI.

### B. Daily driver — P0 + P1 (177 features)

    python3 tools/select_features.py --bundle daily --with-deps

Adds splits/tabs, visual mode, registers, search/substitute, mappings,
autocommands, legacy VimL core, syntax highlighting, folds/display basics,
runtime loading, help/tags and modelines. This is the first build suitable for
editing real projects without falling back to Vim for common tasks.

### C. Scripting and automation

    python3 tools/select_features.py --bundle scripting --with-deps

VimL, builtins, Vim regex and, later, QuickJS/jobs. Select this separately from
the editor core: a broken script engine should not block daily editing.

### D. Runtime compatibility

    python3 tools/select_features.py --bundle runtime --with-deps

Loaders, filetype/ftplugin/indent, syntax/colors/compiler, help/tags,
quickfix and spell. This is the bundle that turns "Vim-like" into "runs Vim's
runtime".

### E. Custom selection

    python3 tools/select_features.py --ids EDT-10,REGEX-02,JS-08 --with-deps
    python3 tools/select_features.py --areas VIML,FUN,REGEX --max-phase P2
    python3 tools/select_features.py --phase P2 --format ids

The tool rejects unknown IDs, includes transitive dependencies, and prints a
topologically ordered table (dependencies first). Use `catalog/features.json`
when integrating with an issue tracker.

## Definition of done (per feature)

A feature is done only when all of the following hold:

1. **Contract:** its behavior and errors are documented, including what is
   deliberately unsupported.
2. **Tests:** unit tests cover the core cases; differential tests against Vim
   9.2 are used where observable behavior is compatible; UI features have
   headless golden frames.
3. **Runtime:** if the feature is used by `runtime/` scripts, at least one real
   runtime file exercises it in the smoke suite.
4. **Honesty:** `has()`/`exists()` return accurate values; a missing feature
   produces a clear error, never a silent wrong result.
5. **Safety:** no panic or undefined behavior on malformed input; unsafe code
   is isolated and documented; security-sensitive paths have negative tests.
6. **Performance:** no unbounded regression against the budgets in
   ARCHITECTURE.md; hot paths have a benchmark.
7. **No forks:** no modifications to the pinned Vim runtime; compatibility
   fixes go into the engine, not the data.

## Mechanical coverage appendix

The curated catalog is backed by generated inventories from the pinned
Vim 9.2.1338 runtime tree. Test fixtures under `testdir/` are excluded because
the editor never loads them. Counts are from the generated files in
[inventory/](inventory/README.md).

| Inventory | Count | Where |
|---|---:|---|
| Ex commands | 593 | [inventory/commands.md](inventory/commands.md) |
| Mode keys | 695 | [inventory/keys.md](inventory/keys.md) |
| Options | 455 | [inventory/options.md](inventory/options.md) |
| Built-in functions | 605 | [inventory/functions.md](inventory/functions.md) |
| Autocommand events | 136 | [inventory/events.md](inventory/events.md) |
| `has()` features | 211 | [inventory/features.md](inventory/features.md) |
| Syntax files | 790 | [inventory/runtime-surface.md](inventory/runtime-surface.md) |
| Filetype plugins | 445 | [inventory/runtime-surface.md](inventory/runtime-surface.md) |
| Indent files | 191 | [inventory/runtime-surface.md](inventory/runtime-surface.md) |
| Compiler definitions | 137 | [inventory/runtime-surface.md](inventory/runtime-surface.md) |
| Help tags | 12,156 | [inventory/runtime-surface.md](inventory/runtime-surface.md) |
| Vim9-script runtime files | 63 | [inventory/runtime-surface.md](inventory/runtime-surface.md) |

The per-item tables are the audit trail. The catalog is the selection surface;
the inventory is the proof that the catalog did not forget a documented area.

## Vim 9.2.1338 delta

The inventory was regenerated from tag `v9.2.1338` (archive SHA-256
`eeb4370a8db32041af1c3b05159bdeeea6b781f5a32d0f1f68bd0c51d45880f3`).
The tag's `version9.txt` groups these additions under `added-9.3`; the pinned
tag is authoritative for this project.
Every new surface item is classified below. There are no unclassified additions.

| Addition | Catalog home | Decision |
|---|---|---|
| `ch_listen()` | JOB-03 | In scope with channels; loopback/Unix only, accept callback. |
| `getbgcolor()` | UI-13, MISC-12 | In scope with screen introspection; image compositing can use it. |
| `getinfo()` | FUN-18 | In scope; function/command/option/vimvar introspection. |
| `image_add()`/`image_info()`/`image_discard()` | MISC-12 | Deferred with `+image`; default build reports false and errors clearly. |
| `tabpanel_getinfo()`/`tabpanel_scroll()` | TAB-08 | In scope with the tabpanel sidebar. |
| `preinserted()` | FUN-10 | In scope with completion state. |
| `redraw_listener_add()`/`redraw_listener_remove()` | UI-15 | In scope after the core redraw loop. |
| `TextPutPre`/`TextPutPost` | EDT-12, AUC-01 | In scope; `TextPutPre` may rewrite `regcontents`. |
| `SessionLoadPre` | BUF-14, AUC-01 | In scope before session restoration. |
| `'termsync'`, `'termresize'`, `t_BS`/`t_ES` | UI-05 | In scope for the TUI; no-op when the terminal lacks support. |
| `'statuslineopt'`, multi-line statuslines | STA-08 | In scope after single-line statusline layout. |
| `'winhighlight'` | SYN-02 | In scope as window-local highlight remapping. |
| `'pumopt'`, `'pumborder'` | CMD-04 | In scope with the completion popup menu. |
| `'scrolloffpad'` | WIN-08 | In scope with scrolling options. |
| `'modelinestrict'` | OPT-06 | In scope with the modeline safe-option allowlist. |
| `'tagsecure'` | TAG-01 | In scope; reject URL-like tag file fields with E1576. |
| `'imageprotocol'` | MISC-12 | Deferred with `+image`; hidden/false in the default build. |
| `statusline_click` flag and `%[Func]` regions | STA-06 | In scope; `has('statusline_click')` becomes true when implemented. |
| `image`, `image_sixel`, `image_kitty`, `image_cairo`, `image_gdk`, `image_gdi` | MISC-12 | Deferred; all report false in the default build. |
| `android`, `termux` | MISC-08, MISC-11 | Platform detection only; false on macOS/Linux builds. |
| `--stdio-channel`, `ch_open('stdio')` | JOB-03, FND-09 | In scope with channels; startup handoff is parsed by FND-09. |
| `Popup`, `PopupBorder`, `PopupTitle`, `VertSplitNC` highlight groups | SYN-01 | In scope with the default highlight table. |
| `v:imagebackend` | VIML-13, MISC-12 | In scope as a predefined variable; reports the disabled backend by default. |

Removed upstream before this tag: `'isexpand'` and `complete_match()` (patch
9.1.1933). RVim must not implement them; `has()`/`exists()` must not advertise
them. Runtime additions (new syntax, ftplugin, indent, compiler, colorscheme,
package and help files) are covered by the runtime tiers and tracked only in
the inventory. Build switches such as `--enable-pixman` (images, MISC-12) and
`--enable-hardcopy-pango` (printing, MISC-02) are packaging options, not
separate runtime features.

## Version reconciliation

The target is pinned to tag `v9.2.1338`, not to a moving branch. The checked-in
inventory is generated from that tag's unmodified `runtime/` tree. Re-run:

    python3 tools/gen_inventory.py --runtime /path/to/vim-9.2.1338/runtime \
      --label "Vim 9.2.1338" --out docs/inventory

Then review the diff: every new command, option, function, event or `has()`
flag must be classified as in-scope, deferred, or excluded. This is a release
gate, not a documentation chore. The archive checksum is recorded in
[RUNTIME-COMPAT.md](RUNTIME-COMPAT.md); a tag or checksum change requires the
same review.






