#!/usr/bin/env python3
"""Eval script for the Rust-based its_hub_rs project.

Runs each eval dimension as a subprocess and outputs JSON to stdout.

Output format:
    {"results": [{"name": str, "score": float, "weight": float, "passed": bool, "details": str}, ...]}
"""

import json
import subprocess
import sys
import os
import re
from pathlib import Path

PROJECT_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "its_hub_rs")

_env = os.environ.copy()
_cargo_bin = os.path.join(os.path.expanduser("~"), ".cargo", "bin")
if _cargo_bin not in _env.get("PATH", ""):
    _env["PATH"] = _cargo_bin + os.pathsep + _env.get("PATH", "")


def _run_cargo(args: list[str], timeout: int = 300) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["cargo"] + args,
        capture_output=True,
        text=True,
        timeout=timeout,
        cwd=PROJECT_DIR,
        env=_env,
    )


def _score_from_output(result: subprocess.CompletedProcess) -> float:
    if result.returncode == 0:
        return 1.0
    error_lines = [ln for ln in (result.stdout + result.stderr).splitlines() if ln.strip()]
    if not error_lines:
        return 0.0
    return max(0.0, 1.0 - len(error_lines) * 0.05)


def eval_tests() -> dict:
    """Run Rust test suite via cargo test."""
    try:
        result = _run_cargo(["test"])
        passed = result.returncode == 0
        return {
            "name": "tests",
            "score": _score_from_output(result),
            "weight": 0.4,
            "passed": passed,
            "details": (result.stdout or result.stderr).strip()[-500:],
        }
    except subprocess.TimeoutExpired:
        return {
            "name": "tests",
            "score": 0.0,
            "weight": 0.4,
            "passed": False,
            "details": "Timed out after 300s",
        }


def eval_lint() -> dict:
    """Run Clippy linter with warnings as errors."""
    try:
        result = _run_cargo(["clippy", "--", "-D", "warnings"])
        passed = result.returncode == 0
        return {
            "name": "lint",
            "score": _score_from_output(result),
            "weight": 0.25,
            "passed": passed,
            "details": (result.stdout or result.stderr).strip()[-500:],
        }
    except subprocess.TimeoutExpired:
        return {
            "name": "lint",
            "score": 0.0,
            "weight": 0.25,
            "passed": False,
            "details": "Timed out after 300s",
        }


def eval_type_check() -> dict:
    """Type check via cargo build (Rust checks types at compile time)."""
    try:
        result = _run_cargo(["build"])
        passed = result.returncode == 0
        return {
            "name": "type_check",
            "score": _score_from_output(result),
            "weight": 0.15,
            "passed": passed,
            "details": (result.stdout or result.stderr).strip()[-500:],
        }
    except subprocess.TimeoutExpired:
        return {
            "name": "type_check",
            "score": 0.0,
            "weight": 0.15,
            "passed": False,
            "details": "Timed out after 300s",
        }


def eval_build_release() -> dict:
    """Build optimized release binary."""
    try:
        result = _run_cargo(["build", "--release"], timeout=600)
        passed = result.returncode == 0
        return {
            "name": "build_release",
            "score": _score_from_output(result),
            "weight": 0.1,
            "passed": passed,
            "details": (result.stdout or result.stderr).strip()[-500:],
        }
    except subprocess.TimeoutExpired:
        return {
            "name": "build_release",
            "score": 0.0,
            "weight": 0.1,
            "passed": False,
            "details": "Timed out after 600s",
        }


def eval_observability() -> dict:
    """Analyze observability coverage in Rust source: logging, structured logging, tracing."""
    skip = {
        "tests", "test", "target", ".git", ".factory", "eval",
    }
    log_pats = [
        r"\blog::\w+!",
        r"\btracing::\w+!",
        r"\binfo!\(", r"\bwarn!\(", r"\berror!\(", r"\bdebug!\(", r"\btrace!\(",
        r"\bprintln!\(", r"\beprintln!\(",
    ]
    struct_pats = [r"\btracing\b", r"\bslog\b", r"\bserde_json\b"]
    trace_pats = [
        r"\btracing::", r"\b#\[instrument\b",
        r"\bopentelemetry\b", r"\bSpan\b", r"\bspan!\(",
    ]

    rs_root = Path(PROJECT_DIR)
    if not rs_root.exists():
        return {"name": "observability", "score": 0.0, "weight": 0.1,
                "passed": True, "details": "Rust project directory not found"}

    sources = [f for f in rs_root.rglob("*.rs")
               if not any(p in f.relative_to(rs_root).parts for p in skip)]

    total_fn = logged_fn = total_log = 0
    has_struct = has_trace = False

    fn_pat = re.compile(r"\b(pub\s+)?(async\s+)?fn\s+\w+")

    for src in sources:
        try:
            code = src.read_text(errors="replace")
        except OSError:
            continue
        lines = code.splitlines()
        for i, line in enumerate(lines):
            if fn_pat.search(line):
                total_fn += 1
                end = min(i + 50, len(lines))
                body = "\n".join(lines[i:end])
                for pat in log_pats:
                    if re.search(pat, body):
                        logged_fn += 1
                        break
        for pat in log_pats:
            total_log += len(re.findall(pat, code))
        for pat in struct_pats:
            if re.search(pat, code):
                has_struct = True
        for pat in trace_pats:
            if re.search(pat, code):
                has_trace = True

    if total_fn == 0:
        return {"name": "observability", "score": 0.0, "weight": 0.1,
                "passed": True, "details": "No functions found to analyze"}

    cov = logged_fn / total_fn
    density = min(1.0, total_log / max(total_fn, 1))
    score = 0.40 * cov + 0.25 * float(has_struct) + 0.20 * float(has_trace) + 0.15 * density

    details = (f"coverage={cov:.0%} ({logged_fn}/{total_fn}), "
               f"structured={'yes' if has_struct else 'no'}, "
               f"tracing={'yes' if has_trace else 'no'}, "
               f"density={density:.0%}")

    return {"name": "observability", "score": round(score, 3), "weight": 0.1,
            "passed": score >= 0.3, "details": details}


EVALS = [eval_tests, eval_lint, eval_type_check, eval_build_release, eval_observability]


def main() -> None:
    results = [fn() for fn in EVALS]
    output = {"results": results}
    json.dump(output, sys.stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
