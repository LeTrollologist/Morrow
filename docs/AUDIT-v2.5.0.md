# Morrow v2.5.0 Verification Report

**Status:** Maintainer-run verification, **not an independent audit**.
**Date:** 2026-11-19
**Evaluated baseline:** v2.4.20 (commit `90d8749`)
**Candidate work:** post-v2.4.20 C1 indirect region-escape summaries
**Author:** Morrow maintainer toolchain (automated commands below)

This report exists because `ROADMAP.md` Phase 15 promised a fresh
`docs/AUDIT-v2.5.0.md`. It records what the v2.4.20 release actually verifies so the
next milestone can be judged against evidence rather than recollection. It is reproducible but not independent: the same project wrote the compiler and
this report. No independent audit is claimed or implied.

## 1. Reproduction commands

Run from the repository root on Windows x64:

```powershell
.\bin\forge.exe version
.\bin\forge.exe test
python benchmarks\bench_runner.py
python -m py_compile scripts\*.py benchmarks\*.py
```

Self-hosting fixed point (three stages):

```powershell
.\bin\mwc.exe compiler\main.mw -o target\s1.exe
.\target\s1.exe compiler\main.mw -o target\s2.exe
.\target\s2.exe compiler\main.mw -o target\s3.exe
# SHA256(s2.exe.ll) == SHA256(s3.exe.ll), s2.exe == s3.exe
```

## 2. Recorded evidence

| Gate | Observed result |
|---|---|
| Toolchain | `forge 2.4.20 (morrow standalone 2026)` |
| Test suites | `test result: ok. 31 positive suites, 24 compile-fail suites passed.` |
| Fixed point | `473E1F0A808673FC8B8F9F34D0B3AA1C4D83FEAE851F4DC51E308D68847598D8` |
| Binary parity | stage2.exe == stage3.exe, 638464 bytes |
| Examples | 22 built; 17 executed with expected exit codes; 5 build-only |
| Benchmark runner | `bench result: ok. all benchmark programs completed.` |
| Python helpers | `py_compile` passes for `scripts/*.py` and `benchmarks/*.py` |
| FFI / SQLite build | `ffi_demo` runs; SQLite examples build (DLL documented, not shipped) |
| LSP framing | valid frames answered; oversized/malformed frames rejected with JSON-RPC `-32700` |
| Parser limits | 20k parentheses/unary input reports the 512-level limit, no stack overflow |
| ABI | signed negative `i32` return (`atoi("-5") == -5`), pointer args, abortive-handler longjmp |
| C1 indirect escapes | `region_indirect_return`, `region_indirect_store`, `region_indirect_container`, and `region_indirect_wrapper` fail on v2.4.20 and pass with call-boundary diagnostics; `region_indirect_safe_test` accepts read-only/region-local cases |

## 3. Phase 15 audit-finding status

This is a **maintainer assessment**, not an external conclusion.

### P0

| Item | Status in v2.4.20 | Evidence / gap |
|---|---|---|
| P0-1 Lossless formatter | **Open** | Safety gate refuses comments/imports/effects; no lossless printer. |
| P0-2 Dynamic effect dispatch | Closed | Callee-under-handler regressions pass. |
| P0-3 Pointers/refs/casts | Closed | `ffi_demo`, pointer matrix, cast tests. |
| P0-4 Example execution + runtime deps | **Partial** | 17 runnable examples gated; SQLite DLL documented, not packaged; 5 build-only. |
| P0-5 Honest tests | Closed | `assert_true` exits; retries removed; formatter token check. |

### P1

| Item | Status | Evidence / gap |
|---|---|---|
| P1-1 Type soundness | Closed | Undefined types, args, returns, missing returns, let/assign. |
| P1-2 Region cleanup | Closed | `region_cleanup_test.mw`, live-count zero. |
| P1-3 Runtime traps | Closed | Unhandled effect / division diagnostics, exit 1. |
| P1-4 Process injection | Closed | `CreateProcessA`/`posix_spawn`, quoted paths, injection test. |
| P1-5 Integer widths | Closed | Width/sign arithmetic, loads/stores, overflow diagnostics. |
| P1-6 Docs truth pass | **Mostly closed** | README/SYNTAX corrected; external audit still outstanding. |
| P1-7 Paths/targets/scaffold | Closed | CWD-independent build, target validation. |

### P2

Closed: P2-1 short-circuit, P2-7 bench runner/py_compile, P2-8 ABI
(`i32`/pointer/narrow return/`returns_twice`).

Partial/open: P2-2 embedded-NUL/fuzz, P2-3 LSP per-request arena/POSIX JSON depth,
P2-4 stale-bin hash and benchmark optimization parity, P2-5 `-1` lookup sentinel and
alignment, P2-6 CSPRNG naming/source, P2-9 dynamic-string `println`, P2-10 CLI nits,
P2-11 CI/process items.

## 4. Remaining language limitations

These are real and must not be described as solved:

1. **Indirect region escapes are partially detected.** Callee summaries now reject
   region values returned through a function, stored through an outer destination
   parameter, or inserted into an outer container; read-only and region-local uses
   remain accepted. Handler-arm bodies are scanned for captured parameter stores. Effect-operation
   parameter flows, arbitrary C callbacks, indirect function-pointer calls, and very
   deep alias chains are not yet analyzed.
2. **Structured concurrency and fibers are absent** from the Pure Morrow compiler.
   The `nursery`/fiber surface belongs to archived Stage-0 and is not shipped.
3. **Variable-range and interprocedural refinement proofs are not implemented.**
   Literal casts, divisor-span checks, and path-sensitive narrowing exist; general
   interval proofs and function contracts do not.
4. **The formatter is safety-gated, not lossless.** It refuses files it cannot
   preserve and never writes after a parse error.
5. **SQLite runtime packaging is incomplete.** `libsqlite3.a` is an import library;
   `sqlite3.dll` is documented in `docs/RUNTIME.md` but not shipped.
6. **LSP memory is bounded per message but not per session.** The request arena is
   not reset between messages.
7. **Wasm effects are a no-op path.** Handlers/resume on wasm are not a supported
   execution claim.

## 5. Verdict

The post-v2.4.20 C1 work is materially stronger than the audited v2.4.16: the release
gate executes supported examples, the test harness can fail, the direct
region/effect/type-safety findings are addressed, input bounds are enforced, the C ABI
is typed, and common indirect region escapes through calls/containers are now
rejected.

It is **not** ready to claim v2.5.0 yet. C1 remains partial (callbacks/handler arms),
and C2–C6 are still open. The remaining work is correctness completion plus regression
coverage, followed by updating this maintainer-run report for the candidate commit. Reproducible verification is the release standard;
no independent audit is claimed or implied. External review is desirable but is not
a release prerequisite.
