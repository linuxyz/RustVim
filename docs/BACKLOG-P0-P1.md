# P0+P1 implementation backlog

Generated from [catalog/features.json](../catalog/features.json). This is the
implementation contract for the 177 P0/P1 features; status is tracked here and
in [STATUS.md](STATUS.md).

Total: **177** features (P0 29, P1 148).

Status values: pending, in-progress, done, blocked. A feature is only
done when its scope is implemented, tested, and the catalog definition of
done is satisfied. Status records the concrete remaining gap when a feature
is in progress; status is maintained in catalog/status.json.

Status: **177** done.

## BUF - Buffers and files

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| BUF-01 | Line buffer and byte indexing | P0 | L | ENC-01, FND-05 | done | - |
| BUF-03 | File read/write core | P0 | L | BUF-01, FND-07 | done | - |
| BUF-02 | Buffer list and lifecycle | P1 | L | BUF-01 | done | - |
| BUF-04 | File format, EOL and BOM | P1 | M | BUF-03 | done | - |
| BUF-05 | File encodings and iconv | P1 | L | BUF-03, ENC-02 | done | - |
| BUF-06 | Backup, swap and recovery | P1 | L | BUF-03, FND-07 | done | - |
| BUF-07 | Marks, jumplist, changelist | P1 | L | BUF-01 | done | - |
| BUF-08 | Buffer-local variables and options | P1 | M | BUF-01, OPT-03 | done | - |
| BUF-09 | Special buffer types | P1 | M | BUF-02 | done | - |
| BUF-10 | Argument list | P1 | M | BUF-02 | done | - |
| BUF-11 | Working directory and path handling | P1 | M | BUF-02, FND-07 | done | - |
| BUF-17 | File identity, permissions and `:saveas` | P1 | S | BUF-03 | done | - |

Scope notes:
- BUF-01: Stable line IDs, byte-indexed access, NUL/NL fidelity, `changedtick`, O(log n) line insert/delete via chunked line tree, O(1) line count.
- BUF-03: `:edit`/`:write`/`:read`, address ranges, `:write !cmd`, `:read !cmd`, filters, permissions, `'writeany'`, partial writes, errors without data loss.
- BUF-02: Listed/unlisted, loaded/unloaded, hidden, `:badd`/`:bufadd`/`:bufload`/`:bdelete`/`:bwipeout`, alternate buffer `#`, `CTRL-^`/`CTRL-6`, `BufAdd/BufDelete/BufUnload` events.
- BUF-04: `'fileformat'` unix/dos/mac, `'fileformats'` detection, `'endofline'`, `'fixendofline'`, `'bomb'`, mixed-EOL reporting, byte-exact rewrite.
- BUF-05: `'fileencoding'`, `'fileencodings'` probe order, BOM/UTF-16, iconv fallback, `:scriptencoding`; internal encoding is UTF-8 only.
- BUF-06: `'backup'`/`'writebackup'`/`'backupcopy'`/`'backupskip'`, swap file naming/permissions/`'updatetime'`, `:preserve`, `:recover`, `-r`, `v:swapchoice`.
- BUF-07: `a-z`/`A-Z`/`0-9` marks, special marks `'` `.` `^` `[` `]` `<` `>`, mark adjustment on edits, jumplist `<C-o>/<C-i>`, changelist `g;/g,`, `getmarklist()`.
- BUF-08: `b:` variables, buffer options, `b:changedtick`, `getbufvar()`/`setbufvar()`, `:let b:...`; lifetime tied to buffer wipe.
- BUF-09: `'buftype'` nofile/nowrite/acwrite/quickfix/help/prompt, `'bufhidden'`, `'swapfile'`, `'modifiable'`/`'readonly'` enforcement and plugin hooks.
- BUF-10: Local/global arglists, `:args`/`:next`/`:prev`/`:rewind`/`:argument`/`:argadd`/`:argdelete`, `argadd()`/`argv()`, `:argdo`.
- BUF-11: `:cd`/`:lcd`/`:tcd`/`:pwd`, `'autochdir'`, `'path'`, `findfile()`/`finddir()`, `DirChanged` events, filename modifiers and escaping.
- BUF-17: `:file`/`:saveas`, read-only file handling, ownership/mode preservation, `'readonly'` override with `!`.

## CMD - Command line

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| CMD-01 | Command-line state machine and editor | P0 | L | UI-01, EX-01 | done | - |
| CMD-02 | Command-line history | P1 | M | CMD-01 | done | - |
| CMD-03 | Command-line completion engine | P1 | L | CMD-01, BUF-02 | done | - |
| CMD-04 | Wildmenu and popup completion | P1 | M | CMD-03, UI-01 | done | - |
| CMD-05 | Command-line window | P1 | M | CMD-01, BUF-09, WIN-01 | done | - |
| CMD-06 | Prompt functions and raw input | P1 | M | CMD-01, MSG-01 | done | - |
| CMD-07 | Search prompt and incremental search | P1 | M | CMD-01, REGEX-01 | done | - |

Scope notes:
- CMD-01: `:`, `/`, `?` and `=` prompts; insert/overwrite, cursor motions, word/line kill, `<C-r>`, `<C-v>`, digraphs, `<C-f>`/`<C-b>`, history recall, prompt type.
- CMD-02: `'history'`, `:history`, `histadd()`/`histdel()`/`histget()`/`histnr()`, `@:` and `@@`, search history, duplicate suppression, persistence via BUF-15.
- CMD-03: Context detection and all completion types: commands, files/dirs, buffers, tags, options, functions, variables, mappings, autocmds, highlights, syntax, compiler, environment; `getcompletion()`.
- CMD-04: `'wildmenu'`, `'wildmode'`, `'wildoptions'` (fuzzy, pum, tagfile), `'pumheight'`/`'pumwidth'`, `'pumopt'`/`'pumborder'` borders, match highlighting, `wildmenumode()`, cancel semantics.
- CMD-05: `q:`/`q/`/`q?`, `'cedit'`, `'cmdwinheight'`, execute on `<CR>`, cancel on `<C-c>`, editable history, `getcmdwintype()`.
- CMD-06: `input()`/`inputlist()`/`inputsecret()`/`confirm()`, `getchar()`/`getcharstr()`/`getcharmod()`, mouse in prompts, cancel/timeout return values.
- CMD-07: `'incsearch'`, `'hlsearch'`, search offsets, `<C-g>/<C-t>`, cancel restores view, `v:searchforward`, pattern delimiter and escaping.

