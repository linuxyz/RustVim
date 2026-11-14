#!/usr/bin/env python3
"""Generate option metadata extras from a pinned Vim source tree."""
from __future__ import annotations
import json, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
source = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("/tmp/rvim-vim/vim-9.2.1338")
inventory = json.loads((ROOT / "docs/inventory/inventory.json").read_text())
options = inventory["options"]
header = (source / "src/optiondefs.h").read_text()

macros: dict[str, str] = {}
for match in re.finditer(r"^#\s*define\s+PV_(\w+)\s+(.+)$", header, re.M):
    macros[match.group(1)] = match.group(2).strip()


def scope_of_macro(name: str) -> str:
    seen: set[str] = set()
    while name not in seen:
        seen.add(name)
        value = macros.get(name, "")
        if not value:
            return "Global"
        if "OPT_BOTH" in value:
            return "GlobalLocalWindow" if "OPT_WIN" in value else "GlobalLocalBuffer"
        if "OPT_BUF" in value:
            return "Buffer"
        if "OPT_WIN" in value:
            return "Window"
        inner = re.search(r"PV_(\w+)", value)
        if inner:
            name = inner.group(1)
            continue
        return "Global"
    return "Global"

# Features this build really has.  Anything left undefined makes the option
# default pointer NULL in Vim's own table; we turn that into an explicit
# availability gate instead of pretending the option works.
DEFINES = {
    "UNIX", "MACOS_X", "HAVE_FSYNC", "HAVE_CLIPMETHOD", "CURSOR_SHAPE",
    "ELAPSED_FUNC", "FEAT_EVAL", "FEAT_QUICKFIX", "FEAT_SYN_HL",
    "FEAT_LINEBREAK", "FEAT_STL_OPT", "FEAT_WRITEBACKUP", "FEAT_DIGRAPHS", "FEAT_COMPL_FUNC",
}

FEATURE_NAMES = {
    "FEAT_GUI": "gui", "FEAT_GUI_GTK": "gui", "FEAT_GUI_MSWIN": "gui",
    "FEAT_GUI_X11": "gui", "FEAT_GUI_TABLINE": "gui", "FEAT_XFONTSET": "gui",
    "FEAT_XIM": "xim", "FEAT_MENU": "menu", "FEAT_TOOLBAR": "toolbar",
    "FEAT_BEVAL": "balloon_eval", "FEAT_BEVAL_GUI": "balloon_eval",
    "FEAT_BEVAL_TERM": "balloon_eval", "FEAT_TERMINAL": "terminal",
    "FEAT_CSCOPE": "cscope", "FEAT_PYTHON": "python",
    "FEAT_PYTHON3": "python3", "FEAT_RUBY": "ruby", "FEAT_PERL": "perl",
    "FEAT_TCL": "tcl", "FEAT_LUA": "lua", "FEAT_MZSCHEME": "mzscheme",
    "FEAT_NETBEANS_INTG": "netbeans_intg", "FEAT_PRINTER": "printer",
    "FEAT_POSTSCRIPT": "postscript", "FEAT_PRINT_PANGO": "printer",
    "FEAT_ARABIC": "arabic", "FEAT_RIGHTLEFT": "rightleft",
    "FEAT_SPELL": "spell", "FEAT_FOLDING": "folding", "FEAT_DIFF": "diff",
    "FEAT_SESSION": "session", "FEAT_VIMINFO": "viminfo",
    "FEAT_PERSISTENT_UNDO": "persistent_undo", "FEAT_VARTABS": "vartabs",
    "FEAT_CONCEAL": "conceal", "FEAT_PROP_POPUP": "textprop",
    "FEAT_TABPANEL": "tabpanel", "FEAT_WAYLAND": "wayland",
    "FEAT_TERMGUICOLORS": "termguicolors", "FEAT_SIGNS": "signs",
    "FEAT_IMAGE": "image", "FEAT_WAK": "waks",
    "FEAT_XCLIPBOARD": "xterm_clipboard", "FEAT_SEARCH_EXTRA": "search_extra",
    "FEAT_RELTIME": "reltime", "FEAT_RENDER_OPTIONS": "render_options",
    "FEAT_MOUSESHAPE": "mouseshape", "FEAT_BROWSE": "browse",
    "FEAT_GUI_DIALOG": "gui", "FEAT_CON_DIALOG": "con_dialog",
    "FEAT_DIGRAPHS": "digraphs", "FEAT_EMACS_TAGS": "emacs_tags",
    "FEAT_KEYMAP": "keymap", "FEAT_LANGMAP": "langmap",
    "FEAT_MULTI_LANG": "multi_lang", "FEAT_FIND_ID": "find_id",
    "FEAT_COMPL_FUNC": "compl_func", "FEAT_AUTOCHDIR": "autochdir",
    "FEAT_AUTOSHELLDIR": "autoshelldir", "FEAT_CRYPT": "crypt",
}

