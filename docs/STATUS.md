# RVim implementation status

**Scope:** P0 and P1 from [FEATURES.md](FEATURES.md) (177 catalog entries), plus
the P2 folding (DSP-01/02/03) and signs (DSP-04) subsystems.
**Target:** Vim 9.2.1338.
**State:** working editor and test suite; P0 is complete, P1 is substantially
implemented but not yet at the catalog's definition of done. Per-feature status
lives in [BACKLOG-P0-P1.md](BACKLOG-P0-P1.md), generated from
[catalog/status.json](../catalog/status.json).
This file is deliberately conservative: a feature is only marked **done** when
its observable scope and tests exist.

## Build and run

~~~sh
cargo build --workspace
cargo test --workspace
cargo run -p rvim-cli -- path/to/file
cargo run -p rvim-cli -- --headless --keys 'ihello<Esc>:wq<CR>' file.txt
~~~

The TUI requires a real terminal. The headless driver is deterministic and is
used by the golden-frame tests. VIMRUNTIME may point at an unpacked Vim
9.2.1338 runtime tree; the repository itself does not vendor the 19 MB tree.

## P0 - walking skeleton

| Feature | Status | Evidence / caveat |
|---|---|---|
| FND-01 workspace and crate graph | done | 16-crate workspace, CI file, MSRV 1.85 |
| FND-02 IDs, errors, diagnostics | done | generational arena, typed IDs, VimError |
| FND-03 event loop and redraw scheduler | done | input/timer/job-event ingress, resize and deferred redraw; nearest-deadline waits; job execution itself is P3 |
| FND-04 command/option/function/event registries | done | 593 commands with doc tag/handler/flags/arity/feature and 586 abbreviations; 605 functions; 136 events; 456 option specs (455 pinned plus numberformat) |
| FND-05 editor ownership and arenas | done | buffers, windows, tabs and scripts use the generational arena; registers are fixed named slots; buffer marks use an ordered map; globals live in the editor |
| FND-06 headless driver and golden frames | done | rvim-headless plus integration tests |
| FND-07 platform abstraction | done (Unix) | files, atomic writes, shell, clipboard, terminal size, PTY |
| OPT-01 all 455 pinned options (+numberformat) | done | typed values, source flags/defaults, short names, 145 feature gates (310 available), value validators |
| OPT-02 :set parser | done | boolean/number/string, ?, &, !, inv, no, +=, abbreviations |
| BUF-01 line tree | done | chunked line tree, byte columns, changedtick |
| BUF-03 file read/write | done | atomic write, backups, range writes, :write !/:read !, permissions/writeany, filters |
| UND-01 undo tree | done | branching tree, sequence/time, block boundaries, undolevels pruning, undotree() |
| WIN-01 split layout | done | layout tree, separators, statusline allocation |
| UI-01 cell/frame/attr | done | backend-neutral protocol |
| UI-02 backend trait | done | TUI, headless |
| UI-03 compositor | done | z-ordered layers, clipping, popup layer, dirty rows, cursor occlusion |
| UI-04 hit regions | done | semantic window/statusline/tab/popup regions, click callbacks and draggable split borders |
| UI-05 TUI | done | raw mode, ANSI cells, cursor, resize, cleanup |
| UI-06 TTY input | done | arrows/F-keys/mouse/SGR/bracketed paste/UTF-8 |
| UI-12 headless backend | done | frames and snapshots |
| STA-01 statusline layout | done | default and custom template, active/inactive |
| CMD-01 command line | done | editing, history, completion, command mode |
| MSG-01 message pipeline | done | echo/echon/echomsg/echoerr/echowindow, kinds/highlights, history, v:statusmsg/v:errmsg |
| EDT-01 normal dispatch/counts | done | counts, pending operators/registers |
| EDT-02 basic motions | done | h/j/k/l/0/$/gg/G etc. |
| INS-01 insert/append/open | done | insert, append, open line, replace-char |
| EX-01 Ex parser | done | ranges, addresses, marks, search addresses, modifiers |
| ENC-01 UTF-8 internal model | done | byte strings, Unicode width table |
| TST-01 test harness | done | 525 workspace tests at last run |