## EDT - Editing core

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| EDT-01 | Normal-mode dispatch and counts | P0 | L | FND-03 | done | - |
| EDT-02 | Basic cursor motions | P0 | M | EDT-01, BUF-01 | done | - |
| EDT-03 | Word and WORD motions | P1 | M | EDT-02, ENC-04 | done | - |
| EDT-04 | Line and column motions | P1 | S | EDT-02 | done | - |
| EDT-05 | Paragraph, sentence, section and bracket motions | P1 | M | EDT-02, ENC-04 | done | - |
| EDT-06 | Character search motions | P1 | S | EDT-02 | done | - |
| EDT-07 | Search motions | P1 | L | EDT-02, REGEX-01, CMD-07 | done | - |
| EDT-08 | Marks, jumps and change positions | P1 | M | EDT-02, BUF-07 | done | - |
| EDT-09 | Screen motions and scrolling keys | P1 | M | EDT-02, WIN-02 | done | - |
| EDT-10 | Operator engine | P1 | L | EDT-01, UND-01 | done | - |
| EDT-11 | Text objects | P1 | L | EDT-10, ENC-04 | done | - |
| EDT-12 | Registers, yank and put | P1 | L | EDT-10, UI-10 | done | - |
| EDT-13 | Join and split lines | P1 | S | EDT-10, INS-01 | done | - |
| EDT-14 | Replace and virtual replace | P1 | M | EDT-01 | done | - |
| EDT-15 | Visual modes | P1 | L | EDT-01, WIN-02 | done | - |
| EDT-17 | Dot repeat and redo register | P1 | M | EDT-10 | done | - |
| EDT-18 | Macros and `:normal` | P1 | M | EDT-01, EX-06 | done | - |
| EDT-19 | Increment and decrement | P1 | S | EDT-02 | done | - |
| EDT-20 | Case, shift, format and indent operators | P1 | L | EDT-10, INS-07 | done | - |
| EDT-23 | Paste mode and bracketed paste | P1 | S | EDT-01, UI-06 | done | - |

Scope notes:
- EDT-01: Mode stack, counts, register prefix, operator-pending state, `v:count`/`v:count1`, ambiguous key resolution, error/beep policy, `<Esc>` cancellation.
- EDT-02: `h j k l`, arrows, `<Space>`, `<BS>`, `+ - _ <CR>`, cursor clamping, counts, `'startofline'` interplay.
- EDT-03: `w W b B e E ge gE` with `'iskeyword'`, counts, end-of-line crossing rules, multibyte correctness.
- EDT-04: `0 ^ $ g_`, bar motion, `Home/End`, `g0/g^/g$/gm`, `gj/gk`, `'startofline'`, virtual columns and `curswant`.
- EDT-05: `{ } ( ) [[ ]] [] ][ [{ ]} [( ])`, `%` and `'matchpairs'`, `'paragraphs'`/`'sections'`, comments and preprocessor blocks.
- EDT-06: `f F t T ; ,`, counts, `'ignorecase'`? (char search is exact), repeat direction, multibyte character boundaries.
- EDT-07: `/ ? n N * # g* g#`, `gn/gN`, `'wrapscan'`/`'ignorecase'`/`'smartcase'`, `'incsearch'`/`'hlsearch'`, search offsets, `v:searchforward`.
- EDT-08: Apostrophe and backtick mark motions, `[ ] < >` marks, `<C-o>/<C-i>`, `g;`/`g,`, `:jumps`/`:changes`/`:marks`, jump vs change list semantics.
- EDT-09: `H M L`, `<C-e>/<C-y>`, `<C-d>/<C-u>`, `<C-f>/<C-b>`, `zz/zt/zb/z./z-/z+`, `'scroll'` and cursor keep.
- EDT-10: `d c y < > = ! gq gw g~ gu gU g? zf g@`, motion/text-object composition, counts, linewise/charwise/blockwise, register selection, abort/undo block.
- EDT-11: `iw aw iW aW is as ip ap i( a( i[ a[ i{ a{ i< a< it at i` `a` `i'` `a'` `i"` `a"`, `ib/ab/iB/aB`, nesting, whitespace rules, block virtual edit.
- EDT-12: Unnamed, `0-9`, `a-z/A-Z`, `-`, `"`, `.`, `:`, `%`, `#`, `=`, `_`; `p/P/gp/gP/]p/[p`, `:put`/`:yank`, `getreg()`/`setreg()`/`getreginfo()`, `TextPutPre`/`TextPutPost` with `v:event`, `'clipboard'`.
- EDT-13: `J`/`gJ`, `:join` with ranges, `'joinspaces'`, insert-mode newline, line splitting at cursor with indent preservation.
- EDT-14: `R`, `gR`, replace stack/backspace restore, `'revins'` deferred with RTL, virtual replace at tab/wide-char boundaries.
- EDT-15: `v/V/<C-v>`, `o/O` swap ends, `gv`, `:visual`, `'selection'`, blockwise virtual corners, visual marks `'<`/`'>`, mode messages.
- EDT-17: `.` with counts, insert replay, operator replay, `".` register, `'report'`, changes that do not set dot, undo interaction.
- EDT-18: `q{reg}`, `@{reg}`/`@@`, counts, recursive macro abort, `:normal[!]` over ranges, register pollution rules, `q:` integration.
- EDT-19: `<C-a>/<C-x>`, `g<C-a>/g<C-x>` in Visual mode, `'nrformats'` (binary/octal/hex/alpha), leading zeros, overflow.
- EDT-20: `~ g~ gu gU g?`, `>> <<`, `=`/`==`, `gq/gw`, `'formatoptions'`, `'textwidth'`, `'shiftwidth'`/`'expandtab'`/`'shiftround'`.
- EDT-23: `'paste'`, `:set paste`, bracket-paste insertion without mappings/abbreviations/autoindent, paste register behaviour.

## ENC - Encoding and multibyte text

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| ENC-01 | UTF-8 internal model | P0 | L | FND-02 | done | - |
| ENC-02 | File encodings and conversion | P1 | L | ENC-01, BUF-03 | done | - |
| ENC-03 | Unicode display width | P1 | L | ENC-01, UI-01 | done | - |
| ENC-04 | Character classes and case folding | P1 | L | ENC-01, OPT-03 | done | - |

Scope notes:
- ENC-01: Byte strings, char boundaries, char/byte index conversion, invalid byte preservation, NUL-in-string handling, `strchars()`/`charidx()`/`byteidx()` semantics.
- ENC-02: `'fileencoding'`/`'fileencodings'`, UTF-8/UTF-16/UTF-32/Latin-1, BOM detection, iconv integration, replacement/error policy, `:scriptencoding`.
- ENC-03: East Asian width, ambiguous-width option, emoji presentation, variation selectors, combining marks, grapheme clusters in cells, `'delcombine'`.
- ENC-04: `'iskeyword'`/`'isident'`/`'isprint'`/`'isfname'`, `\k`/`\i` classes, Unicode case folding, `'ignorecase'`/`'smartcase'`, locale collation basics.