MACRO_VALUES = {
    "CTRL_F_STR": "\x06",
    "ISK_LATIN1": "@,48-57,_,192-255",
    "ISP_LATIN1": "@,161-255",
    "DFLT_BDIR": ".,~/tmp,~/",
    "DFLT_DIR": ".,~/tmp,/var/tmp,/tmp",
    "DFLT_ERRORFILE": "errors.err",
    "DFLT_GREPFORMAT": "%f:%l:%m,%f:%l%m,%f  %l%m",
    "DFLT_FO_VI": "vt",
    "DFLT_FO_VIM": "tcq",
    "CPO_VI": "aAbBcCdDeEfFgHiIjJkKlLmMnoOpPqrRsStuvwWxXyZz$!%*-+<>;",
    "CPO_VIM": "aABceFsz",
    "B_IMODE_USE_INSERT": -1,
    "Ctrl_E": 5,
    "TAB": 9,
    "ENC_DFLT": "utf-8",
    "B_IMODE_NONE": 0,
    "DFLT_EFM": "%*[^\"]\"%f\"%*\\D%l: %m,\"%f\"%*\\D%l: %m,%-Gg%\\?make[%*\\d]: *** [%f:%l:%m,%-Gg%\\?make: *** [%f:%l:%m,%-G%f:%l: (Each undeclared identifier is reported only once,%-G%f:%l: for each function it appears in.),%-GIn file included from %f:%l:%c:,%-GIn file included from %f:%l:%c\\,,%-GIn file included from %f:%l:%c,%-GIn file included from %f:%l,%-G%*[ ]from %f:%l:%c,%-G%*[ ]from %f:%l:,%-G%*[ ]from %f:%l\\,,%-G%*[ ]from %f:%l,%f:%l:%c:%m,%f(%l):%m,%f:%l:%m,\"%f\"\\, line %l%*\\D%c%*[^ ] %m,%D%*\\a[%*\\d]: Entering directory %*[`']%f',%X%*\\a[%*\\d]: Leaving directory %*[`']%f',%D%*\\a: Entering directory %*[`']%f',%X%*\\a: Leaving directory %*[`']%f',%DMaking %*\\a in %f,%f|%l| %m",
}

def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def eval_cond(expr: str) -> bool:
    expr = strip_comments(expr).strip()
    expr = re.sub(
        r"defined\s*\(\s*(\w+)\s*\)",
        lambda m: "1" if m.group(1) in DEFINES else "0",
        expr,
    )
    expr = re.sub(
        r"\bdefined\s+(\w+)",
        lambda m: "1" if m.group(1) in DEFINES else "0",
        expr,
    )
    expr = re.sub(
        r"\b[A-Za-z_]\w*\b",
        lambda m: "1" if m.group(0) in DEFINES else "0",
        expr,
    )
    expr = expr.replace("&&", " and ").replace("||", " or ")
    expr = re.sub(r"!(?!=)", " not ", expr)
    try:
        return bool(eval(expr, {"__builtins__": {}}, {}))
    except Exception:
        return False


def preprocess(lines: list[str]) -> str:
    out: list[str] = []
    stack: list[list[bool]] = []
    active = True
    for line in lines:
        match = re.match(
            r"\s*#\s*(ifdef|ifndef|if|elif|else|endif)\b(.*)", line
        )
        if not match:
            if active:
                out.append(line)
            continue
        kind, rest = match.group(1), match.group(2)
        if kind in ("ifdef", "ifndef", "if"):
            if kind == "ifdef":
                cond = rest.strip() in DEFINES
            elif kind == "ifndef":
                cond = rest.strip() not in DEFINES
            else:
                cond = eval_cond(rest)
            stack.append([active, cond, cond])
            active = active and cond
        elif kind == "elif":
            if stack:
                parent, _, taken = stack[-1]
                cond = (not taken) and eval_cond(rest)
                stack[-1][1] = cond
                stack[-1][2] = taken or cond
                active = parent and cond
        elif kind == "else":
            if stack:
                parent, _, taken = stack[-1]
                cond = not taken
                stack[-1][1] = cond
                stack[-1][2] = True
                active = parent and cond
        elif kind == "endif":
            if stack:
                parent, _, _ = stack.pop()
                active = parent
    return "\n".join(out)


