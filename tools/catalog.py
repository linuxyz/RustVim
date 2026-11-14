"""Parse and validate the RVim feature catalog in docs/FEATURES.md."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path

FEATURE_RE = re.compile(r"^\|\s*([A-Z][A-Z0-9]*-\d+)\s*\|")
HEADING_RE = re.compile(r"^##\s+([A-Z][A-Z0-9]*)\s+(?:—|--)\s+(.+?)\s*$")
PHASES = ["P0", "P1", "P2", "P3", "P4"]
COMPLEXITIES = ["S", "M", "L", "XL"]


@dataclass
class Feature:
    id: str
    title: str
    phase: str
    complexity: str
    deps: list[str]
    scope: str
    area: str = ""
    area_name: str = ""

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "area": self.area,
            "area_name": self.area_name,
            "title": self.title,
            "phase": self.phase,
            "complexity": self.complexity,
            "deps": self.deps,
            "scope": self.scope,
        }


@dataclass
class Catalog:
    features: list[Feature] = field(default_factory=list)
    areas: dict[str, str] = field(default_factory=dict)

    @property
    def by_id(self) -> dict[str, Feature]:
        return {f.id: f for f in self.features}

    def counts(self) -> dict:
        by_phase = {p: 0 for p in PHASES}
        by_area = {}
        for f in self.features:
            by_phase[f.phase] = by_phase.get(f.phase, 0) + 1
            by_area[f.area] = by_area.get(f.area, 0) + 1
        return {"total": len(self.features), "by_phase": by_phase, "by_area": by_area}


def parse(path: Path | str = "docs/FEATURES.md") -> Catalog:
    path = Path(path)
    text = path.read_text(encoding="utf-8")
    cat = Catalog()
    area = ""
    area_name = ""
    for line in text.splitlines():
        hm = HEADING_RE.match(line)
        if hm:
            area, area_name = hm.group(1), hm.group(2)
            cat.areas[area] = area_name
            continue
        if not FEATURE_RE.match(line):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 6:
            raise ValueError(f"bad table row ({len(cells)} cells): {line[:160]}")
        fid, title, phase, cx, dep_text, scope = cells
        if phase not in PHASES:
            raise ValueError(f"bad phase for {fid}: {phase}")
        if cx not in COMPLEXITIES:
            raise ValueError(f"bad complexity for {fid}: {cx}")
        deps = [] if dep_text in ("", "-", "—") else [d.strip() for d in dep_text.split(",") if d.strip()]
        cat.features.append(Feature(fid, title, phase, cx, deps, scope, area, area_name))
    return cat


def find_cycles(cat: Catalog) -> list[list[str]]:
    by_id = cat.by_id
    WHITE, GRAY, BLACK = 0, 1, 2
    color = {fid: WHITE for fid in by_id}
    stack: list[str] = []
    cycles: list[list[str]] = []

    def visit(node: str) -> None:
        color[node] = GRAY
        stack.append(node)
        for dep in by_id[node].deps:
            if dep not in by_id:
                continue
            if color[dep] == GRAY:
                i = stack.index(dep)
                cycles.append(stack[i:] + [dep])
            elif color[dep] == WHITE:
                visit(dep)
        stack.pop()
        color[node] = BLACK

    for fid in by_id:
        if color[fid] == WHITE:
            visit(fid)
    return cycles


def missing_deps(cat: Catalog) -> dict[str, list[str]]:
    by_id = cat.by_id
    return {f.id: [d for d in f.deps if d not in by_id] for f in cat.features if any(d not in by_id for d in f.deps)}


def phase_inversions(cat: Catalog) -> list[tuple[str, str, str, str]]:
    by_id = cat.by_id
    rank = {p: i for i, p in enumerate(PHASES)}
    out = []
    for f in cat.features:
        for d in f.deps:
            if d in by_id and rank[by_id[d].phase] > rank[f.phase]:
                out.append((f.id, f.phase, d, by_id[d].phase))
    return out


def closure(cat: Catalog, ids: list[str]) -> list[Feature]:
    by_id = cat.by_id
    selected: set[str] = set()
    missing = [i for i in ids if i not in by_id]
    if missing:
        raise KeyError("unknown feature ids: " + ", ".join(missing))

    def add(fid: str) -> None:
        if fid in selected:
            return
        selected.add(fid)
        for d in by_id[fid].deps:
            add(d)

    for i in ids:
        add(i)
    return [by_id[i] for i in topo_order(cat, selected)]


def topo_order(cat: Catalog, subset: set[str] | None = None) -> list[str]:
    by_id = cat.by_id
    subset = subset or set(by_id)
    rank = {p: i for i, p in enumerate(PHASES)}
    order = {f.id: i for i, f in enumerate(cat.features)}
    result: list[str] = []
    seen: set[str] = set()

    def visit(fid: str) -> None:
        if fid in seen:
            return
        seen.add(fid)
        for d in by_id[fid].deps:
            if d in subset:
                visit(d)
        result.append(fid)

    for fid in sorted(subset, key=lambda x: (rank[by_id[x].phase], order[x])):
        visit(fid)
    return result


def write_json(cat: Catalog, path: Path | str = "catalog/features.json") -> Path:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    data = {
        "target": "Vim 9.2.1338",
        "source": "docs/FEATURES.md",
        "counts": cat.counts(),
        "areas": cat.areas,
        "features": [f.to_dict() for f in cat.features],
    }
    path.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    return path