## EX - Ex command engine

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| EX-01 | Ex parser, ranges and addresses | P0 | L | FND-04 | done | - |
| EX-02 | Command dispatch and user commands | P1 | L | EX-01 | done | - |
| EX-03 | `:global` and `:vglobal` | P1 | M | EX-01, REGEX-01 | done | - |
| EX-04 | `:substitute` | P1 | L | EX-01, REGEX-01 | done | - |
| EX-05 | Line and range commands | P1 | M | EX-01, EDT-12 | done | - |
| EX-06 | `:normal` and `:execute` | P1 | M | EX-01, EDT-01 | done | - |
| EX-08 | Shell and filter integration | P1 | L | FND-07, BUF-03 | done | - |
| EX-09 | Command modifiers | P1 | M | EX-01 | done | - |
| EX-10 | Listing and introspection commands | P1 | M | EX-02 | done | - |

Scope notes:
- EX-01: Command abbreviation resolution, ranges (`.`, `$`, `%`, marks, search offsets, `;`), counts, bang/register/args, comments, command separators, modifier prefixes.
- EX-02: Built-in command table, `:command` attributes (`-nargs`, `-range`, `-count`, `-register`, `-bang`, `-bar`, `-buffer`, `-complete`, `-addr`, `-keepscript`), replacement placeholders `<args>`/`<q-args>`/`<f-args>`/`<line1>`/`<line2>`/`<count>`/`<reg>`/`<bang>`/`<mods>`.
- EX-03: Mark matching lines, nested command execution, `:g/pat/normal ...`, delete/append interaction, error continuation, recursion limits.
- EX-04: `:s`/`:sno`/`:smagic`/`:snomagic`, flags `g c e i I n p # l r`, count, `&`/`:&&`/`g&`, replacement `\0-\9`, `~`, `\u \U \l \L \e \E`, `\=` expression, multiline.
- EX-05: `:delete`/`:yank`/`:put`/`:join`/`:move`/`:copy`/`:t`/`:sort`/`:retab`/`:left`/`:right`/`:center`/`:z`/`:print`/`:number`/`:list`/`:undolist`.
- EX-06: `:normal[!]` with ranges and counts, `:execute` with expression lists and nested commands, `:eval`, error propagation and abort rules.
- EX-08: `:!`, `:r !`, `:w !`, `:!!`, `:shell`, filter ranges, `'shell'`/`'shellcmdflag'`/`'shellquote'`/`'shellredir'`/`'shellxquote'`/`'shelltemp'`, exit status `v:shell_error`.
- EX-09: `:silent`/`:silent!`/`:unsilent`, `:verbose`, `:noautocmd`, `:keepalt`, `:keepjumps`, `:keepmarks`, `:lockmarks`, `:keeppatterns`, `:sandbox`, `:tab`, `:vertical`, `:aboveleft`/`:belowright`/`:leftabove`/`:rightbelow`/`:topleft`/`:botright`, `:hide`, `:noswapfile`, `:confirm`, `:filter`.
- EX-10: `:ls`/`:buffers`/`:files`/`:args`/`:marks`/`:registers`/`:jumps`/`:changes`/`:map`/`:abbreviate`/`:autocmd`/`:highlight`/`:syntax`/`:scriptnames`/`:messages`/`:options`/`:command`/`:function`/`:set` with Vim-compatible text and redir capture.

## FND - Foundation and process

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| FND-01 | Cargo workspace and crate graph | P0 | S | - | done | - |
| FND-02 | Core IDs, errors and diagnostics | P0 | S | FND-01 | done | - |
| FND-03 | Event loop and redraw scheduler | P0 | L | FND-01 | done | - |
| FND-04 | Registries for commands/options/functions/events | P0 | M | FND-02 | done | - |
| FND-05 | Editor state ownership and arenas | P0 | M | FND-02 | done | - |
| FND-06 | Headless driver and golden frames | P0 | M | FND-01, UI-12 | done | - |
| FND-07 | Platform abstraction | P0 | L | FND-01 | done | - |
| FND-08 | Benchmark and fuzz harness | P1 | M | FND-06 | done | - |
| FND-09 | CLI and startup orchestrator | P1 | M | FND-04, FND-07 | done | - |
| FND-10 | Tracing and verbose diagnostics | P1 | S | FND-02 | done | - |

Scope notes:
- FND-01: Workspace with the crates in ARCHITECTURE.md, feature flags, CI on macOS/Linux, pinned MSRV, license notices.
- FND-02: Newtypes for buffer/window/tab/script IDs; `VimError` with `E###` codes, message text and throwpoint; no panics across API boundaries.
- FND-03: Single-threaded scheduler for input, timers, job events, screen resize and deferred redraw; nearest-deadline waits; no busy loop.
- FND-04: One metadata table per surface: name, abbreviation, arity, flags, feature gate, doc tag, handler. Drives parsing, completion, `:help` and the inventory audit.
- FND-05: `Editor` owns buffers, windows, tab pages, scripts, registers, marks and global variables; slotmap/indexmap arenas, explicit borrow-splitting for render.
- FND-06: Scriptable editor driver: feed keys, run Ex, inspect model, snapshot Frame; used by every subsystem test.
- FND-07: Filesystem, process spawn, PTY, signals, terminal size, clipboard, locale, environment; Unix first, Windows behind the same traits.
- FND-08: Criterion-style benchmarks for startup, frame render, regex, syntax and large files; cargo-fuzz targets for parsers and input.
- FND-09: Args (`-c`, `-S`, `-u`, `--clean`, `-p`, `-o`, `-O`, `-R`, `-r`, `--headless`, `--stdio-channel` handoff), init order, `--startuptime`, exit codes; `--headless` is an RVim extension.
- FND-10: Category-based logs (`input`, `ex`, `viml`, `render`, `runtime`, `job`, `js`), `'verbose'` levels, `:verbose` source tracing, crash report.

## INS - Insert and replace modes

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| INS-01 | Insert, append and open-line | P0 | M | EDT-01 | done | - |
| INS-02 | Insert-mode special keys | P1 | M | INS-01 | done | - |
| INS-03 | Digraphs | P1 | M | INS-01, ENC-01 | done | - |
| INS-04 | Abbreviations | P1 | M | INS-01, EX-01 | done | - |
| INS-07 | Indentation engines | P1 | XL | INS-01, REGEX-01 | done | - |
| INS-09 | Insert-mode mappings and paste | P1 | M | MAP-01, INS-01 | done | - |
| INS-10 | Start/stop insert and one-shot commands | P1 | S | INS-01 | done | - |
| INS-11 | Repeat of insert changes | P1 | M | EDT-17, INS-01 | done | - |