def c_string(value: str) -> str:
    out: list[str] = []
    i = 0
    while i < len(value):
        if value[i] == "\\" and i + 1 < len(value):
            i += 1
            ch = value[i]
            mapping = {"n": "\n", "t": "\t", "r": "\r", "\\": "\\", '"': '"'}
            out.append(mapping.get(ch, ch))
        else:
            out.append(value[i])
        i += 1
    return "".join(out)


# Named defaults whose definitions live in option.h/optiondefs.h.
_option_h = (source / "src/option.h").read_text()
for _macro in ("LISPWORD_VALUE",):
    _match = re.search(
        r'#\s*define\s+' + _macro + r'\s+"((?:\\.|[^"\\])*)"', _option_h
    )
    if _match:
        MACRO_VALUES[_macro] = c_string(_match.group(1))
_highlight = re.search(
    r'#\s*define\s+HIGHLIGHT_INIT\s+"((?:\\.|[^"\\])*)"', header
)
if _highlight:
    MACRO_VALUES["HIGHLIGHT_INIT"] = c_string(_highlight.group(1))


def eval_default(expr: str):
    expr = expr.strip()
    expr = re.sub(r"^\(char_u \*\)\s*", "", expr).strip()
    if expr in ("NULL", "0", "0L"):
        return None
    if expr == "TRUE":
        return True
    if expr == "FALSE":
        return False
    if re.fullmatch(r"-?\d+L?", expr):
        return int(expr.rstrip("L"))
    if re.fullmatch(r'"(?:\\.|[^"\\])*"', expr, re.S):
        return c_string(expr[1:-1])
    macro = re.fullmatch(r'N_\(\s*("(?:\\.|[^"\\])*")\s*\)', expr, re.S)
    if macro:
        return c_string(macro.group(1)[1:-1])
    if expr in MACRO_VALUES:
        return MACRO_VALUES[expr]
    return Ellipsis


def feature_for(raw_block: str) -> str | None:
    features = []
    for cond in re.findall(r"#\s*(?:ifdef|ifndef|if|elif)\s+([^\n]*)", raw_block):
        for macro in re.findall(r"FEAT_[A-Z0-9_]+", cond):
            name = FEATURE_NAMES.get(macro)
            if name and macro not in DEFINES and name not in features:
                features.append(name)
    if features:
        return features[0]
    return "unsupported"


blocks: dict[str, tuple[str, str, str]] = {}
for part in re.split(r"(?=^\s*\{\"[^\"]+\"\s*,)", header, flags=re.M):
    raw = part
    match = re.match(r'\s*\{"([^"]+)",\s*(?:"([^"]*)"|NULL),\s*([^,]+),', part, re.M)
    if match:
        blocks[match.group(1)] = (match.group(2) or "", match.group(3), raw)


def rust_string(value: str) -> str:
    text = json.dumps(value, ensure_ascii=False)
    return re.sub(r"\\u([0-9a-fA-F]{4})", r"\\u{\1}", text)


def fallback_default(option: dict, kind: str):
    text = option.get("default", "")
    if kind == "Bool":
        low = text.lower()
        if "default off" in low or "default: off" in low:
            return ("Bool", False)
        if "default on" in low or "default: on" in low:
            return ("Bool", True)
        return ("Bool", False)
    if kind == "Number":
        match = re.search(r"default[^0-9-]*(-?\d+)", text, re.I)
        return ("Number", int(match.group(1)) if match else 0)
    match = re.search(r'default[^"]*"([^"]*)"', text, re.I)
    return ("Str", match.group(1) if match else "")


# Environment/runtime-derived defaults that Vim fills in at startup.  Keeping a
# stable compile-time value here is better than copying one machine's $TMPDIR.
DEFAULT_OVERRIDES = {
    "backupskip": ("Str", ""),
    "helpfile": ("Str", "$VIMRUNTIME/doc/help.txt"),
    "maxmem": ("Number", 0),
    "maxmemtot": ("Number", 0),
    "packpath": ("Str", ""),
    "runtimepath": ("Str", ""),
    "scroll": ("Number", 0),
    "window": ("Number", 0),
}

