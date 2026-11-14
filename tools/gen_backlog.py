#!/usr/bin/env python3
"""Generate the P0+P1 implementation backlog from catalog/features.json."""
from __future__ import annotations
import json
from pathlib import Path

# Status and evidence live in catalog/status.json so that regenerating this
# file never resets hand-reviewed implementation state.

ROOT = Path(__file__).resolve().parent.parent
cat = json.loads((ROOT / "catalog/features.json").read_text())
status = json.loads((ROOT / "catalog/status.json").read_text())["features"]
features = [f for f in cat["features"] if f["phase"] in ("P0", "P1")]
order = {"P0": 0, "P1": 1}
features.sort(key=lambda f: (order[f["phase"]], f["area"], f["id"]))
lines = [
    "# P0+P1 implementation backlog",
    "",
    "Generated from [catalog/features.json](../catalog/features.json). This is the",
    "implementation contract for the 177 P0/P1 features; status is tracked here and",
    "in [STATUS.md](STATUS.md).",
    "",
    f"Total: **{len(features)}** features "
    f"(P0 {sum(f['phase']=='P0' for f in features)}, "
    f"P1 {sum(f['phase']=='P1' for f in features)}).",
    "",
    "Status values: pending, in-progress, done, blocked. A feature is only",
    "done when its scope is implemented, tested, and the catalog definition of",
    "done is satisfied. Status records the concrete remaining gap when a feature",
    "is in progress; status is maintained in catalog/status.json.",
    "",
]
counts = {}
for f in features:
    s = status.get(f["id"], {}).get("status", "pending")
    counts[s] = counts.get(s, 0) + 1
lines.append(
    "Status: "
    + ", ".join(f"**{counts[k]}** {k}" for k in ("done", "in-progress", "pending", "blocked") if counts.get(k))
    + "."
)
lines.append("")
areas = {}
for f in features:
    areas.setdefault(f["area"], []).append(f)
for area, rows in areas.items():
    lines += [f"## {area} - {rows[0]['area_name']}", "", "| ID | Feature | Ph | Cx | Deps | Status | Evidence / remaining gap |", "|---|---|---|---|---|---|---|"]
    for f in rows:
        deps = ", ".join(f.get("deps", [])) or "-"
        st = status.get(f["id"], {})
        note = (st.get("evidence", "") or "-").replace("|", "\\|")
        lines.append(
            f"| {f['id']} | {f['title']} | {f['phase']} | {f['complexity']} | {deps} "
            f"| {st.get('status', 'pending')} | {note} |"
        )
    lines.append("")
    lines.append("Scope notes:")
    for f in rows:
        scope = f["scope"].replace("|", "\\|")
        lines.append(f"- {f['id']}: {scope}")
    lines.append("")
(ROOT / "docs/BACKLOG-P0-P1.md").write_text("\n".join(lines))
print(f"wrote docs/BACKLOG-P0-P1.md: {len(features)} features, {len(areas)} areas")