Scope notes:
- INS-01: `i I a A o O gi gI`, insert position restoration, counts, `'insertmode'` basics, mode message, undo block boundaries.
- INS-02: `<BS> <C-h> <C-w> <C-u> <C-t> <C-d> <C-e> <C-y> <C-r> <C-o> <C-v> <C-k> <C-@> <C-]>`, `'backspace'`, re-indent keys in `'cinkeys'`/`'indentkeys'`.
- INS-03: `<C-k>`, `'digraph'`, `:digraphs`, RFC 1345 table, `digraph_get()`/`digraph_set()`, insert and cmdline use.
- INS-04: `:abbreviate`/`:iabbrev`/`:cabbrev`, `<buffer>`, `<expr>`, prefixes `i`/`c`/`!`, `'paste'` bypass, expansion before/after cursor, `<C-]>`.
- INS-07: `'autoindent'`, `'smartindent'`, `'cindent'` with `'cinoptions'`/`'cinkeys'`, `'lisp'`, `indent()`, `cindent()`, `shiftwidth()`; expression indentation is INS-12.
- INS-09: `:imap` family, `<buffer>`/`<expr>`/`<nowait>`, `'paste'`, bracket paste, `CTRL-O` one-shot Normal command, mapping failure/beep.
- INS-10: `:startinsert`/`:stopinsert`, `gi`, `<C-o>`, insert-mode `<C-r>` register insertion, returning to Insert after command.
- INS-11: Dot-repeat of inserted text, `<C-a>` in insert (last inserted text), undo block grouping, replace-mode repeat.

## MSG - Messages, errors and prompts

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| MSG-01 | Message pipeline | P0 | M | UI-01, FND-04 | done | - |
| MSG-02 | Hit-enter and message history | P1 | M | MSG-01 | done | - |
| MSG-03 | Capturing output | P1 | M | MSG-01, EX-01 | done | - |
| MSG-04 | Error formatting and throwpoints | P1 | M | MSG-01, FND-02 | done | - |
| MSG-05 | Intro, version and help output | P1 | S | MSG-01 | done | - |

Scope notes:
- MSG-01: `:echo`/`:echon`/`:echomsg`/`:echoerr`/`:echowindow`, message kinds and highlight groups, `v:statusmsg`/`v:errmsg`, expression-list echo, no message truncation of data.
- MSG-02: `'more'`, hit-enter prompt, `:messages` window, message ring buffer, `'cmdheight'`=0 behaviour and scrollback.
- MSG-03: `:redir` to variable/file/register, `execute()` capture, `:silent`/`:silent!`/`:unsilent`, `:filter`, ordering with nested commands.
- MSG-04: E-code registry, `v:errmsg`/`v:exception`/`v:throwpoint`, source/line context, `:verbose` suffix, translation lookup, `:silent!` suppression.
- MSG-05: `:intro`, `:version`, `--version`, feature matrix, startup screen, `'shortmess'` flags, `:help`-less usage.

## OPT - Options

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| OPT-01 | Option registry and typed values | P0 | L | FND-04 | done | - |
| OPT-02 | `:set` parser, flags and abbreviations | P0 | M | OPT-01 | done | - |
| OPT-03 | Scopes, inheritance and global-local options | P1 | L | OPT-01 | done | - |
| OPT-04 | Option access from VimL | P1 | M | OPT-03, VIML-03 | done | - |
| OPT-05 | Defaults and reset semantics | P1 | S | OPT-02 | done | - |
| OPT-06 | Modelines | P1 | M | OPT-03 | done | - |
| OPT-09 | OptionSet event and side effects | P1 | M | OPT-03, AUC-01 | done | - |

Scope notes:
- OPT-01: All Vim 9.2.1338 options with type, scope, default, flags, validator, short name and doc tag; boolean/number/string with typed storage, not stringly-typed; feature-gated options report availability honestly.
- OPT-02: `set`/`setlocal`/`setglobal`, `?`, `&`, `&vim`, `!`, `inv`, `no`, `+=`/`-=`/`^=`, `<` reset, `all`, unambiguous abbreviations.
- OPT-03: Global, buffer, window and tab scopes; global-local options (`'statusline'`, `'errorformat'`, `'tags'`); `:setlocal` fallback semantics; option <-> window ownership.
- OPT-04: `&opt`, `&l:opt`, `&g:opt`, `setbufvar()`/`setwinvar()` option keys, `getbufvar()` readback, option reset rules.
- OPT-05: Vim defaults (not Vi), `&vim`/`all&`, `--clean`, `-u DEFAULTS`, exclusion of `'compatible'` behavior, startup option files.
- OPT-06: `'modeline'`/`'modelines'`/`'modelinestrict'` parsing, first/last N lines, safe-option allowlist, secure filtering, OptionSet side effects and tests.
- OPT-09: Fire `OptionSet` with `v:option_*`, apply side effects (wrap, numberwidth, syntax, filetype, foldmethod), avoid recursive set storms.

## STA - Statusline and tabline widgets

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| STA-01 | Statusline layout engine | P0 | M | UI-01, WIN-01 | done | - |
| STA-02 | Statusline item expansion | P1 | M | STA-01, VIML-11 | done | - |
| STA-03 | Statusline expression evaluation and caching | P1 | M | STA-02, SYN-01 | done | - |
| STA-04 | Mode, ruler and command feedback | P1 | M | STA-01, OPT-03 | done | - |
| STA-05 | Tabline layout and items | P1 | M | TAB-01, UI-01 | done | - |

Scope notes:
- STA-01: Left/center/right groups (`%=`), truncation (`%<`), minimum widths, highlight run merging, wide-character width, click region capture.
- STA-02: `%f %F %t %m %M %r %R %h %H %w %W %y %Y %l %L %c %C %v %V %o %O %p %P %a %{...} %#hl# %1* %0* %= %< %%%`, `%[FuncName]`/`%[]` click regions, `%@` newline and modifier flags.
- STA-03: Evaluate `'statusline'` per window with `g:statusline_winid`, sandbox errors into messages, cache by buffer/window/changedtick, invalidate on relevant events.
- STA-04: `'laststatus'`, `'ruler'`/`'rulerformat'`, `'showcmd'`/`'showcmdloc'`, `'showmode'`, `'shortmess'`, counts, pending operator display and `CTRL-G` file info.
- STA-05: Default tab labels, tab numbers, modified marker, `%1T` clickable tabs, `%X` close, `%#HL#`, `%{...}`, alignment/truncation, `'showtabline'`.

## TST - Testing and quality

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| TST-01 | Unit and integration harness | P0 | M | FND-06 | done | - |
| TST-03 | Frame golden tests | P1 | M | UI-12, TST-01 | done | - |

Scope notes:
- TST-01: Test fixtures, in-memory editor, key injection, Ex runner, assertions for text/registers/options/messages, deterministic time and randomness.
- TST-03: 80x24 and 120x40 snapshots, cell attributes, cursor position, wide chars, popup menu, statusline/tabline/cmdline; update tooling with review.