core_names: set[str] = set()
core_path = ROOT / "crates/rvim-editor/src/options.rs"
if core_path.exists():
    core = core_path.read_text()
    core_names = set(re.findall(r'spec!?\(\s*"([^"]+)"', core))

lines = [
    "//! Generated option metadata for Vim 9.2.1338.",
    "//! Generated by tools/gen_options.py; do not edit.",
    "use crate::options::{DefaultValue, OptionScope, OptionSpec, OptionType};",
    "",
    "pub static EXTRA_SPECS: &[OptionSpec] = &[",
]
count = 0
seen: set[str] = set()
unresolved: list[str] = []
for option in sorted(options, key=lambda value: value["name"]):
    name = option["name"]
    if name in core_names or name in seen:
        continue
    seen.add(name)
    info = blocks.get(name)
    short = option.get("short", "")
    kind = {"boolean": "Bool", "number": "Number", "string": "String"}.get(
        option.get("type", "boolean"), "Bool"
    )
    default = fallback_default(option, kind)
    feature = None
    scope = "Global"
    if info:
        short = info[0] or short
        block = preprocess(info[2].splitlines())
        flags_match = re.match(
            r'\s*\{"[^"]+",\s*(?:"[^"]*"|NULL),\s*([^,]+),', block, re.S
        )
        flags = re.sub(r"\s+", " ", flags_match.group(1) if flags_match else info[1]).strip()
        if "P_NUM" in flags:
            kind = "Number"
        elif "P_STRING" in flags:
            kind = "String"
        pv = re.search(r"PV_([A-Z0-9_]+)", block)
        if pv and pv.group(1) != "NONE":
            scope = scope_of_macro(pv.group(1))
        pointer_match = re.search(r"^\s*\(char_u \*\)\s*([^,\n]+)", block, re.M)
        pointer = pointer_match.group(1).strip() if pointer_match else "NULL"
        if pointer == "NULL":
            feature = feature_for(info[2])
        default = fallback_default(option, kind)
        casts = [m.start() for m in re.finditer(r"\(char_u \*\)", block)]
        if len(casts) >= 3:
            values = []
            for start in casts[-2:]:
                tail = block[start:]
                match = re.match(r"\(char_u \*\)\s*(?:\(long\))?\s*(" + r'"(?:\\.|[^"\\])*"' + r"|-?\d+L?|TRUE|FALSE|NULL|N_\([^)]*\)|[A-Za-z_]\w*)", tail, re.S)
                values.append(eval_default(match.group(1)) if match else Ellipsis)
            vi_default, vim_default = values
            chosen = vim_default if vim_default is not None else vi_default
            if chosen is Ellipsis:
                unresolved.append(name)
            elif chosen is not None:
                if kind == "Bool" and isinstance(chosen, bool):
                    default = ("Bool", chosen)
                elif kind == "Number" and isinstance(chosen, int):
                    default = ("Number", chosen)
                elif kind == "String" and isinstance(chosen, str):
                    default = ("Str", chosen)
    if name in DEFAULT_OVERRIDES:
        default = DEFAULT_OVERRIDES[name]
    if default[0] == "Str":
        default_text = rust_string(default[1])
    elif default[0] == "Bool":
        default_text = "true" if default[1] else "false"
    else:
        default_text = str(default[1])
    feature_text = f'Some({rust_string(feature)})' if feature else "None"
    lines.append(
        f'    OptionSpec {{ name: {rust_string(name)}, short: {rust_string(short)}, '
        f'kind: OptionType::{kind}, scope: OptionScope::{scope}, '
        f'default: DefaultValue::{default[0]}({default_text}), doc: {rust_string(chr(39) + name + chr(39))}, '
        f'flags: {rust_string(flags if info else "")}, feature: {feature_text} }},'
    )
    count += 1
lines.append("];")
(ROOT / "crates/rvim-editor/src/options_extra.rs").write_text("\n".join(lines) + "\n")
print(f"generated {count} extra option specs")
if unresolved:
    print("unresolved defaults:", ", ".join(unresolved), file=sys.stderr)
