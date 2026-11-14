#!/usr/bin/env python3
"""Lightweight documentation checks: links, fences, inline code tags."""

from __future__ import annotations

import re
import sys
from pathlib import Path

LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
FENCE = chr(96) * 3
TILDE_FENCE = "~~~"
ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    problems = []
    files = [p for p in ROOT.rglob("*.md") if ".git" not in p.parts]
    for path in files:
        text = path.read_text(encoding="utf-8", errors="replace")
        fences = [
            line for line in text.splitlines()
            if line.startswith(FENCE) or line.startswith(TILDE_FENCE)
        ]
        if len(fences) % 2:
            problems.append("%s: odd number of code fences (%d)" % (path, len(fences)))
        if text.count("<code>") != text.count("</code>"):
            problems.append("%s: unbalanced <code> tags" % path)
        if chr(167) in text:
            problems.append("%s: section-sign placeholder remains" % path)
        if "TODO" in text or "TBD" in text:
            problems.append("%s: TODO/TBD remains" % path)
        for match in LINK_RE.finditer(text):
            target = match.group(1).split("#")[0]
            if not target or target.startswith(("http://", "https://", "mailto:")):
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                problems.append("%s: broken link %s" % (path, match.group(1)))
    print("checked %d markdown files" % len(files))
    if problems:
        print("PROBLEMS")
        for p in problems:
            print(" - " + p)
        return 1
    print("no link/fence/tag problems")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