## UI - UI protocol, compositor and TUI

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| UI-01 | Cell, grid, frame and attribute model | P0 | M | FND-02 | done | - |
| UI-02 | `UiBackend` trait and capabilities | P0 | M | UI-01 | done | - |
| UI-03 | Compositor | P0 | L | UI-01 | done | - |
| UI-04 | Hit regions and mouse routing | P0 | M | UI-01 | done | - |
| UI-05 | TUI backend | P0 | L | UI-02, FND-07 | done | - |
| UI-06 | TTY input decoding | P0 | L | UI-02, FND-07 | done | - |
| UI-12 | Headless snapshot backend | P0 | S | UI-02 | done | - |
| UI-07 | Color negotiation and fallback | P1 | M | UI-02, SYN-02 | done | - |
| UI-08 | Cursor, focus and paste events | P1 | M | UI-06 | done | - |
| UI-09 | Suspend, resume and signals | P1 | M | UI-05, FND-07 | done | - |
| UI-10 | Clipboard providers | P1 | M | FND-07 | done | - |
| UI-14 | Redraw control | P1 | M | UI-03 | done | - |

Scope notes:
- UI-01: Backend-neutral `Frame` with grids, `Cell` (grapheme cluster + attr id), `Attr` (fg/bg/sp, bold, italic, underline/curl/double/dotted/dashed, strike, reverse, blink, conceal), cursor and metadata.
- UI-02: Lifecycle trait: init, present, poll event, resize, title, clipboard, suspend/resume, shutdown; capability negotiation for color depth, mouse, paste, focus, kitty keyboard, synchronized output, in-band resize, IME, pixel metrics and images.
- UI-03: Layer stack (base, popups, external overlays), z-order, clipping, dirty rectangles/rows, cursor occlusion, frame sequence numbers and diff-friendly output.
- UI-04: Core registers clickable/draggable regions with semantic `ActionId` (window border, statusline item, tab, sign, fold, popup item); backends send coordinates only.
- UI-05: Raw mode, alternate screen, terminfo/termcap lookup, `SIGWINCH` resize, `'termresize'` in-band resize, `'termsync'` synchronized output, title, bell, cursor shape, panic-safe terminal restore, `:set t_*` bridge.
- UI-06: Function/arrow keys, modifiers, xterm/SGR/urxvt/X10 mouse, bracketed paste, focus events, `modifyOtherKeys` and kitty keyboard protocol; documented ESC/Alt ambiguity policy tied to `'ttimeoutlen'`.
- UI-12: Deterministic 80x24 (configurable) cell dump, JSON snapshots, `screenchar()`-style inspection, diffable frames for CI.
- UI-07: Mono/16/256/truecolor, `t_Co`, `'termguicolors'`, nearest-color mapping for cterm, GUI truecolor path, stable `AttrId` palette per frame.
- UI-08: Cursor style per mode, blink policy, focus in/out, bracketed-paste payloads, `'paste'` handoff, kitty key release/repeat filtering.
- UI-09: `SIGTSTP`/`SIGCONT`/`SIGINT`/`SIGTERM`, `:stop`/`:suspend`, terminal state save/restore around shell commands, `VimSuspend`/`VimResume`.
- UI-10: macOS `pbcopy/pbpaste`, Wayland `wl-copy/wl-paste`, X11 `xclip/xsel`, optional OSC 52; core owns `'clipboard'`/registers, provider only moves bytes.
- UI-14: `:redraw`/`:redraw!`, `'lazyredraw'`, `'redrawtime'`, partial updates, message/flush ordering, no flicker on large frames.

## UND - Undo and redo

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| UND-01 | Undo tree and undo blocks | P0 | L | BUF-01 | done | - |
| UND-02 | `u`/`<C-r>` and counts | P1 | M | UND-01, EDT-01 | done | - |
| UND-03 | Ex undo navigation | P1 | M | UND-01, EX-01 | done | - |

Scope notes:
- UND-01: Branching undo tree, sequence numbers, time stamps, block boundaries for insert/operator/Ex edits, `'undolevels'` pruning, `undotree()` data.
- UND-02: Counted undo/redo, cursor restore, `'undolevels'` negative/zero special cases, message reporting.
- UND-03: `:undo`, `:redo`, `:earlier`/`:later` with counts and time units (`s`, `m`, `h`, `d`, `f`), `g-`/`g+`.

## WIN - Windows and layout

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| WIN-01 | Split tree, layout and sizes | P0 | L | FND-05, UI-01 | done | - |
| WIN-02 | Viewport, cursor and scroll state | P1 | L | WIN-01, BUF-01 | done | - |
| WIN-03 | Window-local options and variables | P1 | M | WIN-01, OPT-03 | done | - |
| WIN-04 | Window commands and `:wincmd` | P1 | M | WIN-01, EX-01 | done | - |
| WIN-05 | Window focus, movement and constraints | P1 | M | WIN-01 | done | - |
| WIN-10 | Window decorations and separators | P1 | L | WIN-02 | done | - |
| WIN-12 | Global statusline and `'laststatus'` | P1 | S | WIN-01, STA-01 | done | - |

Scope notes:
- WIN-01: Frame tree of horizontal/vertical splits, min sizes, `'equalalways'`/`'eadirection'`, resize propagation, deterministic geometry for tests.
- WIN-02: Per-window `topline`/`leftcol`/`skipcol`/`curswant`, independent cursors for one buffer, scrolloff, cursor clamping, `winsaveview()`/`winrestview()`.
- WIN-03: `w:` variables, window options (`'number'`, `'wrap'`, `'foldmethod'`, `'signcolumn'`...), inheritance on split and `:setlocal`.
- WIN-04: `:split`/`:vsplit`/`:new`/`:vnew`/`:close`/`:only`/`:hide`/`:quit`/`:resize`/`:vertical resize`, `CTRL-W` command matrix, `:wincmd`.
- WIN-05: `CTRL-W h/j/k/l/w/W/t/b/p`, `:wincmd`, `'winminheight'`/`'winminwidth'`, focus follows close, alternate window, `WinEnter`/`WinLeave`.
- WIN-10: Window separators, statusline rows (height supplied by the statusline widget), border/click regions, correct width accounting, resize repaint; gutters are DSP-10.
- WIN-12: `'laststatus'` 0/1/2/3, global statusline, statusline cache invalidation on window/buffer changes.

