#!/usr/bin/env python3
"""Select a dependency-closed feature backlog from docs/FEATURES.md.

Examples:
    python3 tools/select_features.py --ids EDT-10,REGEX-02 --with-deps
    python3 tools/select_features.py --phase P0
    python3 tools/select_features.py --areas VIML,FUN,REGEX --max-phase P3 --with-deps
    python3 tools/select_features.py --bundle walking --format ids
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import catalog as catlib

BUNDLES = {
    "walking": {
        "phases": ["P0"],
        "desc": "Walking skeleton: headless editor, UI contract, TUI, basic editing.",
    },
    "daily": {
        "phases": ["P0", "P1"],
        "desc": "Daily-driver editor: undo, visual, search, mapping, VimL core, syntax, runtime.",
    },
    "scripting": {
        "areas": ["VIML", "FUN", "REGEX", "JS", "JOB"],
        "max_phase": "P3",
        "desc": "Scripting stack: legacy VimL, builtins, regex, QuickJS, jobs.",
    },
    "runtime": {
        "areas": ["RT", "TAG", "SYN", "QF", "SPELL"],
        "max_phase": "P3",
        "desc": "Vim runtime compatibility: loaders, help/tags, syntax, quickfix, spell.",
    },
}


def phase_rank(p: str) -> int:
    return catlib.PHASES.index(p)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", default="", help="comma-separated feature IDs")
    ap.add_argument("--areas", default="", help="comma-separated area prefixes")
    ap.add_argument("--phase", default="", help="exact phase, e.g. P0")
    ap.add_argument("--max-phase", default="P3", help="include phases up to this one")
    ap.add_argument("--bundle", choices=sorted(BUNDLES), default="")
    ap.add_argument("--with-deps", action="store_true", help="include transitive dependencies")
    ap.add_argument("--format", choices=["markdown", "ids", "yaml"], default="markdown")
    ap.add_argument("--file", default="docs/FEATURES.md")
    args = ap.parse_args()

    cat = catlib.parse(args.file)
    by_id = cat.by_id
    chosen: set[str] = set()

    if args.ids:
        for raw in args.ids.replace(" ", "").split(","):
            if not raw:
                continue
            if raw not in by_id:
                print("error: unknown feature " + raw, file=sys.stderr)
                return 2
            chosen.add(raw)

    if args.bundle:
        b = BUNDLES[args.bundle]
        phases = set(b.get("phases", []))
        areas = set(b.get("areas", []))
        max_phase = b.get("max_phase", "P3")
        for f in cat.features:
            if areas and f.area not in areas:
                continue
            if phases and f.phase not in phases:
                continue
            if not phases and phase_rank(f.phase) > phase_rank(max_phase):
                continue
            chosen.add(f.id)

    if args.areas:
        areas = {a.strip().upper() for a in args.areas.split(",") if a.strip()}
        for f in cat.features:
            if f.area in areas:
                chosen.add(f.id)

    if args.phase:
        for f in cat.features:
            if f.phase == args.phase:
                chosen.add(f.id)

    if not args.ids and not args.bundle and not args.areas and not args.phase:
        print("error: choose --ids, --areas, --phase or --bundle", file=sys.stderr)
        return 2

    if args.with_deps:
        features = catlib.closure(cat, sorted(chosen))
        added = [f.id for f in features if f.id not in chosen]
    else:
        features = [by_id[i] for i in catlib.topo_order(cat, chosen)]
        added = []

    if args.format == "ids":
        print("\n".join(f.id for f in features))
        return 0

    if args.format == "yaml":
        print("features:")
        for f in features:
            print(f"  - id: {f.id}")
            print(f"    title: {f.title!r}")
            print(f"    phase: {f.phase}")
            print(f"    complexity: {f.complexity}")
            print(f"    deps: [{', '.join(f.deps)}]")
        return 0

    print("# Selected backlog")
    print()
    print(f"{len(features)} features, dependency order.")
    if added:
        print(f"{len(added)} dependencies added automatically.")
    print()
    print("| ID | Feature | Phase | Cx | Deps | Scope |")
    print("|---|---|---|---|---|---|")
    for f in features:
        scope = f.scope.replace("|", "\\|")
        print(f"| {f.id} | {f.title} | {f.phase} | {f.complexity} | {', '.join(f.deps) or '—'} | {scope} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
