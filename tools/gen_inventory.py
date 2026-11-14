#!/usr/bin/env python3
"""Generate completeness inventories for RVim from a Vim runtime tree.

The generated files are reference appendices, not the curated feature catalog.
They are produced mechanically so that "complete" can be checked and refreshed
against a newer Vim runtime.

Usage:
    python3 tools/gen_inventory.py --runtime /usr/share/vim/vim91
    python3 tools/gen_inventory.py --runtime /path/to/vim/runtime \
        --label "Vim 9.2.1338" --out docs/inventory
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import sys
from collections import defaultdict
from datetime import date, datetime, timezone
from pathlib import Path

VIM9_COMMANDS = {
    "abstract", "class", "def", "defcompile", "disassemble", "endclass",
    "enddef", "endenum", "endinterface", "enum", "export", "final", "import",
    "interface", "static", "type", "var", "vim9cmd", "vim9script",
}
EX_LEGACY_COMMANDS = {"append", "change", "insert", "open"}
NON_EVENT_TAGS = {
    "autocmd", "autocmd-define", "autocmd-events", "autocmd-events-abc",
    "autocmd-execute", "autocmd-groups", "autocmd-intro", "autocmd-nested",
    "autocmd-pattern", "autocmd-patterns", "autocmd-search-pat",
    "autocmd.txt", "autocommand", "Command-line",
}


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def one_line(text: str, limit: int | None = None) -> str:
    text = re.sub(r"\s+", " ", text).strip().replace("|", r"\|")
    if limit and len(text) > limit:
        text = text[: limit - 1].rstrip() + "..."
    return text


def md_cell(text: str) -> str:
    return one_line(text)


def code(text: str) -> str:
    return "<code>" + text + "</code>"


def doc_ref(tag: str) -> str:
    return code(":h " + tag)


def version_from_runtime(runtime: Path) -> str:
    for name in ("version9.txt", "version8.txt", "version7.txt"):
        p = runtime / "doc" / name
        if p.exists():
            m = re.search(r"Vim version\s+([0-9.]+)", read(p))
            if m:
                return m.group(1)
    return "unknown"


def parse_commands(index_text: str) -> list[dict]:
    start = index_text.find("*ex-cmd-index*")
    if start < 0:
        raise SystemExit("could not find *ex-cmd-index* in index.txt")
    rows: list[dict] = []
    current: dict | None = None
    for line in index_text[start:].splitlines():
        m = re.match(r"^\s*\|(:[A-Za-z0-9_]+)\|\s+(\S+)\s*(.*)$", line)
        if m:
            tag, spec, desc = m.group(1), m.group(2), m.group(3).strip()
            name = tag[1:]
            current = {
                "name": name, "spec": spec, "doc": tag, "description": desc,
                "vim9": name in VIM9_COMMANDS,
                "legacy": name in EX_LEGACY_COMMANDS,
            }
            rows.append(current)
            continue
        if current is not None and line.strip() and not line.lstrip().startswith("|"):
            cont = line.strip()
            if cont.endswith("~"):
                cont = cont[:-1].rstrip()
            if not current["description"]:
                current["description"] = cont
            elif len(current["description"]) < 180:
                current["description"] += " " + cont
    seen, unique = set(), []
    for r in rows:
        if r["name"] in seen:
            continue
        seen.add(r["name"])
        unique.append(r)
    return unique


def parse_keys(index_text: str) -> list[dict]:
    start = index_text.find("*insert-index*")
    end = index_text.find("*ex-cmd-index*")
    if start < 0 or end < 0:
        raise SystemExit("could not find mode-key sections in index.txt")
    rows: list[dict] = []
    section = "1. Insert mode"
    subsection = ""
    current: dict | None = None
    for line in index_text[start:end].splitlines():
        hm = re.match(r"^(\d+(?:\.\d+)?)\.\s+([^\t]+?)\s*\*", line)
        if hm:
            num, title = hm.group(1), hm.group(2).strip()
            if "." not in num:
                section, subsection = f"{num}. {title}", ""
            else:
                subsection = f"{num} {title}"
            current = None
            continue
        m = re.match(r"^\s*\|([^|]+)\|\s*(.*)$", line)
        if m:
            tag, rest = m.group(1), m.group(2).strip()
            if not rest:
                current = None
                continue
            pieces = re.split(r"\t+| {2,}", rest, maxsplit=2)
            key = pieces[0].strip()
            count, desc = "", ""
            if len(pieces) >= 2:
                if re.fullmatch(r"\d+(?:,\d+)*", pieces[1].strip()):
                    count = pieces[1].strip()
                    if len(pieces) >= 3:
                        desc = pieces[2].strip()
                else:
                    desc = " ".join(pieces[1:]).strip()
            if not key:
                current = None
                continue
            current = {
                "tag": tag, "key": key, "count": count, "description": desc,
                "mode": section, "group": subsection,
            }
            rows.append(current)
            continue
        if current is not None and line.strip() and line.startswith((" ", "\t")):
            if not line.lstrip().startswith("|") and len(current["description"]) < 220:
                current["description"] = (current["description"] + " " + line.strip()).strip()
    seen, unique = set(), []
    for r in rows:
        k = (r["tag"], r["key"])
        if k in seen:
            continue
        seen.add(k)
        unique.append(r)
    return unique


def parse_options(text: str) -> list[dict]:
    rows = []
    pat = r"^'([a-z]+)'(?: '([a-z]+)')?\s+(boolean|number|string)\s+(.*)$"
    for m in re.finditer(pat, text, re.M):
        long_name, short_name, typ, default = m.groups()
        default = re.sub(r"\*[^*]+\*", "", default).strip()
        rows.append({
            "name": long_name, "short": short_name or "", "type": typ,
            "default": one_line(default, 120), "doc": "'" + long_name + "'",
        })
    return rows


def parse_functions(doc_dir: Path) -> list[dict]:
    found: dict[str, str] = {}
    for path in sorted(doc_dir.glob("*.txt")):
        for name in re.findall(r"\*([a-z][a-zA-Z0-9_]*)\(\)\*", read(path)):
            if name not in found or path.name == "builtin.txt":
                found[name] = path.name
    return [{"name": n, "doc": n + "()", "file": found[n]} for n in sorted(found)]


def parse_events(text: str) -> list[dict]:
    rows: dict[str, dict] = {}
    lines = text.splitlines()
    for i, line in enumerate(lines):
        stripped = line.strip()
        if not stripped.startswith("*"):
            continue
        if re.sub(r"\*[A-Za-z][A-Za-z0-9_]*\*", "", stripped).strip():
            continue
        tags = re.findall(r"\*([A-Za-z][A-Za-z0-9_]*)\*", stripped)
        for tag in tags:
            if tag in NON_EVENT_TAGS or not tag[0].isupper() or tag in rows:
                continue
            desc = []
            for follow in lines[i + 1:i + 6]:
                if not follow.strip():
                    break
                if re.fullmatch(r"\s*(?:\*[A-Za-z][A-Za-z0-9_]*\*\s*)+", follow):
                    break
                desc.append(follow.strip())
            text_desc = one_line(" ".join(desc), 240)
            names = [t for t in tags if t[0].isupper()]
            changed = True
            while changed and text_desc:
                changed = False
                for name in names:
                    if text_desc.startswith(name + " "):
                        text_desc = text_desc[len(name) + 1:]
                        changed = True
                        break
                for prefix in ("or ", ", "):
                    if text_desc.startswith(prefix):
                        text_desc = text_desc[len(prefix):]
                        changed = True
            rows[tag] = {"name": tag, "doc": tag, "description": one_line(text_desc, 180)}
    return [rows[k] for k in sorted(rows)]


def parse_features(text: str) -> list[dict]:
    start, end = text.find("*feature-list*"), text.find("*string-match*")
    if start < 0 or end < 0:
        return []
    rows: list[dict] = []
    current: dict | None = None
    in_code = False
    for line in text[start:end].splitlines():
        if line.strip().startswith(">"):
            in_code = True
            continue
        if in_code:
            if line.strip() == "<":
                in_code = False
            continue
        m = re.match(r"^([a-z][a-z0-9_]*)(\t+| {2,})(.+)$", line)
        if m:
            name, desc = m.group(1), m.group(3).strip()
            if name in {"there", "example", "note", "features", "hint", "this"}:
                current = None
                continue
            current = {"name": name, "description": desc}
            rows.append(current)
            continue
        if current is not None and line.startswith((" ", "\t")) and line.strip():
            current["description"] += " " + line.strip()
        elif not line.strip():
            current = None
    seen, unique = set(), []
    for r in rows:
        if r["name"] in seen:
            continue
        seen.add(r["name"])
        r["description"] = one_line(r["description"], 180)
        unique.append(r)
    return unique


def parse_help_tags(doc_dir: Path) -> int:
    p = doc_dir / "tags"
    return sum(1 for line in read(p).splitlines() if line.strip()) if p.exists() else 0


def is_test_fixture(path: Path) -> bool:
    """Test fixtures are shipped in the source tree but never loaded at runtime."""
    return "testdir" in path.parts


def runtime_surface(runtime: Path, out_dir: Path) -> dict:
    dirs = ["syntax", "ftplugin", "indent", "colors", "compiler", "keymap",
            "autoload", "plugin", "pack", "spell", "doc", "import", "macros",
            "print", "tutor", "tools", "lang"]
    counts, sizes = {}, {}
    for d in dirs:
        p = runtime / d
        files = [f for f in p.rglob("*") if f.is_file() and not is_test_fixture(f.relative_to(p))] if p.is_dir() else []
        counts[d] = len(files)
        sizes[d] = sum(f.stat().st_size for f in files)

    def stems(sub: str) -> set[str]:
        p = runtime / sub
        return {f.stem for f in p.glob("*.vim")} if p.is_dir() else set()

    syntax, ftplugin, indent = stems("syntax"), stems("ftplugin"), stems("indent")
    languages = sorted(syntax | ftplugin | indent)
    with (out_dir / "runtime-languages.csv").open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["language", "syntax", "ftplugin", "indent"])
        for lang in languages:
            w.writerow([lang, int(lang in syntax), int(lang in ftplugin), int(lang in indent)])

    def names(sub: str) -> list[str]:
        p = runtime / sub
        return sorted(f.stem for f in p.glob("*.vim")) if p.is_dir() else []

    packs = []
    pack_root = runtime / "pack"
    if pack_root.is_dir():
        packs = sorted(str(p.relative_to(runtime)) for p in pack_root.glob("*/*/*") if p.is_dir())

    vim9_files = []
    for f in runtime.rglob("*.vim"):
        if is_test_fixture(f.relative_to(runtime)):
            continue
        try:
            with f.open("r", encoding="utf-8", errors="replace") as fh:
                first = fh.readline().strip()
        except OSError:
            continue
        if first == "vim9script" or first.startswith("vim9script "):
            vim9_files.append(str(f.relative_to(runtime)))

    return {
        "counts": counts,
        "sizes": sizes,
        "languages": {
            "total": len(languages), "full_triad": len(syntax & ftplugin & indent),
            "syntax_only": len(syntax - ftplugin - indent), "syntax": len(syntax),
            "ftplugin": len(ftplugin), "indent": len(indent),
        },
        "plugins": names("plugin"), "colors": names("colors"),
        "compilers": names("compiler"), "packs": packs,
        "vim9_files": sorted(vim9_files), "help_tags": parse_help_tags(runtime / "doc"),
    }


def write_commands(rows: list[dict], out: Path, label: str) -> None:
    lines = [
        "# Ex command inventory", "",
        "Source: " + label + ". Generated by tools/gen_inventory.py; do not edit.", "",
        "Every Ex command tag in runtime/doc/index.txt. The curated selection",
        "units are in [../FEATURES.md](../FEATURES.md). Vim9 is deferred with",
        "Vim9 script; Ex-legacy marks Vi/Ex text-entry commands.",
        "", "| Command | Form | Description | Flags | Help |", "|---|---|---|---|---|",
    ]
    for r in rows:
        flags = "Vim9" if r["vim9"] else ("Ex-legacy" if r["legacy"] else "")
        lines.append("| " + code(":" + r["name"]) + " | " + code(r["spec"]) + " | " +
                     md_cell(r["description"]) + " | " + flags + " | " + doc_ref(r["doc"]) + " |")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_keys(rows: list[dict], out: Path, label: str) -> None:
    groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for r in rows:
        groups[(r["mode"], r["group"])].append(r)
    lines = [
        "# Mode-key inventory", "",
        "Source: " + label + ". Generated by tools/gen_inventory.py; do not edit.", "",
        "Every tagged key in the Insert, Normal, Visual, Command-line and",
        "Terminal-Job mode indexes, including operator-pending/text-object keys.",
        "",
    ]
    for (mode, group), items in groups.items():
        title = mode + ((" -- " + group) if group else "")
        lines += ["## " + title, "", "| Key | Count | Action | Help |", "|---|---|---|---|"]
        for r in items:
            lines.append("| " + code(md_cell(r["key"])) + " | " + r["count"] + " | " +
                         md_cell(r["description"]) + " | " + doc_ref(r["tag"]) + " |")
        lines.append("")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_options(rows: list[dict], out: Path, label: str) -> None:
    lines = [
        "# Option inventory", "",
        "Source: " + label + ". Generated by tools/gen_inventory.py; do not edit.", "",
        "Every option definition in runtime/doc/options.txt.", "",
        "| Option | Short | Type | Default (documented) | Help |",
        "|---|---|---|---|---|",
    ]
    for r in rows:
        short = code(r["short"]) if r["short"] else ""
        lines.append("| " + code("'" + r["name"] + "'") + " | " + short + " | " +
                     r["type"] + " | " + md_cell(r["default"]) + " | " + doc_ref(r["doc"]) + " |")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_functions(rows: list[dict], out: Path, label: str) -> None:
    lines = [
        "# Built-in function inventory", "",
        "Source: " + label + ". Generated by tools/gen_inventory.py; do not edit.", "",
        "Every *name()* tag in the runtime documentation, including popup,",
        "terminal, channel, text-property and sign functions documented outside",
        "builtin.txt.", "",
        "| Function | Documented in | Help |", "|---|---|---|",
    ]
    for r in rows:
        lines.append("| " + code(r["name"] + "()") + " | " + code(r["file"]) + " | " + doc_ref(r["doc"]) + " |")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_events(rows: list[dict], out: Path, label: str) -> None:
    lines = [
        "# Autocommand event inventory", "",
        "Source: " + label + ". Generated by tools/gen_inventory.py; do not edit.", "",
        "Every event tag in runtime/doc/autocmd.txt.", "",
        "| Event | Summary | Help |", "|---|---|---|",
    ]
    for r in rows:
        lines.append("| " + code(r["name"]) + " | " + md_cell(r["description"]) + " | " + doc_ref(r["doc"]) + " |")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_features(rows: list[dict], out: Path, label: str) -> None:
    lines = [
        "# has() feature inventory", "",
        "Source: " + label + ". Generated by tools/gen_inventory.py; do not edit.", "",
        "Every feature name in :h feature-list. RVim must return honest values;",
        "runtime scripts use these checks to branch. quickjs is an RVim extension",
        "not present in Vim and is listed in the catalog, not in this appendix.", "",
        "| Feature | Meaning | Help |", "|---|---|---|",
    ]
    for r in rows:
        lines.append("| " + code(r["name"]) + " | " + md_cell(r["description"]) + " | " + doc_ref("feature-list") + " |")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_runtime_surface(surface: dict, out: Path, label: str) -> None:
    c, lang = surface["counts"], surface["languages"]
    lines = [
        "# Runtime surface", "",
        "Source: " + label + ". Generated by tools/gen_inventory.py; do not edit.", "",
        "This is the size of the compatibility target, not a claim that all of it",
        "must work in the first milestone. See [../RUNTIME-COMPAT.md](../RUNTIME-COMPAT.md)",
        "for the tiering model. Test fixtures under <code>testdir/</code> are excluded;",
        "they are not loaded by the editor.", "", "## File counts", "",
        "| Directory | Files | Bytes |", "|---|---:|---:|",
    ]
    for d in c:
        lines.append("| " + code(d + "/") + " | " + format(c[d], ",") + " | " + format(surface["sizes"][d], ",") + " |")
    lines += [
        "", "## Language support files", "",
        "- Syntax files: **" + format(lang["syntax"], ",") + "**",
        "- Filetype plugins: **" + format(lang["ftplugin"], ",") + "**",
        "- Indent files: **" + format(lang["indent"], ",") + "**",
        "- Languages with all three: **" + format(lang["full_triad"], ",") + "**",
        "- Syntax-only languages: **" + format(lang["syntax_only"], ",") + "**", "",
        "Per-language matrix: runtime-languages.csv.", "",
        "## Shipped plugins", "", ", ".join(code(p) for p in surface["plugins"]) or "(none)", "",
        "## Optional packages", "", ", ".join(code(p) for p in surface["packs"]) or "(none)", "",
        "## Colorschemes", "", ", ".join(code(p) for p in surface["colors"]) or "(none)", "",
        "## Compilers", "",
        str(len(surface["compilers"])) + " definitions: " + ", ".join(code(p) for p in surface["compilers"]), "",
        "## Help", "", "- doc/tags entries: **" + format(surface["help_tags"], ",") + "**", "",
        "## Vim9-script runtime files", "",
        "- Files whose first line is vim9script: **" + str(len(surface["vim9_files"])) + "**",
        "- Skipped with a warning under the default Vim9-deferred policy.", "",
        "<details><summary>List</summary>", "",
    ]
    lines += ["- " + code(p) for p in surface["vim9_files"]]
    lines += ["", "</details>", ""]
    out.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runtime", required=True, type=Path)
    ap.add_argument("--out", default=Path("docs/inventory"), type=Path)
    ap.add_argument("--label", default="")
    args = ap.parse_args()

    runtime = args.runtime.resolve()
    doc = runtime / "doc"
    if not (doc / "index.txt").exists():
        print("error: " + str(doc / "index.txt") + " not found", file=sys.stderr)
        return 2
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)

    label = args.label or ("Vim " + version_from_runtime(runtime) + " (local snapshot)")
    index_text = read(doc / "index.txt")
    commands = parse_commands(index_text)
    keys = parse_keys(index_text)
    options = parse_options(read(doc / "options.txt"))
    functions = parse_functions(doc)
    events = parse_events(read(doc / "autocmd.txt"))
    features = parse_features(read(doc / "builtin.txt"))
    surface = runtime_surface(runtime, out)

    write_commands(commands, out / "commands.md", label)
    write_keys(keys, out / "keys.md", label)
    write_options(options, out / "options.md", label)
    write_functions(functions, out / "functions.md", label)
    write_events(events, out / "events.md", label)
    write_features(features, out / "features.md", label)
    write_runtime_surface(surface, out / "runtime-surface.md", label)

    inventory = {
        "meta": {
            "label": label, "runtime": str(runtime),
            "generated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "generator": "tools/gen_inventory.py",
            "counts": {
                "commands": len(commands), "keys": len(keys), "options": len(options),
                "functions": len(functions), "events": len(events), "features": len(features),
            },
        },
        "commands": commands, "keys": keys, "options": options, "functions": functions,
        "events": events, "features": features, "runtime": surface,
    }
    (out / "inventory.json").write_text(json.dumps(inventory, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")

    readme = """# Generated Vim inventories