## AUC - Autocommands

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| AUC-01 | Event registry and firing | P1 | L | FND-04 | done | - |
| AUC-02 | `:autocmd` and `:augroup` | P1 | L | AUC-01, EX-02 | done | - |
| AUC-03 | Pattern matching and match variables | P1 | M | AUC-02 | done | - |
| AUC-04 | Execution control | P1 | M | AUC-02 | done | - |
| AUC-05 | Event execution and error handling | P1 | L | AUC-02, VIML-13 | done | - |
| AUC-06 | Buffer-local autocommands | P1 | M | AUC-02, BUF-02 | done | - |
| AUC-07 | Filetype, syntax and option event wiring | P1 | M | AUC-01, RT-05 | done | - |

Scope notes:
- AUC-01: Event definitions and ordering, per-event pattern rules, `v:event` payloads, fast/slow event policy, recursion guard; includes `TextPutPre`/`TextPutPost` and `SessionLoadPre`.
- AUC-02: Add/remove/list, groups, `au!`, `++once`, `++nested`, `<buffer=N>`, event/pattern completion, `:doautocmd`/`:doautoall`.
- AUC-03: `*`/`?`/`[...]` patterns, comma-separated patterns, `<amatch>`/`<afile>`/`<abuf>`/`<aevent>`, `expand('<amatch>')`.
- AUC-04: `'eventignore'`/`'eventignorewin'`, `:noautocmd`, nested-event suppression, errors inside autocommands, abort/continue semantics.
- AUC-05: Script context (`<sfile>`/`<stack>`), exceptions, hit-enter avoidance, `v:event` lifetime, reentrancy and `:silent!`.
- AUC-06: `<buffer>` registration, automatic cleanup on wipe, buffer pattern matching, `autocmd_get()` filtering.
- AUC-07: `BufRead/BufNewFile/FileType/Syntax/OptionSet` integration, `:filetype`/`:syntax` loading hooks, secure event ordering.

## DSP - Display features

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| DSP-08 | List chars, fill chars and break indent | P1 | M | WIN-10 | done | - |
| DSP-09 | Wrapping and smooth scrolling | P1 | L | WIN-02 | done | - |
| DSP-10 | Gutters and screen columns | P1 | M | WIN-10 | done | - |
| DSP-13 | Cursor line, column and conceal interaction | P1 | S | SYN-01, WIN-10 | done | - |

Scope notes:
- DSP-08: `'list'`/`'listchars'`, `'fillchars'`, `'showbreak'`, `'breakindent'`/`'breakindentopt'`, tab/space/trail/nbsp/eol/extends/precedes, `'display'` lastline/truncate/uhex.
- DSP-09: `'wrap'`/`'nowrap'`, `'linebreak'`/`'breakat'`, `'sidescroll'`/`'sidescrolloff'`, `'smoothscroll'`, `gj/gk`/`g0/g$/gm`, wrapped-line cursor and mouse mapping, skipcol.
- DSP-10: `'number'`/`'relativenumber'`/`'numberwidth'`/`'numberformat'`, `'colorcolumn'`, `'signcolumn'`, `'foldcolumn'`, `'cursorline'`/`'cursorlineopt'`, `'cursorcolumn'`, column width and click regions.
- DSP-13: `'cursorline'`/`'cursorcolumn'`/`'colorcolumn'` priority, conceal reveal on cursor line, cursor screen position with virtual text, no flicker.

## MAP - Mappings and keys

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| MAP-01 | Mapping command matrix | P1 | L | EDT-01, CMD-01 | done | - |
| MAP-02 | Key notation and special keys | P1 | L | MAP-01 | done | - |
| MAP-03 | Mapping expansion and timeouts | P1 | L | MAP-01, OPT-03 | done | - |

Scope notes:
- MAP-01: `:map`/`:nmap`/`:vmap`/`:xmap`/`:smap`/`:omap`/`:imap`/`:lmap`/`:cmap`/`:tmap`, `!` variants, `:noremap`/`:unmap`/`:mapclear`, mode-specific tables.
- MAP-02: `<C-...>`, `<S-...>`, `<M-...>`, `<D-...>`, `<F1>`-`<F12>`, `<Plug>`, `<SID>`, `<script>`, `<expr>`, `<unique>`, `<buffer>`, `<nowait>`, `<silent>`, `<special>`, key code normalization.
- MAP-03: Recursive vs non-recursive expansion, `'maxmapdepth'`, `'timeout'`/`'ttimeout'`/`'timeoutlen'`/`'ttimeoutlen'`, ambiguous prefix handling, remap stack and error messages.

## MISC - Platform and optional features

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| MISC-07 | Terminfo/termcap fallback | P1 | M | UI-05 | done | - |
| MISC-09 | Startup timing and diagnostics | P1 | S | FND-09 | done | - |
| MISC-11 | Version and feature matrix | P1 | S | FND-04 | done | - |

Scope notes:
- MISC-07: `-T`, `'term'`, terminfo database lookup, builtin xterm fallback, missing-capability diagnostics, `:set termcap`.
- MISC-09: `--startuptime` output, `--cmd`/`-c` ordering, crash log location, `-V` verbose levels.
- MISC-11: `:version`/`--version`, `has()` table, patch level `v:versionlong`, honest false values for GUI/Vim9/unsupported features, including `statusline_click`, `image*`, `android` and `termux`.

## QF - Quickfix and location lists

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| QF-01 | Quickfix and location list model | P1 | M | FND-05 | done | - |

Scope notes:
- QF-01: Entries (filename, lnum, col, text, type, valid, nr, module, pattern), list stacks/IDs, titles, context, `'switchbuf'`, per-window location lists.

## REGEX - Vim regular expressions

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| REGEX-01 | Parser, AST and magic modes | P1 | XL | FND-02 | done | - |
| REGEX-02 | Backtracking VM and captures | P1 | XL | REGEX-01 | done | - |
| REGEX-03 | Lookaround and boundary atoms | P1 | L | REGEX-02 | done | - |
| REGEX-05 | Character classes and case folding | P1 | L | REGEX-01, ENC-04 | done | - |
| REGEX-06 | Engine integration | P1 | L | REGEX-02 | done | - |
| REGEX-08 | Timeouts, interrupts and limits | P1 | M | REGEX-02, FND-03 | done | - |

Scope notes:
- REGEX-01: `\v \m \M \V` magic switching, atoms, groups `\%(`, alternation, quantifiers `* \+ \= \{n,m} \{-}`, character classes, escapes, delimiter handling, syntax errors with positions.
- REGEX-02: Byte-oriented matching with capture groups `\1-\9`, greedy/lazy quantifiers, backreferences, leftmost-first semantics, recursion/step limits, match spans.
- REGEX-03: `\@= \@! \@<= \@<! \@>`, `\zs \ze`, `\%^ \%$ \%# \%V \%'m \%l \%c`, word-boundary atoms, zero-width repetition rules.
- REGEX-05: `\i \I \k \K \f \F \p \P \s \S \d \D \w \W \a \A \l \L \u \U \x \X \o \O \h \H`, POSIX classes, `[[=a=]]` equivalence, `\c \C`, `'ignorecase'`/`'smartcase'`.
- REGEX-06: Search (`/ ? n N * #`), `:substitute`, `:global`, `match()` family, syntax engine, filetype patterns, quickfix errorformat, tag search; shared compiled-pattern cache.
- REGEX-08: Step budget, `got_int` checks, `'redrawtime'`/`'matchtime'`, backtracking limit errors, resumable matching for huge files.