P0 is complete.  Options gated behind unimplemented features (GUI, terminal,
spell, diff, sessions, Python/Ruby/etc.) are absent from `:set`, completion
and `exists("+name")`, matching a Vim built without those features.

## P1 - daily driver

Implemented and exercised by tests:

- **Options:** scopes, local/window/buffer storage, defaults, modelines,
  OptionSet, VimL option access, statusline customization.
- **Window views:** winsaveview()/winrestview(); getwinpos()/getwinposx()/getwinposy().
- **Window queries:** winnr()/winbufnr()/winheight()/winwidth()/winline()/wincol().
- **Completion:** complete()/complete_add()/complete_check()/pumvisible()/complete_info().
- **Registers:** getreg()/setreg()/getregtype() with list and kind options; getmousepos() reflects the last mouse event.
- **Buffer queries:** bufnr()/bufname()/bufexists()/bufloaded()/bufwinnr()/bufadd().
- **Buffer lines:** getline()/setline(), getbufline()/getbufoneline()/setbufline()/append()/deletebufline().
- **Buffers/files:** :sball/:sbnext/:sbprevious/:sbrewind/:sblast split-buffer navigation, buffer list and switching, alternate buffer, getjumplist()/getchangelist()/getbufinfo() (windows/variables)/getwininfo() (geometry/variables), argument list, :bufdo, DOS/Mac/Unix formats, BOM, built-in UTF-8/Latin-1/UTF-16 conversion with 'fileencodings' probe order, iconv(1) fallback for other encodings (has('iconv')), :scriptencoding in sourced scripts, backups, swap files and :recover, :saveas, :file, cwd and path modifiers (%:h/%:t/%:r/%:e/%:p).
  :bufdo, DOS/Mac/Unix formats, BOM, Latin-1/UTF-16 decoding, backups, swap
  files and :recover, :saveas, :file, cwd handling.
- **Scripts:** getscriptinfo() and :scriptnames list sourced scripts with stable sids.
- **Tags:** tagfiles()/taglist() read the 'tags' index, gettagstack()/settagstack() inspect/reset the tag stack; the K command runs 'keywordprg' (Ex command or shell program), g]/CTRL-]/CTRL-W ] select, jump to and split-jump to the keyword under the cursor, and CTRL-W }/CTRL-W z plus :ptag/:ptselect/:pedit/:pclose drive a 'previewheight'-sized preview window.
- **Environment:** getenv()/setenv()/environ(); getpid()/hostname() process identity.
- **File functions:** resolve()/simplify()/fnamemodify(); tempname(), mkdir(), delete() and rename(); glob()/globpath(), filewritable(), getfsize(), getftype() and readdir().
- **Screen:** screenchar()/screenstring() read the rendered frame and screenpos() maps window/line/column to screen coordinates.
- **Translation:** tr() character translation/deletion.
- **Expansion:** expand() supports %, #, ~, $ENV, <cword>/<cWORD>, <afile>/<amatch>.
- **Escaping:** escape()/shellescape()/fnameescape().
- **String matching:** matchlist() captures and matchstrpos() positions.
- **String widths:** strwidth()/strdisplaywidth().
- **String indices:** byteidx()/byteidxcomp()/charidx().
- **Positions:** getcharpos()/setcharpos()/getcursorcharpos()/setcursorcharpos(); virtcol()/virtcol2col(), getcurpos(), getpos(), setpos(), getmarklist() and getmousepos().
- **Prompts:** feedkeys(), getchar(), getcharstr(), input(), inputlist(), inputsecret(), inputdialog(), inputsave()/inputrestore() and confirm() consume a queued key source.
- **Messages:** :messages clear and automatic hit-enter prompt for long/multiline messages; one key dismisses it.
- **Windows/tabs:** winlayout() split tree, win_getid([win[,tab]])/win_id2win()/win_gotoid()/win_findbuf(), gettabinfo() (window id list), gettabwinvar()/settabwinvar(), getwinvar()/setwinvar() and getbufvar()/setbufvar(), vertical/horizontal splits, directional focus, equalalways, winfix resize guards, tab pages with :tabnew/:tabnext/:tabprevious/:tabclose/:tabonly/:tabmove, custom tabline with per-tab hit regions, per-window and global statuslines, closing/only/quit behavior including :tabclose! and :tabonly! (E445 checks modified buffers), command-line getcmdline()/getcmdpos()/getcmdtype()/setcmdline()/setcmdpos() APIs, 'cedit' opening the command-line window from Command-line mode and 'cmdwinheight' sizing it; 'showcmd' pending count/register/operator feedback, 'showmode' INSERT/REPLACE/VISUAL messages, and 'ruler' line/column feedback plus 'rulerformat' expansion and Ctrl-L clear/redraw and :redrawstatus/:redrawtabline in the command line; tab-local variables via t: and settabvar()/gettabvar() plus tabpagenr(), tabpagewinnr() (current/$) and tabpagebuflist(); TabNew/TabClosed/TabEnter/TabLeave autocmds fire on tab lifecycle.
  tabline, per-window statuslines, closing/only/quit behavior.
