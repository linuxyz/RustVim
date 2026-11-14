#!/usr/bin/env python3
"""Runtime compatibility tiers (RT-11).

Runs the pinned Vim runtime tree against the RVim binary and records how much
of each compatibility tier loads without errors:

* Tier A: core editing/help/syntax for common languages.
* Tier B: common plugins/colorschemes/compilers.
* Tier C: every syntax file in the runtime tree.

The report is written as Markdown and JSON.  --gate makes Tier A failures fail
the process so CI can use it as a release gate.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ERROR_RE = re.compile(r"\bE\d+:")

TIER_A_LANGUAGES = {
    "rust": "rs",
    "python": "py",
    "c": "c",
    "cpp": "cpp",
    "javascript": "js",
    "json": "json",
    "markdown": "md",
    "vim": "vim",
    "sh": "sh",
    "html": "html",
    "css": "css",
    "yaml": "yaml",
    "toml": "toml",
}


def run_rvim(rvim: Path, runtime: Path, args: list[str], timeout: float = 20.0) -> tuple[int, str]:
    env = dict(os.environ)
    env["VIMRUNTIME"] = str(runtime)
    try:
        result = subprocess.run(
            [str(rvim), "--headless", *args],
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
        return result.returncode, (result.stdout + result.stderr)
    except subprocess.TimeoutExpired:
        return 124, "TIMEOUT"
    except OSError as error:
        return 127, str(error)


def check(output: str) -> str | None:
    for line in output.splitlines():
        match = ERROR_RE.search(line)
        if match:
            return line.strip()[:200]
    return None


def tier_a(rvim: Path, runtime: Path, work: Path) -> list[dict]:
    results = []
    code, output = run_rvim(rvim, runtime, ["-u", "NONE", "-c", "help syntax"])
    missing_help = "E149" in output or "E484" in output
    results.append(
        {
            "check": "help syntax",
            "ok": not missing_help,
            "error": "E149" if missing_help else None,
        }
    )
    for language, extension in TIER_A_LANGUAGES.items():
        sample = work / f"sample.{extension}"
        sample.write_text(
            {
                "rust": "fn main() { let x = 1; }\n",
                "python": "def f(x):\n    return x + 1\n",
                "c": "int main(void) { return 0; }\n",
                "cpp": "int main() { return 0; }\n",
                "javascript": "const x = 1;\n",
                "json": '{"a": 1}\n',
                "markdown": "# title\n",
                "vim": "let g:x = 1\n",
                "sh": "echo hello\n",
                "html": "<html><body>x</body></html>\n",
                "css": "body { color: red; }\n",
                "yaml": "a: 1\n",
                "toml": "a = 1\n",
            }[language],
            encoding="utf-8",
        )
        code, output = run_rvim(
            rvim,
            runtime,
            ["-u", "NONE", "-c", "syntax on", "-c", "set ft?", str(sample)],
        )
        error = check(output)
        results.append(
            {
                "check": f"syntax:{language}",
                "ok": error is None,
                "error": error,
            }
        )
    return results


def tier_b(rvim: Path, runtime: Path) -> list[dict]:
    results = []
    colors = sorted((runtime / "colors").glob("*.vim"))
    for path in colors:
        name = path.stem
        code, output = run_rvim(
            rvim, runtime, ["-u", "NONE", "-c", f"colorscheme {name}"]
        )
        error = check(output)
        results.append({"check": f"colorscheme:{name}", "ok": error is None, "error": error})
    compilers = sorted((runtime / "compiler").glob("*.vim"))[:20]
    for path in compilers:
        name = path.stem
        code, output = run_rvim(
            rvim, runtime, ["-u", "NONE", "-c", f"compiler {name}"]
        )
        error = check(output)
        results.append({"check": f"compiler:{name}", "ok": error is None, "error": error})
    return results


def tier_c(rvim: Path, runtime: Path, limit: int) -> list[dict]:
    results = []
    syntax_files = sorted((runtime / "syntax").glob("*.vim"))
    if limit > 0:
        syntax_files = syntax_files[:limit]
    for path in syntax_files:
        name = path.stem
        code, output = run_rvim(
            rvim,
            runtime,
            ["-u", "NONE", "-c", f"source {path}"],
            timeout=15.0,
        )
        error = check(output)
        results.append({"check": f"syntax-file:{name}", "ok": error is None, "error": error})
    return results


def summarize(tier: str, results: list[dict]) -> dict:
    passed = sum(1 for item in results if item["ok"])
    failures = [item for item in results if not item["ok"]]
    return {
        "tier": tier,
        "total": len(results),
        "passed": passed,
        "failed": len(failures),
        "failures": failures,
    }


def write_report(out_dir: Path, summaries: list[dict], runtime: Path, rvim: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "runtime": str(runtime),
        "binary": str(rvim),
        "generated": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "tiers": summaries,
    }
    (out_dir / "compat-report.json").write_text(json.dumps(report, indent=2) + "\n")
    lines = [
        "# Runtime compatibility report",
        "",
        f"Generated: {report['generated']}",
        f"Runtime: {runtime}",
        f"Binary: {rvim}",
        "",
        "| Tier | Passed | Total | Failures |",
        "|---|---|---|---|",
    ]
    for summary in summaries:
        lines.append(
            f"| {summary['tier']} | {summary['passed']} | {summary['total']} | {summary['failed']} |"
        )
    lines.append("")
    lines.append("## Failure details")
    for summary in summaries:
        if not summary["failures"]:
            lines.append(f"- Tier {summary['tier']}: no failures")
            continue
        lines.append(f"### Tier {summary['tier']}")
        for failure in summary["failures"][:50]:
            lines.append(f"- {failure['check']}: {failure['error']}")
    (out_dir / "compat-report.md").write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", default=os.environ.get("VIMRUNTIME", ""))
    parser.add_argument("--rvim", default="target/debug/rvim")
    parser.add_argument("--out-dir", default="target/compat")
    parser.add_argument("--tiers", default="a,b,c")
    parser.add_argument("--limit-c", type=int, default=0)
    parser.add_argument("--gate", action="store_true")
    args = parser.parse_args()
    runtime = Path(args.runtime)
    rvim = Path(args.rvim)
    if not runtime.is_dir():
        print(f"runtime not found: {runtime}", file=sys.stderr)
        return 2
    if not rvim.exists():
        print(f"binary not found: {rvim}", file=sys.stderr)
        return 2
    tiers = {part.strip() for part in args.tiers.split(",") if part.strip()}
    summaries: list[dict] = []
    with tempfile.TemporaryDirectory(prefix="rvim-compat-") as temporary:
        work = Path(temporary)
        if "a" in tiers:
            summaries.append(summarize("A", tier_a(rvim, runtime, work)))
        if "b" in tiers:
            summaries.append(summarize("B", tier_b(rvim, runtime)))
        if "c" in tiers:
            summaries.append(summarize("C", tier_c(rvim, runtime, args.limit_c)))
    write_report(Path(args.out_dir), summaries, runtime, rvim)
    for summary in summaries:
        print(
            f"Tier {summary['tier']}: {summary['passed']}/{summary['total']} passed, "
            f"{summary['failed']} failures"
        )
    if args.gate:
        tier_a_summary = next((item for item in summaries if item["tier"] == "A"), None)
        if tier_a_summary is None:
            print("gate: Tier A was not run", file=sys.stderr)
            return 2
        if tier_a_summary["failed"]:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