## RT - Vim runtime compatibility

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| RT-01 | `'runtimepath'` discovery and order | P1 | L | FND-07 | done | - |
| RT-02 | `:runtime` and `:runtime!` | P1 | M | RT-01 | done | - |
| RT-04 | Sourcing and script identity | P1 | M | EX-01, FND-07 | done | - |
| RT-05 | Filetype detection | P1 | L | RT-02 | done | - |
| RT-08 | Defaults and startup modes | P1 | M | RT-01, OPT-05 | done | - |
| RT-09 | Runtime documentation and tags | P1 | M | RT-01, TAG-06 | done | - |
| RT-11 | Compatibility tiers and matrix | P1 | M | RT-01 | done | - |
| RT-13 | Runtime pinning, update and licensing | P1 | S | FND-01 | done | - |

Scope notes:
- RT-01: Unix/macOS/Windows defaults, `$VIMRUNTIME`/`$VIM`, user config dirs, `after/` last, `:set rtp` parsing, path normalization and escaping.
- RT-02: Glob expansion, all-matches ordering, `after` semantics, missing-file errors, `:runtime` with `!`, source errors with script attribution.
- RT-04: `:source`, script IDs, `:scriptnames`, `<sfile>`/`<slnum>`/`<stack>`, `:finish`, nested source, error line mapping, BOM/shebang handling.
- RT-05: `filetype.vim`/`scripts.vim` loading, `:setf`/`:setfiletype`, `'filetype'`, extension/pattern/shebang detection, `BufNewFile/BufRead` ordering, `:filetype on/off`.
- RT-08: `defaults.vim`, `vimrc_example.vim`, `-u NONE`/`-u NORC`/`-u DEFAULTS`, `--clean`, `--noplugin`, `vimrc`/`gvimrc` order, `v:vim_did_enter` and `VimEnter`.
- RT-09: `doc/tags`, `:help` on shipped docs, `'helplang'`, doc file encoding, `:helptags` regeneration, missing-tag diagnostics.
- RT-11: Tier A (core editing/help/syntax), Tier B (common plugins/languages), Tier C (everything), explicit gap list, CI dashboard and release gate.
- RT-13: Pin Vim 9.2.1338 as a vendored subtree or downloaded artifact, `runtime.lock` tag/archive SHA-256, checksum, Vim license notices, reproducible update script, no local patches to runtime files.

## SEC - Security, sandbox and limits

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| SEC-02 | Modeline and exrc restrictions | P1 | M | OPT-06 | done | - |
| SEC-03 | Shell invocation and quoting | P1 | L | EX-08 | done | - |

Scope notes:
- SEC-02: `'modeline'`/`'modelines'`/`'secure'`, allowlisted options, `'exrc'`, `:set exrc`, sandboxed modeline execution, no shell expansion.
- SEC-03: `'shell'`/`'shellcmdflag'`/`'shellquote'`/`'shellredir'`/`'shellxquote'`/`'shellslash'`/`'shelltemp'`, argument construction, injection tests, temp-file permissions.

## SYN - Syntax and highlighting

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| SYN-01 | Highlight table and default groups | P1 | M | UI-01 | done | - |
| SYN-02 | Highlight attributes, colors and links | P1 | L | SYN-01 | done | - |
| SYN-03 | Background and color resolution | P1 | M | SYN-02, UI-07 | done | - |
| SYN-04 | Colorschemes | P1 | M | SYN-02, RT-01 | done | - |
| SYN-05 | Syntax engine core | P1 | XL | REGEX-06, BUF-01 | done | - |
| SYN-06 | Syntax containment and linking | P1 | XL | SYN-05 | done | - |
| SYN-08 | `:syntax` commands and Syntax event | P1 | M | SYN-05, AUC-07 | done | - |

Scope notes:
- SYN-01: Global/local highlight groups, default links, `Normal`/`Comment`/`Constant` families, `Popup`/`PopupBorder`/`PopupTitle`/`VertSplitNC`, `:hi clear`, `:highlight` with no args, group redefinition and `hlID()`.
- SYN-02: `cterm`/`gui` attributes, `ctermfg/bg`, `guifg/bg`, `guisp`, `font`, `blend` (stored), `NONE`, `:hi link`/`:hi def link`/`:hi!`, `'winhighlight'` window-local remapping, color names and `#RRGGBB`.
- SYN-03: `'background'` light/dark, `'termguicolors'`, `t_Co`, `v:colornames`, cterm nearest-color fallback, `ColorScheme` event ordering.
- SYN-04: `:colorscheme`, `colors/*.vim` search order, `g:colors_name`, `:hi clear` then reload, errors in colorschemes, `after/colors`.
- SYN-05: `:syn keyword/match/region/cluster/include`, start/end/skip patterns, priorities, keyword boundaries, case options, per-line state and highlight spans.
- SYN-06: `contains`/`containedin`/`contained`, `nextgroup`, `matchgroup`, `transparent`, `extend`/`keepend`, `oneline`, `excludenl`, offset modifiers `ms=`/`me=`/`hs=`/`he=`/`rs=`/`re=`.
- SYN-08: `:syntax on/off/enable/clear`, `'syntax'`, `Syntax` event, `syntax/syntax.vim` loading, `:runtime! syntax/<ft>.vim` order, `syntax_cmd` handling.

## TAB - Tab pages

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| TAB-01 | Tab-page model | P1 | M | WIN-01 | done | - |
| TAB-02 | Tab commands and navigation | P1 | M | TAB-01, EX-01 | done | - |
| TAB-03 | `'showtabline'` and tabline integration | P1 | M | TAB-01, STA-05 | done | - |

Scope notes:
- TAB-01: Ordered tab pages, each with a window tree and current window; tab-local variables/options; closing the last window closes the tab.
- TAB-02: `:tabnew`/`:tabedit`/`:tabclose`/`:tabonly`/`:tabmove`/`:tabnext`/`:tabprevious`/`:tabfirst`/`:tablast`, `gt`/`gT`/`<C-PageUp>`, counts.
- TAB-03: 0/1/2 modes, tabline row ownership, minimum one tab, resize interaction, tabline hidden when appropriate.

