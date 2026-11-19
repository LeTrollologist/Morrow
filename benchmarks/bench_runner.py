#!/usr/bin/env python3
"""Morrow benchmark runner.

Runs the canonical benchmark suite through `bin/forge.exe bench`, which compiles
and executes every `benchmarks/*.mw` workload and the `benchmarks/c/` baselines.
This wrapper exists so the benchmark suite can be driven from the documented
`benchmarks/` location without duplicating the compiler's benchmark logic.

Usage:
    python benchmarks/bench_runner.py [extra forge bench arguments]

Exit status is 0 only when the complete benchmark suite succeeds. Missing tools,
compile failures, runtime failures, and benchmark mismatches propagate as a
non-zero status.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FORGE = ROOT / "bin" / "forge.exe"
ALT_FORGE = ROOT / "bin" / "forge"


def find_forge() -> Path:
    if FORGE.exists():
        return FORGE
    if ALT_FORGE.exists():
        return ALT_FORGE
    exe = shutil.which("forge")
    if exe:
        return Path(exe)
    raise SystemExit("bench_runner: bin/forge.exe not found; build the toolchain first")


def main(argv: list[str]) -> int:
    if argv and argv[0] in ("-h", "--help"):
        print(__doc__.strip())
        return 0
    forge = find_forge()
    cmd = [str(forge), "bench", *argv]
    print("=== Morrow benchmark suite ===")
    print("command:", " ".join(cmd), flush=True)
    try:
        result = subprocess.run(cmd, cwd=str(ROOT))
    except OSError as exc:
        print(f"bench_runner: failed to launch forge: {exc}", file=sys.stderr)
        return 127
    if result.returncode != 0:
        print(f"bench result: FAILED (exit {result.returncode})", file=sys.stderr)
    return result.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
