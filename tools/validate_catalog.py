#!/usr/bin/env python3
"""Validate docs/FEATURES.md and emit catalog/features.json."""

from __future__ import annotations

import argparse
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import catalog as catlib


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--file", default="docs/FEATURES.md")
    ap.add_argument("--json", default="catalog/features.json")
    args = ap.parse_args()

    cat = catlib.parse(args.file)
    errors = []
    if not cat.features:
        errors.append("no feature rows found")
    ids = [f.id for f in cat.features]
    for fid, n in Counter(ids).items():
        if n > 1:
            errors.append(f"duplicate id {fid} ({n})")
    missing = catlib.missing_deps(cat)
    for fid, deps in missing.items():
        errors.append(f"{fid} depends on unknown: {', '.join(deps)}")
    cycles = catlib.find_cycles(cat)
    for cycle in cycles:
        errors.append("dependency cycle: " + " -> ".join(cycle))
    inv = catlib.phase_inversions(cat)
    for fid, phase, dep, dep_phase in inv:
        errors.append(f"phase inversion: {fid} ({phase}) depends on {dep} ({dep_phase})")

    # Balanced inline code spans per line.
    for i, line in enumerate(Path(args.file).read_text(encoding="utf-8").splitlines(), 1):
        if line.count("§") % 2:
            errors.append(f"unbalanced placeholder at line {i}")

    if errors:
        print("FAIL")
        for e in errors:
            print(" - " + e)
        return 1

    out = catlib.write_json(cat, args.json)
    counts = cat.counts()
    print("OK: " + str(counts["total"]) + " features")
    print("  phases: " + " ".join(f"{p}={counts['by_phase'].get(p, 0)}" for p in catlib.PHASES))
    print("  areas:  " + " ".join(f"{a}={n}" for a, n in sorted(counts["by_area"].items())))
    print("  json:   " + str(out))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
