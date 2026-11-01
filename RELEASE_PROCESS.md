# Morrow Release Process & Pipeline Specification

This document defines the standardized local build, verification, packaging, and release pipeline for the Morrow programming language toolchain.

---

## 1. Pipeline Overview

The Morrow release pipeline (`scripts/pipeline.py`) replaces external CI/CD dependencies with a deterministic, local 7-stage workflow:

| Stage | Command / Action | Description |
|---|---|---|
| **1. `preflight`** | `clang --version`, `gh auth status` | Validates host toolchain requirements, git clean working state, and GitHub CLI authorization. |
| **2. `build`** | 3-stage self-hosting fixed-point loop | Compiles `mwc -> stage1 -> stage2 -> stage3` and requires 100% bitwise parity of `SHA256(stage2.ll) == SHA256(stage3.ll)`. Updates `bin/mwc.exe` and `bin/forge.exe`. |
| **3. `test`** | `forge test` + example compilation | Runs all 23 native test suites + 15 compile-fail suites, and compiles all 22 examples under `examples/*.mw`. |
| **4. `bench`** | `forge bench` | Executes memory region allocation, HTTP parser, and HashMap benchmarks along with `-O2` C baselines. |
| **5. `package`** | Asset creation in `dist/v{VER}/` | Packages `Morrow-v{VER}-win64.zip` and copies standalone CLI binaries (`forge.exe`, `mwc.exe`). |
| **6. `verify`** | Checksums and zip testing | Generates `SHA256SUMS.txt` and tests zip header / CRC-32 integrity. |
| **7. `publish`** | `git push` + `gh release create` | Fast-forwards `main`, tags `v{VER}`, and publishes GitHub release with release notes and distribution assets. |

---

## 2. CLI Usage

### Full Automated Release
```bash
python scripts/pipeline.py v2.3.0
# Or using make:
make release TAG=v2.3.0
```

### Dry Run (Preview Only)
```bash
python scripts/pipeline.py v2.3.0 --dry-run
# Or using make:
make dry-run TAG=v2.3.0
```

### Local Check (No Push / No GitHub Release)
```bash
python scripts/pipeline.py v2.3.0 --no-publish
# Or using make:
make check TAG=v2.3.0
```

### Resume from a Specific Stage
```bash
python scripts/pipeline.py v2.3.0 --from package
```

### Publish as Draft
```bash
python scripts/pipeline.py v2.3.0 --draft
```

---

## 3. Distribution Layout

Release artifacts are written to `dist/v{VER}/`:
```
dist/v2.3.0/
├── Morrow-v2.3.0-win64.zip    # Full distribution archive
├── forge.exe                  # Standalone unified CLI
├── mwc.exe                    # Standalone compiler driver
├── SHA256SUMS.txt             # Checksums for all released assets
└── RELEASE_NOTES.md           # Generated release documentation
```
