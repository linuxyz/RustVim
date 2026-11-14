#!/usr/bin/env python3
"""Vim9 differential-testing harness and corpus driver.

The Vim9 front end is exercised through the example binary

    cargo run -q -p rvim-viml --example vim9_parse -- <file.vim>

which prints exactly OK and exits 0 when the file parses, or prints
'error: E<code>: <message> (line N)' on stderr and exits 1 when it does not.

The corpus lives in tools/vim9_corpus:

* positive/*.vim - valid Vim9 scripts.  Each one starts with a
  'vim9script' header, exercises one language area and ends with
  :echomsg assertions so a real Vim produces useful expected output.
* negative/*.vim - scripts that must be rejected.  A leading comment
  '# expect-error: E<code>' (or '# expect: fail' when the exact code is
  not important) states the expectation.
* support/*.vim - importable modules used by the import/export cases.
* expected/<name>.out - output captured from a real Vim for every
  oracle-checkable positive; the ground truth for the future 'diff' mode.

Subcommands:

* check-corpus - validate corpus hygiene (names, headers, markers,
  expected-output coverage, forbidden side effects).
* parse - run every case through the Rust parser example.
* oracle - capture/verify expected output with a real Vim.  --check
  re-runs the capture and compares without overwriting.
* diff - Phase 2 stub: compares RVim execution with expected/*.out.
  It reports clearly that it is not implemented yet instead of faking a pass.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CORPUS = ROOT / "tools" / "vim9_corpus"
EXAMPLE_SOURCE = ROOT / "crates" / "rvim-viml" / "examples" / "vim9_parse.rs"
PREBUILT_EXAMPLE = ROOT / "target" / "debug" / "examples" / "vim9_parse"
DEFAULT_RVIM = ROOT / "target" / "debug" / "rvim"

ERROR_RE = re.compile(r"\bE\d+:")
EXPECT_ERROR_RE = re.compile(r"^#\s*expect-error:\s*(E\d+)\b", re.MULTILINE)
EXPECT_FAIL_RE = re.compile(r"^#\s*expect:\s*fail\b", re.MULTILINE)
NO_ORACLE_RE = re.compile(r"^#\s*vim9-corpus-no-oracle\b(?::\s*(.*))?$", re.MULTILINE)
VIM9_HEADER_RE = re.compile(r"^vim9script(?:\s|$)")
CASE_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9_]*$")
FORBIDDEN_POSITIVE_TOKENS = ("writefile(", "system(", "job_start(", "term_start(")
CARGO_FAILURE_MARKERS = (
    "could not compile",
    "no example target named",
    "error[E",
    "failed to run custom build command",
    "error: failed to",
)

RESULT_PASS = "PASS"
RESULT_FAIL = "FAIL"
RESULT_SKIP = "SKIP"

# A parser that has not implemented a feature still exits nonzero.  Such output
# must not count as a pass for a negative case (the construct was not actually
# rejected on its merits).
UNIMPLEMENTED_RE = re.compile(r"not implemented|unimplemented", re.IGNORECASE)


@dataclass
class Case:
    """One corpus file plus the expectations encoded in its comments."""

    path: Path
    kind: str  # "positive" | "negative"
    stem: str
    text: str = ""
    expect_code: str | None = None
    expect_fail: bool = False
    no_oracle: bool = False
    no_oracle_reason: str = ""

    @property
    def name(self) -> str:
        return self.path.name


@dataclass
class CaseResult:
    case: Case
    status: str
    detail: str = ""
    output: str = ""


def read_text(path: Path) -> str:
    return path.read_bytes().decode("utf-8", errors="replace")


def analyse_markers(case: Case) -> Case:
    """Fill in parser/oracle expectations from '#' marker comments."""
    match = EXPECT_ERROR_RE.search(case.text)
    if match:
        case.expect_code = match.group(1)
    case.expect_fail = bool(EXPECT_FAIL_RE.search(case.text))
    match = NO_ORACLE_RE.search(case.text)
    if match:
        case.no_oracle = True
        case.no_oracle_reason = (match.group(1) or "").strip()
    return case


def discover_cases(corpus: Path, pattern: str | None = None) -> list[Case]:
    """Return every case under positive/ and negative/, sorted by path."""
    cases: list[Case] = []
    for kind in ("positive", "negative"):
        directory = corpus / kind
        if not directory.is_dir():
            continue
        for path in sorted(directory.glob("*.vim")):
            if pattern and pattern not in path.name:
                continue
            case = Case(path=path, kind=kind, stem=path.stem)
            try:
                case.text = read_text(path)
            except OSError:
                case.text = ""
            cases.append(analyse_markers(case))
    return cases


def resolve_executable(name: str) -> str | None:
    """Resolve a command name or path without invoking a shell."""
    path = Path(name)
    if path.is_absolute() or os.sep in name:
        return str(path) if path.is_file() else None
    return shutil.which(name)


# ---------------------------------------------------------------------------
# parse: exercise the Rust Vim9 parser example
# ---------------------------------------------------------------------------


def example_command(args: argparse.Namespace) -> tuple[list[str] | None, str]:
    """Return the command prefix used to invoke the parser example.

    The contract is 'cargo run -q -p rvim-viml --example vim9_parse -- FILE'.
    A pre-built binary is accepted when the example source has not been
    created yet, and --example-bin overrides both for local experiments.
    """
    if args.example_bin:
        path = Path(args.example_bin).resolve()
        if not path.is_file():
            return None, f"example binary not found: {path}"
        return [str(path)], str(path)
    if EXAMPLE_SOURCE.is_file():
        return (
            ["cargo", "run", "-q", "-p", "rvim-viml", "--example", "vim9_parse", "--"],
            "cargo run -q -p rvim-viml --example vim9_parse",
        )
    if PREBUILT_EXAMPLE.is_file():
        return (
            [str(PREBUILT_EXAMPLE)],
            f"{PREBUILT_EXAMPLE} (pre-built; example source is missing)",
        )
    return None, (
        "vim9_parse example not built yet: no crates/rvim-viml/examples/vim9_parse.rs "
        "and no target/debug/examples/vim9_parse.  Build it with:\n"
        "    cargo build -p rvim-viml --example vim9_parse"
    )


def looks_like_build_failure(returncode: int, combined: str) -> bool:
    if returncode == 0:
        return False
    if combined.lstrip().startswith("error: E"):
        # The parser's own error format; a normal test failure, not a build error.
        return False
    return any(marker in combined for marker in CARGO_FAILURE_MARKERS)


def run_parse_case(case: Case, command_prefix: list[str], timeout: float) -> CaseResult:
    command = [*command_prefix, str(case.path)]
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return CaseResult(case, RESULT_FAIL, f"timed out after {timeout:.0f}s")
    except OSError as error:
        return CaseResult(case, RESULT_FAIL, f"cannot run example: {error}")
    elapsed = time.monotonic() - started
    combined = completed.stdout + completed.stderr
    if looks_like_build_failure(completed.returncode, combined):
        return CaseResult(case, RESULT_FAIL, "BUILD-FAILURE", combined)

    if case.kind == "positive":
        if completed.returncode != 0:
            detail = first_line(combined) or f"exit {completed.returncode}"
            return CaseResult(case, RESULT_FAIL, detail, combined)
        stdout = completed.stdout.strip()
        if stdout != "OK":
            # The contract is exactly "OK".  A trailing debug suffix such as
            # "OK items=27" is tolerated (and reported) so the gate stays
            # usable while the example is being developed.
            if stdout.startswith("OK") and not stdout[2:3].isalnum():
                return CaseResult(
                    case,
                    RESULT_PASS,
                    f"OK (extra stdout: {stdout[2:].strip()!r}, {elapsed:.2f}s)",
                )
            shown = stdout.replace("\n", "\\n")[:120]
            return CaseResult(
                case,
                RESULT_FAIL,
                f"expected 'OK', got stdout {shown!r} (exit {completed.returncode}, {elapsed:.2f}s)",
                combined,
            )
        return CaseResult(case, RESULT_PASS, f"OK ({elapsed:.2f}s)")

    # Negative case: any nonzero exit is a rejection.  An expected E<code>
    # marker additionally has to appear in the parser's combined output.
    if completed.returncode == 0:
        stdout = completed.stdout.strip()[:120]
        return CaseResult(
            case,
            RESULT_FAIL,
            f"parser accepted a script that must fail (stdout={stdout!r})",
            combined,
        )
    if UNIMPLEMENTED_RE.search(combined):
        return CaseResult(
            case,
            RESULT_SKIP,
            f"parser not implemented: {first_line(combined)}",
            combined,
        )
    if case.expect_code and case.expect_code not in combined:
        return CaseResult(
            case,
            RESULT_FAIL,
            f"rejected, but {case.expect_code} not found in output: {first_line(combined)}",
            combined,
        )
    return CaseResult(case, RESULT_PASS, case.expect_code or "rejected", combined)


def first_line(text: str) -> str:
    for line in text.splitlines():
        if line.strip():
            return line.strip()[:200]
    return ""


def print_parse_row(result: CaseResult) -> None:
    case = result.case
    expected = "ok" if case.kind == "positive" else (case.expect_code or "fail")
    print(f"{case.name:<38} {case.kind:<9} {expected:<8} {result.status:<5} {result.detail}")


def cmd_parse(args: argparse.Namespace) -> int:
    corpus = Path(args.corpus)
    cases = discover_cases(corpus, args.filter)
    if not cases:
        print(f"parse: no corpus cases found under {corpus}", file=sys.stderr)
        return 2
    command_prefix, description = example_command(args)
    if not command_prefix:
        print(f"parse: {description}", file=sys.stderr)
        print(
            f"parse: {len(cases)} cases not run; parser acceptance was NOT checked.",
            file=sys.stderr,
        )
        return 2
    print(f"parser example: {description}")

    results: list[CaseResult] = []
    fatal: str | None = None
    for index, case in enumerate(cases):
        if fatal:
            results.append(CaseResult(case, RESULT_SKIP, fatal))
            continue
        if index == 0 and command_prefix[0] == "cargo" and shutil.which("cargo") is None:
            fatal = "cargo not found on PATH"
            results.append(CaseResult(case, RESULT_FAIL, fatal))
            continue
        result = run_parse_case(case, command_prefix, args.timeout)
        if result.detail == "BUILD-FAILURE":
            fatal = "parser example failed to build"
            result = CaseResult(case, RESULT_FAIL, fatal, result.output)
            print(f"parse: {fatal}:", file=sys.stderr)
            print(indent_text(first_lines(result.output, 12)), file=sys.stderr)
            print(
                "parse: fix the example with: cargo build -p rvim-viml --example vim9_parse",
                file=sys.stderr,
            )
        results.append(result)

    print()
    print(f"{'case':<38} {'kind':<9} {'expected':<8} {'result':<5} detail")
    print("-" * 100)
    for result in results:
        print_parse_row(result)

    if args.verbose:
        print()
        for result in results:
            print(f"### {result.case.name} ({result.status})")
            print(f"    file: {result.case.path}")
            if result.output:
                print(indent_text(first_lines(result.output, 40), 4))
            elif result.detail:
                print(f"    {result.detail}")
            print()

    failures = [r for r in results if r.status == RESULT_FAIL]
    skipped = [r for r in results if r.status == RESULT_SKIP]
    passed = len(results) - len(failures) - len(skipped)
    positives = sum(1 for c in cases if c.kind == "positive")
    negatives = len(cases) - positives
    print()
    unimplemented = [r for r in skipped if r.detail.startswith("parser not implemented")]
    print(
        f"parse: {passed} passed, {len(failures)} failed, {len(skipped)} skipped "
        f"({positives} positive, {negatives} negative)"
    )
    if unimplemented:
        print(
            f"parse: {len(unimplemented)} negative cases hit an unimplemented parser "
            "feature; they are NOT counted as passed."
        )
    if args.json:
        write_json(
            args.json,
            {
                "command": "parse",
                "example": description,
                "cases": [
                    {
                        "name": r.case.name,
                        "kind": r.case.kind,
                        "expected": "ok" if r.case.kind == "positive" else (r.case.expect_code or "fail"),
                        "status": r.status,
                        "detail": r.detail,
                    }
                    for r in results
                ],
                "passed": passed,
                "failed": len(failures),
                "skipped": len(skipped),
            },
        )
        print(f"parse: wrote {args.json}")
    if fatal:
        return 2
    return 1 if failures else 0


# ---------------------------------------------------------------------------
# oracle: capture expected output from real Vim
# ---------------------------------------------------------------------------


@dataclass
class VimCapture:
    returncode: int
    stdout: str
    stderr: str
    output: str | None
    timed_out: bool = False


def vim_source_path(path: Path) -> str:
    """Escape a path for use as the argument of an Ex command."""
    text = str(path)
    return (
        text.replace("\\", "\\\\")
        .replace(" ", "\\ ")
        .replace("|", "\\|")
        .replace('"', '\\"')
        .replace("#", "\\#")
        .replace("%", "\\%")
    )


def vim_single_quoted(text: str) -> str:
    return "'" + text.replace("'", "''") + "'"


def vim_version(vim: str) -> str:
    try:
        completed = subprocess.run(
            [vim, "--version"],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=20,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return f"unknown ({error})"
    lines = completed.stdout.splitlines()
    version = lines[0].strip() if lines else "unknown"
    patches = next((line.strip() for line in lines if "patches:" in line.lower()), "")
    return f"{version} ({patches})" if patches else version


def capture_with_vim(vim: str, script: Path, out_path: Path, timeout: float) -> VimCapture:
    """Source script in headless Vim and write captured messages to out_path.

    The invocation is deliberately explicit: no vimrc, no swap file, no
    viminfo, silent Ex mode and no terminal.  :redir collects everything the
    script emits through :echo/:echomsg (including error messages, so a
    failing script is visible) and :writefile stores it byte-for-byte.
    """
    command = [
        vim,
        "-Nu", "NONE",
        "-n",
        "-es",
        "-i", "NONE",
        "--not-a-term",
        "-c", "set encoding=utf-8",
        "-c", "set nomore",
        "-c", "redir => g:vim9_corpus_output",
        "-c", f"source {vim_source_path(script)}",
        "-c", "redir END",
        "-c",
        "call writefile(split(g:vim9_corpus_output, \"\\n\"), "
        + vim_single_quoted(str(out_path))
        + ")",
        "-c", "qa!",
    ]
    try:
        completed = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return VimCapture(127, "", "", None, timed_out=True)
    except OSError as error:
        return VimCapture(126, "", str(error), None)
    output: str | None = None
    if out_path.is_file():
        output = out_path.read_bytes().decode("utf-8", errors="replace")
    return VimCapture(completed.returncode, completed.stdout, completed.stderr, output)


def check_capture(capture: VimCapture, case: Case) -> str | None:
    """Return an error string when Vim did not cleanly accept the script."""
    if capture.timed_out:
        return "Vim timed out"
    if capture.output is None:
        return f"Vim did not write captured output (exit {capture.returncode})"
    if capture.returncode != 0:
        combined = capture.stdout + capture.stderr + capture.output
        detail = first_line(combined) or f"exit {capture.returncode}"
        return f"Vim exit {capture.returncode}: {detail}"
    combined = capture.output + capture.stdout + capture.stderr
    match = ERROR_RE.search(combined)
    if match:
        line = next(
            (line for line in combined.splitlines() if "E" in line),
            match.group(0),
        ).strip()
        return f"Vim reported an error: {line[:200]}"
    if "Error detected" in combined:
        return "Vim reported 'Error detected while processing'"
    extras = (capture.stdout + capture.stderr).strip()
    if extras:
        return f"Vim wrote unexpected output: {first_line(extras)}"
    if not capture.output.strip():
        return "captured output is empty (add a top-level :echomsg assertion)"
    return None


def cmd_oracle(args: argparse.Namespace) -> int:
    corpus = Path(args.corpus)
    positives = [case for case in discover_cases(corpus) if case.kind == "positive"]
    if args.filter:
        positives = [case for case in positives if args.filter in case.name]
    if not positives:
        print(f"oracle: no positive cases found under {corpus}", file=sys.stderr)
        return 2
    vim = resolve_executable(args.vim)
    if vim is None:
        print(f"oracle: Vim executable not found: {args.vim}", file=sys.stderr)
        return 2
    expected_dir = corpus / "expected"
    if args.check and not expected_dir.is_dir():
        print(
            f"oracle --check: expected directory missing: {expected_dir}\n"
            "run 'python3 tools/vim9_diff.py oracle' first",
            file=sys.stderr,
        )
        return 2
    print(f"Vim: {vim_version(vim)}")
    print(f"corpus: {corpus}")
    print("oracle: " + ("checking recorded output" if args.check else "capturing expected output"))

    failures: list[str] = []
    skipped: list[str] = []
    created = updated = unchanged = checked_ok = 0
    records: list[dict] = []
    with tempfile.TemporaryDirectory(prefix="rvim-vim9-oracle-") as temporary:
        work = Path(temporary)
        for case in positives:
            if case.no_oracle:
                skipped.append(case.name)
                print(f"{case.name:<38} SKIP  {case.no_oracle_reason or 'no-oracle'}")
                continue
            out_path = work / (case.stem + ".out")
            capture = capture_with_vim(vim, case.path, out_path, args.timeout)
            error = check_capture(capture, case)
            if error:
                failures.append(case.name)
                print(f"{case.name:<38} FAIL  {error}")
                if args.verbose:
                    print(indent_text(first_lines(capture.output or "", 20), 4))
                continue
            output = capture.output or ""
            expected_path = expected_dir / (case.stem + ".out")
            if args.check:
                if not expected_path.is_file():
                    failures.append(case.name)
                    print(f"{case.name:<38} FAIL  missing {expected_path.name}")
                    continue
                recorded = expected_path.read_bytes().decode("utf-8", errors="replace")
                if recorded == output:
                    checked_ok += 1
                    print(f"{case.name:<38} OK    {len(output)} bytes")
                else:
                    failures.append(case.name)
                    print(f"{case.name:<38} STALE {expected_path.name}")
                    print(indent_text(describe_difference(recorded, output), 4))
            else:
                expected_dir.mkdir(parents=True, exist_ok=True)
                data = output.encode("utf-8")
                if not expected_path.is_file():
                    expected_path.write_bytes(data)
                    created += 1
                    action = "created"
                elif expected_path.read_bytes() != data:
                    expected_path.write_bytes(data)
                    updated += 1
                    action = "updated"
                else:
                    unchanged += 1
                    action = "unchanged"
                print(f"{case.name:<38} {action:<9} {len(output)} bytes")
            records.append({"name": case.name, "bytes": len(output)})

    print()
    if args.check:
        print(
            f"oracle --check: {checked_ok} ok, {len(failures)} failed, "
            f"{len(skipped)} skipped (no-oracle)"
        )
    else:
        print(
            f"oracle: {created} created, {updated} updated, {unchanged} unchanged, "
            f"{len(failures)} failed, {len(skipped)} skipped (no-oracle)"
        )
    if failures:
        print(f"oracle: failed cases: {', '.join(failures)}", file=sys.stderr)
    if args.json:
        write_json(
            args.json,
            {
                "command": "oracle",
                "vim": vim_version(vim),
                "check": args.check,
                "created": created,
                "updated": updated,
                "unchanged": unchanged,
                "skipped": skipped,
                "failed": failures,
                "cases": records,
            },
        )
        print(f"oracle: wrote {args.json}")
    return 1 if failures else 0


def describe_difference(recorded: str, actual: str) -> str:
    recorded_lines = recorded.splitlines()
    actual_lines = actual.splitlines()
    for index in range(max(len(recorded_lines), len(actual_lines))):
        old = recorded_lines[index] if index < len(recorded_lines) else "<missing line>"
        new = actual_lines[index] if index < len(actual_lines) else "<missing line>"
        if old != new:
            return f"first difference at line {index + 1}:\n  recorded: {old!r}\n  current:  {new!r}"
    return "outputs differ only in trailing newline"


# ---------------------------------------------------------------------------
# diff: compare RVim execution against expected output (Phase 2)
# ---------------------------------------------------------------------------


def cmd_diff(args: argparse.Namespace) -> int:
    """Phase 2 stub.

    Plumbing is in place: every oracle-checkable positive case is discovered,
    its '# vim9-corpus-no-oracle' marker is honoured, and its recorded
    expected/<name>.out is loaded.  Executing Vim9 with RVim is not
    implemented yet, so the command reports that honestly and exits 2 - it
    must never pass by pretending to have compared anything.

    TODO(phase2): this is the intended per-case execution path, once RVim can
    run Vim9 script::

        completed = subprocess.run(
            [rvim, "--headless", "-u", "NONE",
             "-c", f"source {case.path}", "-c", "qa!"],
            capture_output=True, text=True, timeout=timeout,
        )
        # RVim's :echomsg capture needs to match the oracle layout, then
        # compare the resulting text with expected/<stem>.out byte for byte.

    """
    corpus = Path(args.corpus)
    cases = [case for case in discover_cases(corpus) if case.kind == "positive"]
    if args.filter:
        cases = [case for case in cases if args.filter in case.name]
    if not cases:
        print(f"diff: no positive cases found under {corpus}", file=sys.stderr)
        return 2
    expected_dir = corpus / "expected"
    print("diff: compare RVim Vim9 execution against expected/*.out")
    print(f"corpus: {corpus}")
    print(f"rvim:   {args.rvim}")
    if resolve_executable(args.rvim) is None:
        print(f"diff: note: RVim binary not found ({args.rvim}); build it with 'cargo build -p rvim-cli'")
    if resolve_executable(args.vim) is None:
        print(f"diff: note: oracle Vim not found ({args.vim}); expectations cannot be refreshed")

    oracle_cases = [case for case in cases if not case.no_oracle]
    total_bytes = 0
    missing_expected: list[str] = []
    print()
    print(f"{'case':<38} {'expected':<10} status")
    print("-" * 70)
    for case in cases:
        if case.no_oracle:
            print(f"{case.name:<38} {'-':<10} SKIP (no-oracle)")
            continue
        expected_path = expected_dir / (case.stem + ".out")
        if not expected_path.is_file():
            missing_expected.append(case.name)
            print(f"{case.name:<38} {'missing':<10} FAIL (no recorded output)")
            continue
        size = len(expected_path.read_bytes())
        total_bytes += size
        print(f"{case.name:<38} {size:<10} TODO (Phase 2)")

    print()
    if missing_expected:
        print(
            "diff: expected output missing for: " + ", ".join(missing_expected)
            + "\n      run 'python3 tools/vim9_diff.py oracle' first",
            file=sys.stderr,
        )
    print(
        "diff not implemented yet (Phase 2): RVim has no Vim9 execution path.\n"
        f"Plumbing loaded {len(oracle_cases) - len(missing_expected)}/{len(oracle_cases)} "
        f"cases ({total_bytes} expected bytes); no output was compared and nothing passed.\n"
        "TODO: implement cmd_diff() in tools/vim9_diff.py once RVim can source Vim9 files."
    )
    return 2


# ---------------------------------------------------------------------------
# check-corpus: validate corpus hygiene
# ---------------------------------------------------------------------------


def cmd_check_corpus(args: argparse.Namespace) -> int:
    corpus = Path(args.corpus)
    problems: list[str] = []

    def fail(message: str) -> None:
        problems.append(message)

    positive_dir = corpus / "positive"
    negative_dir = corpus / "negative"
    expected_dir = corpus / "expected"
    support_dir = corpus / "support"
    for label, directory in (
        ("positive", positive_dir),
        ("negative", negative_dir),
        ("expected", expected_dir),
    ):
        if not directory.is_dir():
            fail(f"missing corpus directory: {directory}")

    positive_paths = sorted(positive_dir.glob("*.vim")) if positive_dir.is_dir() else []
    negative_paths = sorted(negative_dir.glob("*.vim")) if negative_dir.is_dir() else []
    support_paths = sorted(support_dir.glob("*.vim")) if support_dir.is_dir() else []
    if not positive_paths:
        fail(f"no positive cases in {positive_dir}")
    if not negative_paths:
        fail(f"no negative cases in {negative_dir}")

    case_paths = [("positive", path) for path in positive_paths] + [
        ("negative", path) for path in negative_paths
    ]
    seen_names: dict[str, str] = {}
    for kind, path in case_paths:
        if not CASE_NAME_RE.match(path.stem):
            fail(f"{kind}/{path.name}: name must match [a-z0-9][a-z0-9_]*.vim")
        if path.stem in seen_names:
            fail(f"duplicate case name {path.stem!r}: {seen_names[path.stem]} and {path.name}")
        else:
            seen_names[path.stem] = f"{kind}/{path.name}"

    positives: list[tuple[Path, str]] = []
    for path in positive_paths:
        text = read_corpus_file(path, fail)
        if not text:
            continue
        positives.append((path, text))
        lines = text.splitlines()
        first = next((line for line in lines if line.strip()), "")
        if not VIM9_HEADER_RE.match(first.strip()):
            fail(f"positive/{path.name}: first non-blank line must be a 'vim9script' header")
        if "echomsg" not in text:
            fail(f"positive/{path.name}: no :echomsg assertion, oracle output would be empty")
        for token in FORBIDDEN_POSITIVE_TOKENS:
            for number, line in enumerate(lines, 1):
                if line.lstrip().startswith("#"):
                    continue
                if token in line:
                    fail(f"positive/{path.name}:{number}: forbidden side effect {token!r}")
        match = NO_ORACLE_RE.search(text)
        if match:
            reason = (match.group(1) or "").strip()
            if not reason:
                fail(
                    f"positive/{path.name}: '# vim9-corpus-no-oracle' needs a reason "
                    "after ':' (for example ': added in Vim 9.2')"
                )
            expected = expected_dir / (path.stem + ".out")
            if expected.is_file():
                fail(
                    f"positive/{path.name}: no-oracle case must not have "
                    f"expected/{expected.name}"
                )
        else:
            expected = expected_dir / (path.stem + ".out")
            if not expected.is_file():
                fail(
                    f"positive/{path.name}: missing expected/{expected.name} "
                    "(run 'python3 tools/vim9_diff.py oracle')"
                )

    for path in negative_paths:
        text = read_corpus_file(path, fail)
        if not text:
            continue
        match = EXPECT_ERROR_RE.search(text)
        if not match and not EXPECT_FAIL_RE.search(text):
            fail(
                f"negative/{path.name}: needs a '# expect-error: E<code>' or "
                "'# expect: fail' comment"
            )
        if NO_ORACLE_RE.search(text):
            fail(f"negative/{path.name}: no-oracle marker is only valid for positive cases")
        expected = expected_dir / (path.stem + ".out")
        if expected.is_file():
            fail(f"negative/{path.name}: must not have expected/{expected.name}")

    positive_stems = {path.stem for path in positive_paths}
    negative_stems = {path.stem for path in negative_paths}
    no_oracle_stems = {
        path.stem for path, text in positives if NO_ORACLE_RE.search(text)
    }
    expected_stems: set[str] = set()
    if expected_dir.is_dir():
        for path in sorted(expected_dir.iterdir()):
            if path.is_dir():
                fail(f"expected/{path.name}: directories are not allowed")
                continue
            if path.suffix != ".out":
                fail(f"expected/{path.name}: expected files must end in .out")
                continue
            if path.stem not in positive_stems:
                fail(f"expected/{path.name}: no matching positive case")
                continue
            if path.stem in negative_stems:
                fail(f"expected/{path.name}: negative cases must not have expected output")
                continue
            if path.stem in no_oracle_stems:
                fail(f"expected/{path.name}: no-oracle case must not have expected output")
                continue
            expected_stems.add(path.stem)
            try:
                data = path.read_bytes()
            except OSError as error:
                fail(f"expected/{path.name}: cannot read: {error}")
                continue
            if b"\r" in data:
                fail(f"expected/{path.name}: contains carriage returns")
            text = data.decode("utf-8", errors="replace")
            if not text.strip():
                fail(f"expected/{path.name}: empty expected output")
            if ERROR_RE.search(text) or "Error detected" in text:
                fail(f"expected/{path.name}: contains a Vim error; re-run oracle")
            if data and not data.endswith(b"\n"):
                fail(f"expected/{path.name}: captured output must end with a newline")

    for path in support_paths:
        text = read_corpus_file(path, fail)
        if not text:
            continue
        first = next((line for line in text.splitlines() if line.strip()), "")
        if not VIM9_HEADER_RE.match(first.strip()):
            fail(f"support/{path.name}: first non-blank line must be a 'vim9script' header")

    print(f"corpus: {corpus}")
    print(
        f"positives: {len(positive_paths)} "
        f"({len(positive_paths) - len(no_oracle_stems)} oracle-checked, "
        f"{len(no_oracle_stems)} no-oracle)"
    )
    print(f"negatives: {len(negative_paths)}")
    print(f"expected:  {len(expected_stems)} files")
    print(f"support:   {len(support_paths)} modules")
    if problems:
        print()
        print(f"check-corpus: {len(problems)} problem(s) found:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print("check-corpus: OK")
    return 0


def read_corpus_file(path: Path, fail) -> str:
    try:
        raw = path.read_bytes()
    except OSError as error:
        fail(f"{path.name}: cannot read: {error}")
        return ""
    if b"\r" in raw:
        fail(f"{path.name}: contains carriage returns (use LF line endings)")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        fail(f"{path.name}: not valid UTF-8: {error}")
        return raw.decode("utf-8", errors="replace")
    if not text.strip():
        fail(f"{path.name}: file is empty")
        return ""
    return text


# ---------------------------------------------------------------------------
# helpers and entry point
# ---------------------------------------------------------------------------


def first_lines(text: str, count: int) -> str:
    lines = text.splitlines()
    if len(lines) <= count:
        return text
    return "\n".join(lines[:count]) + f"\n... ({len(lines) - count} more lines)"


def indent_text(text: str, spaces: int) -> str:
    prefix = " " * spaces
    return "\n".join(prefix + line for line in text.splitlines())


def write_json(path: str, payload: dict) -> None:
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    subparsers = parser.add_subparsers(dest="command", required=True)

    def add_common(subparser: argparse.ArgumentParser) -> None:
        subparser.add_argument(
            "--corpus",
            default=str(DEFAULT_CORPUS),
            help="corpus directory (default: tools/vim9_corpus)",
        )

    parse = subparsers.add_parser("parse", help="run corpus cases through the Rust vim9_parse example")
    add_common(parse)
    parse.add_argument("--filter", default=None, help="only run cases whose name contains this text")
    parse.add_argument("--verbose", action="store_true", help="show parser output per case")
    parse.add_argument("--timeout", type=float, default=120.0, help="per-case timeout in seconds")
    parse.add_argument("--example-bin", default=None, help="path to a pre-built vim9_parse binary")
    parse.add_argument("--json", default=None, help="write results as JSON to this path")
    parse.set_defaults(func=cmd_parse)

    oracle = subparsers.add_parser("oracle", help="capture/verify expected output with real Vim")
    add_common(oracle)
    oracle.add_argument("--filter", default=None, help="only handle cases whose name contains this text")
    oracle.add_argument("--check", action="store_true", help="compare with expected/ without writing")
    oracle.add_argument("--vim", default="vim", help="Vim executable (default: vim from PATH)")
    oracle.add_argument("--timeout", type=float, default=30.0, help="per-case timeout in seconds")
    oracle.add_argument("--verbose", action="store_true", help="show captured output on failure")
    oracle.add_argument("--json", default=None, help="write results as JSON to this path")
    oracle.set_defaults(func=cmd_oracle)

    diff = subparsers.add_parser("diff", help="compare RVim output with expected/ (Phase 2 stub)")
    add_common(diff)
    diff.add_argument("--filter", default=None, help="only handle cases whose name contains this text")
    diff.add_argument("--rvim", default=str(DEFAULT_RVIM), help="RVim binary (default: target/debug/rvim)")
    diff.add_argument("--vim", default="vim", help="Vim executable used to refresh expectations")
    diff.add_argument("--timeout", type=float, default=30.0, help="per-case timeout in seconds")
    diff.set_defaults(func=cmd_diff)

    check = subparsers.add_parser("check-corpus", help="validate corpus hygiene")
    add_common(check)
    check.set_defaults(func=cmd_check_corpus)

    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    sys.exit(main())