- **Undo:** branching undo tree with sequence/time, counts, :undo/:redo/:earlier/:later, :undolist, undotree().
- **Editing:** words/WORDs, paragraphs/sentences/brackets, f/t/F/T, jumplist,
  marks, changelist (g; / g,), operators, text objects, registers, visual
  selection operations, case operators, join, replace/insert/repeat, macros,
  increments, gv, screen motions, z-scrolling, gq/gw, >>/<<, indentexpr.
- **Search/regex:** Vim regex engine (magic modes, groups, backrefs,
  quantifiers, classes, zs/ze, lookaround, step budget), / and ?, incsearch,
  n/N, star/hash, hlsearch, :s with captures/flags, :g/:v, :vimgrep.
- **Ex:** user commands, :normal, :execute, :source, :runtime, substitutions,
  global, sort/copy/move, shell filters, ranges, range writes, :write !/:read !,
  permissions/writeany, file commands, quickfix/make/grep, :vimgrep recursive
  globs, :compiler,
  :function listing, :scriptnames, shell bang with %/#/!! expansion, q:/q//q?
  command-line window, tag/help/highlight/autocmd/user-command completion and
  getcompletion().
- **Mappings:** normal/insert/visual/command/operator-pending maps, noremap,
  key notation, multi-key pending, timeout, abbreviations, digraphs.
- **Messages:** echo/echon/echomsg/echoerr/echowindow, message kinds and
  highlights, full history, v:statusmsg/v:errmsg.
- **Registries:** generated metadata for all 593 commands, 605 functions and
  136 events, plus implemented flags for the current command surface.
- **Autocommands:** event registry, groups, patterns, buffer-local patterns,
  User events with names, ++once, filetype/syntax/OptionSet/insert/write events.
- **VimL:** value/GC model, parser/evaluator, functions/closures/lambdas,
  scopes, control flow, exceptions, dictionaries/lists/blobs, heredoc
  assignments (`let x =<< trim END`), many builtins, `:source` execution,
  on-demand autoload script loading for `foo#Bar()` functions and
  `g:foo#var` globals, `:runtime`/`:runtime!` inside sourced scripts,
  user commands with raw `-nargs=*` arguments executed in their defining
  script's `s:` context (including `<q-args>` quoting), dotted function
  names, `is#`/`isnot#`, the bare `version` constant, `v:lang`, and
  `<sfile>` expansion with `:p`/`:h`/`:t`/`:r`/`:e` modifiers,
  Vim 9.2 interpolated strings (`$'...'` / `$"..."` with `{expr}`,
  `{{`/`}}` literal braces) and dynamic `:let {expr}name` targets.
- **Syntax/highlight:** highlight groups and links, :highlight, colorscheme
  sourcing, syntax keyword/match/region engine, runtime syntax loading.