## TAG - Tags and help

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| TAG-01 | Tag file parser and search | P1 | M | FND-07 | done | - |
| TAG-02 | Tag commands and stack | P1 | L | TAG-01, WIN-01 | done | - |
| TAG-05 | Help buffer and navigation | P1 | L | TAG-01, BUF-09 | done | - |
| TAG-06 | Helptags generation and doc parsing | P1 | M | TAG-05, FND-07 | done | - |
| TAG-07 | Keyword lookup | P1 | S | TAG-02, EX-08 | done | - |

Scope notes:
- TAG-01: `tags` format, sorted/binary search, duplicate tags, `'tags'`/`'tagrelative'`/`'taglength'`/`'showfulltag'`/`'tagsecure'` (reject URL file fields with E1576), Emacs tags optional, encoding and caching.
- TAG-02: `:tag`/`:tselect`/`:tjump`/`:tnext`/`:tprev`/`:tfirst`/`:tlast`/`:trewind`/`:pop`, `CTRL-]`/`CTRL-t`/`g]`, tag stack, preview hooks, tag pattern execution.
- TAG-05: `:help`/`:h`, `'buftype=help'`, `'iskeyword'`/`'modifiable'`, bar-delimited tag links, `CTRL-]`/`CTRL-t`/`CTRL-o`, `:helpclose`, `'helpheight'`.
- TAG-06: `:helptags` over `doc/*.txt`, `*tag*` extraction, duplicate/error reporting, `doc/tags` format, `:helptags ALL`.
- TAG-07: `K`/`'keywordprg'`, `g]` tag select, `CTRL-W ]`/`CTRL-W }`, `:ptag`/`:ptselect`, man page fallback, shell quoting.

## VIML - Legacy Vim script

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| VIML-01 | Value model and garbage collector | P1 | XL | FND-05 | done | - |
| VIML-02 | Expression and script parser | P1 | XL | VIML-01 | done | - |
| VIML-03 | Variables and scopes | P1 | L | VIML-02 | done | - |
| VIML-04 | Function definitions and calls | P1 | L | VIML-02 | done | - |
| VIML-05 | Funcref, Partial, lambda and closures | P1 | L | VIML-04 | done | - |
| VIML-06 | Control flow and exceptions | P1 | L | VIML-02 | done | - |
| VIML-07 | Literals, collections and slicing | P1 | L | VIML-01 | done | - |
| VIML-08 | Operators, coercion and comparison | P1 | M | VIML-07 | done | - |
| VIML-09 | Script-local scope and script identity | P1 | M | VIML-03, RT-04 | done | - |
| VIML-10 | Autoload functions | P1 | L | VIML-04, RT-02 | done | - |
| VIML-11 | `:execute`, `eval()` and capture | P1 | M | VIML-02, MSG-03 | done | - |
| VIML-12 | Exceptions and throwpoints | P1 | M | VIML-06 | done | - |
| VIML-13 | Predefined `v:` variables | P1 | M | VIML-03 | done | - |
| VIML-18 | Script sourcing semantics | P1 | M | VIML-02, RT-04 | done | - |

Scope notes:
- VIML-01: Number (64-bit), Float, byte String (NUL-safe), List, Dict, Blob, Funcref, Partial, Bool, Special, Channel/Job; mark-sweep heap with cycles, roots and deterministic collection points.
- VIML-02: Pratt/recursive-descent expression grammar, `:let`/`:call`/`:execute` forms, function bodies, line continuation, comments, heredoc, string escapes, error recovery with line numbers.
- VIML-03: `g: b: w: t: s: l: a: v:`, `:let`/`:const`/`:unlet`/`:unlet!`, `:lockvar`/`:unlockvar`, environment `$VAR`, options `&opt`, registers `@r`, destructuring.
- VIML-04: `:function`/`:endfunction`, `:call`, `:return`, `:delfunction`, default arguments, varargs `.../a:000/a:0/a:1`, dict functions, recursion, `'maxfuncdepth'`.
- VIML-05: `function()`/`funcref()`, `Partial` with bound args/dict, `{x -> ...}`, closures over outer locals, `->` method chains, `call()`, garbage-collected function values.
- VIML-06: `:if/:elseif/:else/:endif`, `:while/:for`, `:break/:continue`, `:try/:catch/:finally/:endtry/:throw`, `:finish`, exception patterns and cleanup ordering.
- VIML-07: Number formats (decimal/octal/hex/binary), Float, strings and escapes, List/Dict/Blob literals, indexing and slicing, negative indices, `v:null`/`v:none`/`v:true`/`v:false`.
- VIML-08: Arithmetic, bitwise, boolean short-circuit, string/list/dict comparison, `==#`/`==?`/`is`/`isnot`/`=~`/`!~`, type coercion table, division by zero errors.
- VIML-09: `s:` variables, `<SID>`, `<script>`, `:scriptnames`, `:scriptversion`, script IDs stable across reload, `getscriptinfo()`.
- VIML-10: `foo#bar#baz()` naming, lazy sourcing of `autoload/foo/bar.vim`, `#` escaping, load-once semantics, error attribution to the loaded script.
- VIML-11: `:execute` with expression lists, `eval()`, `:redir` integration, nested command strings, newline handling, evaluation errors and abort.
- VIML-12: `v:exception`, `v:throwpoint`, `:throw`, `:catch /pat/`, rethrow, error conversion, `try` around Ex commands, cleanup with `:finally`.
- VIML-13: `v:count/count1/register`, `v:lnum`, `v:val/key`, `v:char`, `v:completed_item`, `v:event`, `v:option_*`, `v:errmsg/statusmsg/warningmsg`, `v:version/versionlong`, `v:vim_did_enter`, `v:argv`, `v:shell_error`, `v:termresponse`, `v:imagebackend`, `v:testing`, `v:exiting`, type constants.
- VIML-18: `:source`, line continuation rules, `:finish`, `:scriptencoding`, `:scriptversion`, modeline-free script header, nesting and error reporting.

## VIS - Visual and Select modes

| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |
|---|---|---|---|---|---|---|
| VIS-01 | Visual selection model | P1 | L | EDT-01 | done | - |
| VIS-02 | Visual operators and block insert | P1 | L | VIS-01, EDT-10 | done | - |
| VIS-03 | Visual marks and ex integration | P1 | M | VIS-01, EX-01 | done | - |

Scope notes:
- VIS-01: Char/line/block modes, anchor and cursor, `o/O`, `gv`, `:visual`, per-window visual state, reselect after operator, `'selection'`.
- VIS-02: `d c y < > = ! gq gw g~ gu gU g? J p P r~ u U`, block `I A c $`, blockwise put, virtual block corners, `'virtualedit'`.
- VIS-03: `'<`/`'>`, `:'<,'>` ranges, visual `:normal`/`:!`/`:vimgrep`, `gv` after command, `'<`/`'>` update rules.