Source: **""" + label + """**  
Generated: """ + date.today().isoformat() + """ by tools/gen_inventory.py.

These files are mechanically extracted from the unmodified Vim runtime tree.
They make "complete" auditable and support per-item coverage tracking. Test
fixtures under <code>testdir/</code> are excluded; they are not loaded by the
editor. The curated, selectable feature list is [../FEATURES.md](../FEATURES.md).

| Inventory | Count | File |
|---|---:|---|
| Ex commands | """ + format(len(commands), ",") + """ | [commands.md](commands.md) |
| Mode keys (insert/normal/visual/cmdline/terminal) | """ + format(len(keys), ",") + """ | [keys.md](keys.md) |
| Options | """ + format(len(options), ",") + """ | [options.md](options.md) |
| Built-in functions | """ + format(len(functions), ",") + """ | [functions.md](functions.md) |
| Autocommand events | """ + format(len(events), ",") + """ | [events.md](events.md) |
| has() features | """ + format(len(features), ",") + """ | [features.md](features.md) |
| Runtime surface | -- | [runtime-surface.md](runtime-surface.md) |

Machine-readable form: [inventory.json](inventory.json).

The terminal emulator (<code>:terminal</code>, <code>term_*</code> functions and
Terminal-Job mode) is descoped by project decision. Its commands, functions,
options and keys remain in these mechanical inventories so the gap is visible
and auditable; they are not part of the active catalog.

## Regenerate

    python3 tools/gen_inventory.py --runtime /path/to/vim/runtime \\
      --label "Vim 9.2.1338" --out docs/inventory

The checked-in snapshot is generated from the pinned runtime named in
[../RUNTIME-COMPAT.md](../RUNTIME-COMPAT.md); never edit generated files by hand.
Review the diff. New commands/options/functions/events need a catalog decision:
in scope, deferred, or excluded.
"""
    (out / "README.md").write_text(readme, encoding="utf-8")

    print("wrote " + str(out))
    for k, v in inventory["meta"]["counts"].items():
        print("  " + k.ljust(12) + str(v).rjust(5))
    print("  vim9 files  " + str(len(surface["vim9_files"])).rjust(5))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