- **Runtime/help:** runtimepath discovery, :runtime/:runtime!, filetype detection, :setf/:setfiletype, :packadd/:packloadall,
  ftplugin/indent loading, real compiler files, findfile/scope-dictionary host
  functions, help tags, :help in a help split, :helpclose,
  :tag/:tselect/:tjump/:tnext/:tprev/:tfirst/:tlast/g],
  :helptags. The runtime compatibility matrix ([COMPATIBILITY.md](COMPATIBILITY.md)) runs the pinned runtime tree in three tiers via tools/runtime_compat.py, gated on Tier A in .github/workflows/runtime-compat.yml. The user vimrc is sourced when present; otherwise
  defaults.vim is used as a fallback when VIMRUNTIME is set and neither -u
  nor --clean is given.
- **UI:** :suspend/:stop terminal suspend/resume, TUI dumb-terminal fallback (plain rows, no ANSI setup/teardown), runtime terminfo numeric/string-capability lookup with a tparm-style parameterized formatter (cursor addressing, conditionals, arithmetic, char/hex/octal printing), settable/queryable `t_` termcap options including t_AF/t_AB, built-in ansi/xterm/screen/tmux/linux/vt100 fallback plus TUI color-depth negotiation (Mono/Ansi16/Ansi256/TrueColor) with ANSI-16 fallback; z-ordered compositor with popup layers, dirty rows and cursor occlusion; semantic hit regions for windows/statusline/tabs/popups; draggable split borders; multi-row wildmenu popup; clipboards, mouse, focus events, wrapped lines with linebreak/breakat/showbreak/breakindent, smoothscroll chunk scrolling with cursor mapping, wrapped-line mouse mapping, wheel smoothscroll, partial-column skipcol rendering/cursor mapping, listchars, scrolloff, resizing, cursorline/column/colorcolumn, gutter reservations for 'signcolumn' (auto/yes/no/number) and 'foldcolumn' with 'numberformat' line-number formatting, plus the fold and sign contents described under P2 below, custom statuslines/echo-window popups.
- **Scheduler:** nearest-deadline input/timer/job-event reactor, VimL timer_start/stop, resize and deferred redraw.
- **Statusline:** %f/%F/%t/%m/%M/%r/%R/%h/%H/%y/%l/%L/%c/%v/%a/%{...}/%=/%<, expression caching, g:statusline_winid, and %[FuncName] clickable regions with minwid and click metadata.
- **Tabline:** custom/default labels, per-tab hit regions, and %[FuncName] click callbacks that take precedence over tab selection.
- **Quickfix:** per-window location lists (:lgetexpr/:lgetfile/:lbuffer/:laddexpr/:laddbuffer/:lfile, :lopen/:lclose/:lnext/:lprevious/:lfirst/:llast/:lrewind/:lolder/:lnewer and getloclist()/setloclist()) plus the quickfix stack; :cgetexpr/:cgetfile/:cbuffer replace and :caddexpr/:caddbuffer append evaluated expressions/buffer lines; errorformat `%n` numbers are captured into entries; list model with getqflist()/setqflist(), 'switchbuf' split/useopen/newtab behavior and :colder/:cnewer stack, :make uses 'makeprg'/arguments, :grep/:grepadd append/parse through 'grepprg' and 'grepformat', :cfile, :copen/:cclose/:cnext, make/grep ingestion, Vim-style errorformat including multi-line %A/%C/%Z groups, %D/%X directory stacks, %P/%Q file stacks, and %E/%W/%I/%t error-type capture.
- **Substitute:** g/i/I/e/n/p/#/l/count/& flags, count-selected matches, :s repeat, :&, ~ replacement, \U/\L/\u/\l case regions.
- **Editing:** replace mode overwrites text; dot repeats recorded insert/operator key sequences; visual block d/y/c/case operations; insert <C-n>/<C-p> keyword completion plus <C-x><C-u>/<C-x><C-o> completefunc/omnifunc providers; :normal with ranges; indent()/cindent()/shiftwidth()/col()/line() builtins.
- **Ex modifiers:** :silent/:silent!, :noautocmd, :keepjumps, :filter, :tab, :vertical; global-local :set/:setlocal/:setglobal values; :setf/:setfiletype.
- **Persistent undo:** +persistent_undo, 'undofile'/'undodir', the full undo node tree is saved on write/exit and restored with its current node on open.
- **Viminfo:** :wviminfo/:rviminfo round-trip registers, command/search history, file marks and :oldfiles/v:oldfiles; writes merge with existing entries using timestamped register bar lines; newer external registers/file marks win and command/search histories are timestamp-ordered; Number/String/Bool/List/Dict/Blob/Special and named/partial/simple-lambda Funcref g: variables plus the listed buffer list persist; the TUI reads after opening files and writes on exit, honoring 'viminfo'/'viminfofile' and -u NONE.
- **Shell:** shell/shellcmdflag honored; shellquote and shellxquote apply in Vim's order (shellquote wraps the command, shellxquote wraps the whole thing incl. redirection) with cmd.exe-style '^' shellxescape only for "("; 'shelltemp' filters use private 0600 temp files connected with shellredir while 'noshelltemp' uses pipes (has('filterpipe') is 1); range filters (:1,3!cmd), v:shell_error, :redir to files/registers/variables; %/# file modifiers, escaped \%/\#, and !! reuse in :! commands.
- **History:** :history, histadd()/histdel()/histget()/histnr() for command and search history.
- **Mappings:** mapping timeout honors 'timeout'/'timeoutlen'; :normal applies mappings while :normal! bypasses them; <buffer>, <nowait>, <silent>, <unique> and <expr> attributes are parsed and honored.
- **Completion:** getcompletion() covers command/function/event/option/tag/buffer/highlight/environment/mapping/compiler/color/variable/file plus syntax/filetype/scriptnames/runtime/shellcmd; wildmenu respects pumheight/pumwidth and wildmenumode() reports popup state.
- **Buffers:** special buffer types nofile/nowrite/acwrite/quickfix/help/prompt, with prompt_setprompt()/prompt_setcallback()/prompt_setinterrupt() and Enter/CTRL-C callback dispatch that restores the prompt line; listed/unlisted state, :bfirst/:blast/:bmodified navigation, :bufdo/:windo/:tabdo/:argdo with target restoration, :badd/:bufload/:bdelete/:bwipeout/:bunload with BufAdd/BufDelete/BufUnload/BufWipeout events; :ls omits unlisted buffers, :ls! includes them; 'hidden' and 'bufhidden' hide/unload/delete/wipe are honored when a buffer is abandoned, with E37 for modified buffers; the default (empty) bufhidden unloads unmodified hidden buffers.
- **Indent:** cindent control-word indentation, cinoptions >/e/n/}/^/=/:/{/f/L/+/c/C/g/h/N/E/i/(/b/#/l/J/p amounts, 'cinkeys' closing-brace reindent, lisp parentheses indentation, shiftwidth=0 falls back to tabstop, expandtab-aware indents.
- **Sessions/views:** :mksession restores tab order/current tab, per-window buffers and cursors; :mkview/:loadview write sourceable Vim scripts; cursor()/col()/line() host builtins support them.
- **CLI/startup:** has()/exists('+option') feature-matrix checks, :verbose command prefix, -p/-o/-O tab and split layouts, -u NONE/NORC/DEFAULTS, --cmd before startup files, real --startuptime, :version/--version feature matrix, :set termcap terminal diagnostics.
- **Diagnostics:** `--log FILE` and `--log-categories` record timestamped `input`, `ex`, `viml`, `render`, `runtime`, `job` and `js` events (job/js reserved for P3 subsystems); `-V[LEVEL][FILE]` sets 'verbose' and logs to FILE; panics append a crash report with version/location to the log file or `$TMPDIR/rvim-crash-<pid>.log`.
- **Startup security:** user vimrc discovery (~/.vimrc, ~/.vim/vimrc, $XDG_CONFIG_HOME/vim/vimrc) with defaults.vim fallback, 'exrc' loading of .vimrc/.exrc from the current directory, 'secure' mode rejecting shell/write/autocmd commands (E12) and P_SECURE options, and the :sandbox modifier rejecting the same unsafe commands (E48).
- **Statusline:** multi-row per-window and global statuslines via 'statuslineopt=maxheight:N' and %@ newlines, with fixedheight support.
- **Display lines:** gj/gk/g0/g$/gm move by wrapped display lines using the same showbreak/breakindent geometry as rendering.
- **Testing/tooling:** 80x24, 120x40, cursor, wide-character, popup and per-cell attribute/palette golden snapshots; deterministic randomized regex/syntax fuzz smoke test; libFuzzer/cargo-fuzz targets under fuzz/ (regex, syntax, VimL) that compile with `cargo check --manifest-path fuzz/Cargo.toml`, plus a scheduled weekly fuzz workflow; std-only benchmark targets under crates/rvim-headless/benches covering startup, frame render, large-file open, regex scans and syntax highlighting.
- **VimL/regex:** =~, matchstr(), match(), substitute() and :catch use the full Vim regex engine (lookaround including \@N limits, \zs/\ze, backrefs, classes). Vim's syntax atoms are supported: \z( groups with cross-pattern \z1 references in :syntax region end/skip patterns, \%[ optional sequences, \& conjunction, \%( groups (including a redundant backslash as in \%\(), \%d/\%o/\%x/\%u/\%U character escapes, \%#=N engine selectors, \Z, and contextual \% atoms (compiled as never-matching because the crate has no editor context). Multi-byte class members are approximated by their UTF-8 bytes and reverse class ranges are treated as literals, matching Vim's permissive runtime behaviour.
- **Syntax:** multi-line regions terminate correctly on later lines; matchgroup delimiter highlighting, oneline regions, nextgroup chaining, contains=/containedin= filtering across multi-line and same-line regions, contained-only items, and option-token filtering are supported. Region `start=`/`end=`/`skip=` options are tokenised like Vim's skip_regexp(), so delimiters and spaces inside `[...]` collections or patterns do not break parsing; `\z(...)` captures are substituted into dynamic end patterns.

Still missing or partial in P1:

- Completion contexts cover buffers/functions/variables/syntax/compiler/colorscheme; some Vim cmdwin-specific conveniences (type column, cmdwin insert-mode start) are not reproduced.
- :ptselect lists candidates and previews the first match; the interactive select-then-preview step and :psearch are not implemented.
- Full shell quoting matrix and remaining errorformat corner cases (%D/%X directory stacks are supported).
- Vim9-script runtime files are out of scope and reported as `E319` (compiler/context.vim and several newer syntax files such as 2html, context, csv, hare, help, hlsl, java and tsv).
- `syntax/glimmer.vim` only loads after a JavaScript/TypeScript syntax file has set `b:current_syntax`; sourcing it standalone fails in Vim too, so the Tier C check is environment dependent.
- The basic autoload loader covers script-level autoload files; deeper autoload and full legacy VimL semantics are still incomplete.
- Concealment remains P2 (`has('conceal')` is 0); the conceal options are
  accepted and stored so runtime files load. Folding is implemented, see the
  P2 section.
- Package after/ ordering and full startup-file ordering; :packadd/:packloadall and tags completion are present.
- Closure-capturing Lambda Shada values (simple lambdas, named/partial Funcref, Number/String/Bool/List/Dict/Blob/Special and buffer lists are persisted). multi-level persistent undo, viminfo startup/shutdown and basic :mksession/:mkview/:loadview are present.
- Remaining niche C++/Java cinoptions items; wrapped-line mouse mapping, wheel/chunk-level and partial-column smoothscroll are supported. keyword, completefunc and omnifunc completion plus indentexpr/cindent/cinkeys/lisp/showbreak/breakindent/display-line motions/chunk smoothscroll with cursor mapping are supported.
- Remaining widget hit-region actions; per-window/global multi-row statuslines, statusline/tabline clicks and draggable split borders are present.
- Longer continuous fuzzing runs; libFuzzer targets, a scheduled weekly workflow, deterministic fuzz smoke tests and text/popup/wide-char/attribute golden snapshots are present.

## P2 - display subsystems

Implemented and exercised by tests:

- **Folding (DSP-01/02/03):** window-local fold state with `'foldmethod'`
  manual, indent, marker and expr (including `foldexpr` evaluation with
  `v:lnum` and Vim's string results such as `>1`/`<1`/`a1`/`s1`/`=`),
  `'foldignore'`/`'foldnestmax'`/`'foldminlines'`, `'foldlevel'` and
  `'foldenable'`, fold text via the default `foldtext()` implementation or a
  custom `'foldtext'` expression with `v:foldstart`/`v:foldend`/
  `v:folddashes`/`v:foldlevel`, `zf`/`zo`/`zc`/`za`/`zO`/`zC`/
  `zR`/`zM`/`zr`/`zm`/`zd`/`zD`/`zE`/`zi`/`zn`/`zN`/`zj`/
  `zk`/`[z`/`]z`, `:fold`/`:foldopen`/`:foldclose`/`:folddoopen`/
  `:folddoclosed`, `foldclosed()`/`foldclosedend()`/`foldlevel()`/
  `foldtext()`/`foldtextresult()`, `'foldcolumn'` rendering with Vim's
  fold-column characters and click-to-toggle, fold-aware cursor motion,
  scrolling and mouse mapping, and fold text rows rendered with `Folded` and
  `'fillchars'` `fold`/`foldopen`/`foldclose`/`foldsep` entries.
  `has('folding')` is 1.
- **Signs (DSP-04):** `:sign define/undefine/place/unplace/list/jump` with
  groups and priorities, `sign_define()`/`sign_undefine()`/
  `sign_getdefined()`/`sign_place()`/`sign_unplace()`/
  `sign_getplaced()`/`sign_jump()`/`sign_placelist()`/
  `sign_unplacelist()`, text/texthl/linehl/numhl attributes, `'signcolumn'`
  no/yes/auto/number rendering (including signs inside the number column),
  sign clicks, and placements anchored to stable line ids so they follow edits
  and undo. `has('signs')` is 1.

Still approximated or missing in P2:

- `'foldmethod'` `syntax` and `diff` are accepted but produce no folds
  (syntax fold regions and diff mode are not implemented yet).
- `'foldopen'`/`'foldclose'` are stored but do not automatically open folds
  after jumps or search; opening a fold requires `zo`/`za` or a click.
- Folds are recomputed when the buffer or fold options change; there is no
  incremental fold update for very large files, and manual folds are not yet
  written by `:mksession`/`:mkview`.
- `zf` works in Normal mode (and through `:fold`); Visual-mode `zf` and the
  count forms of `zF` are not implemented. `'fillchars'` fold characters are
  assumed to be single-cell characters.
- Sign icons/images are parsed but not drawn (no GUI/image API); only the first
  sign of a line is displayed, and `culhl` is accepted but not rendered.

Diff, text properties, spell, sessions, jobs and QuickJS remain P2/P3 work.

## Verification

Last green run:

~~~text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
~~~

At the time of writing the workspace has 525 tests and no clippy warnings.
The optional PTY smoke script is tools/pty_smoke.py; some sandboxes deny
openpty, so it is not part of the required CI gate yet.

## Known intentional deviations

- Vim9 script, terminal emulator, image API and GUI are out of scope.
- Generated option defaults are parsed from the pinned source where possible;
  a handful of feature-dependent defaults are approximated.
- The editor's internal text is UTF-8, but regex and buffer APIs remain
  byte-oriented.  Multi-byte characters inside regex collections are therefore
  approximated by their UTF-8 bytes, and context-dependent regex atoms
  (`\%#`, `\%V`, line/column atoms) compile but never match.
- No runtime tree is vendored; tests use temporary runtime trees.
- Folding approximates a few Vim details: fold levels are recomputed from the
  whole buffer instead of incrementally, `foldopen`/`foldclose` do not open
  folds after jumps, and the `syntax`/`diff` fold methods produce no folds.
- Sign placement is anchored with stable line ids; deleting the anchored line
  removes the sign, as in Vim. Icons and `culhl` are not rendered.
