# Morrow Release Process & Pipeline Specification

This document defines the standardized local build, verification, packaging, and
release pipeline for the Morrow toolchain (`scripts/pipeline.py`). It is the
authoritative description of how a release is made; if the script and this
document disagree, the script is wrong.

---

## 1. Pipeline overview

| Stage | Command / Action | Description |
|---|---|---|
| **1. `preflight`** | `clang --version`, `git` checks | Validates host requirements: clang on `PATH`, `bin/forge.exe` present, inside a git work tree on a branch, and an `origin` remote configured. A dirty tree is reported here and enforced later, at publish time. No host service or CLI client is consulted. |
| **2. `version`** | rewrites declarations | Sets the release version in every place it is declared: `compiler/forge.mw` (`forge X.Y.Z ...`), `docs/SYNTAX.md` (header + "verified against" line), `README.md` (release badge), `Makefile` (`TAG ?=`), `ROADMAP.md` (status note). Fails if any declaration still carries a different version afterwards. Line endings are preserved. |
| **3. `build`** | `mwc -> stage1 -> stage2 -> stage3` | 3-stage self-hosting bootstrap; requires `SHA256(stage2.exe.ll) == SHA256(stage3.exe.ll)`. Copies stage 3 over `bin/forge.exe` and `bin/mwc.exe`, then **asserts the rebuilt binary reports the release version and that `docs/SYNTAX.md` agrees** (the same rule the CI gate enforces). |
| **4. `test`** | `forge test` + example builds | Runs all positive and compile-fail suites, writes the full log to `target/pipeline-test.log`, and **requires the `test result: ok. N positive suites, M compile-fail suites passed.` line**. Then compiles every `examples/*.mw`. |
| **5. `bench`** | `forge bench` | Runs the Morrow benchmarks and their `-O2` C baselines. |
| **6. `package`** | assets in `dist/v{VER}/` | Copies `forge.exe`, `mwc.exe`, builds `Morrow-v{VER}-win64.zip`, and writes `RELEASE_NOTES.md` (see §4). |
| **7. `verify`** | checksums + integrity | Requires the four expected assets to exist, writes `SHA256SUMS.txt` covering every asset, and runs a CRC check over the archive. |
| **8. `publish`** | `git` only | Commits pipeline-managed changes, tags `HEAD`, pushes branch and tag, then **verifies with `git ls-remote` that the tag exists on `origin` and points at the tagged commit**. Writes `dist/v{VER}/PUBLISH.txt` recording commit, remote, fixed point and asset list. |

---

## 2. CLI usage

```bash
python scripts/pipeline.py v2.3.1                 # full release
python scripts/pipeline.py v2.3.1 --dry-run       # preview, no side effects
python scripts/pipeline.py v2.3.1 --no-publish    # everything except tag + push
python scripts/pipeline.py v2.3.1 --from package  # resume from a stage
python scripts/pipeline.py v2.3.1 --skip bench    # skip one stage
python scripts/pipeline.py v2.3.1 --no-bump       # declarations must already match
python scripts/pipeline.py v2.3.1 --force         # adopt dirty files, move an existing tag
```

Equivalent `make` targets (they pass `TAG`):

```bash
make release TAG=v2.3.1      # full pipeline
make check   TAG=v2.3.1      # build + test + bench + package + verify, no publish
make build | test | bench | package | verify | dry-run
```

---

## 3. Publish semantics

Publishing is entirely local: `git commit`, `git tag -a`, `git push`,
`git ls-remote`, plus the release record described in §7. No host CLI, API client or
CI runner is involved, and no Release object is created on any forge — the tag is the
release.

1. **Dirty trees.** The publish stage owns exactly the files it writes: version
   declarations, `bin/*.exe`, `release-notes/`, `releases/`, `dist/`. If only those
   changed, it commits them as `chore(release): vX.Y.Z`. Any other uncommitted file
   aborts the release unless `--force` is passed.
2. **Tags.** The annotated tag `vX.Y.Z` is created at `HEAD`. Moving an existing tag
   requires `--force`; a tag that is *behind* `HEAD` (but an ancestor) is a
   republication and requires `--clobber`.
3. **Verification, not trust.** After pushing, the pipeline runs
   `git ls-remote --tags origin` and checks the tag object id and its peeled commit
   against the local tag. A push that silently did not land fails the stage.
4. **No API client.** There is no dependency on a web UI, a personal access token or a
   forge CLI, so the same command works offline, against a mirror, or on a fork with a
   different remote.
5. **No host CLI on `main`.** `main` never invokes `gh`, `glab`, `hub` or any other
   forge client -- not for auth, not for release creation, not for uploads. This is
   checked mechanically: the preflight stage scans `scripts/pipeline.py`, `Makefile`
   and `.github/workflows/ci.yml` for host-CLI invocations and aborts the release if
   one appears, naming the file and line.

## 3a. `--clobber`: republication as a first-class operation

Publishing the same version twice is a normal need (a rebuilt artifact, a corrected
notice, a lost `dist/`). It is supported, but never silently:

| Situation | Behaviour |
|---|---|
| `dist/vX.Y.Z/` already holds `PUBLISH.txt` | package refuses to overwrite it; `--clobber` rebuilds |
| rebuilt artifact differs from the recorded publication | verify fails, naming the asset; `--clobber` accepts and records the new bytes |
| the version is already recorded in `releases/` | publish refuses; `--clobber` records another publication |
| republication identical to the recorded one | nothing is written and no commit is created -- a clean no-op |
| `--clobber` on a tag that is behind `HEAD` | allowed; the tag keeps identifying the release commit and the record lands on top |

`--clobber` never moves a tag. Moving one is `--force`, deliberately a different flag.

---

## 4. Release notes

Authored notes live in `release-notes/vX.Y.Z.md` (tracked in git). The package
stage copies them to `dist/vX.Y.Z/RELEASE_NOTES.md`, and the publish stage uses
that file as the release body and title (first markdown heading).

If the file is missing, the pipeline generates a *factual skeleton* — fixed-point
hash, test summary, asset list, and an explicit note that authored notes are
missing. It never invents feature claims.

---

## 5. Distribution layout

```
dist/v2.4.1/
├── Morrow-v2.4.1-win64.zip    # bin, std, compiler, lib (incl. CRT), examples,
│                              # tests (incl. tests/negative), benchmarks + c,
│                              # docs, scripts, release-notes, releases, licenses,
│                              # .github, README, ROADMAP, RELEASE_PROCESS,
│                              # LICENSE.md, THIRD-PARTY-NOTICES.md, Makefile
├── forge.exe                  # standalone unified CLI
├── mwc.exe                    # standalone compiler driver
├── RELEASE_NOTES.md           # release body
├── SHA256SUMS.txt             # checksums for every asset above
└── PUBLISH.txt                # written by the publish stage: tag, commit, remote,
                               # fixed point, test summary, asset inventory
```

`dist/` is git-ignored; it is produced fresh by the package stage on every run.

### Licensing in the archive

The verify stage refuses to pass an archive that does not contain `LICENSE.md`,
`THIRD-PARTY-NOTICES.md` and the upstream license texts under `licenses/`. The
bundled CRT (`lib/crt/*.a`, `*.o`) is third-party material — GCC runtime (GPL-3.0 +
GCC Runtime Library Exception), mingw-w64 (ZPL 2.1), libiconv (LGPL-2.1), SQLite
(public domain) — so its attribution has to ship with it.

Note that `LICENSE.md` is **not** part of the version-bump list in §1: it records
"effective for v2.4.1 and later" as a historical boundary, so the `version` stage
must never rewrite it to a later release.

---

## 7. The local release record

Every publication writes `releases/vX.Y.Z.json`, committed with the release. It is
the repository's own answer to "what did this version actually ship", with no host
service in the loop:

```json
{
  "tag": "v2.4.4",
  "version": "2.4.4",
  "remote": "https://github.com/LeTrollologist/Morrow.git",
  "fixed_point": "…",
  "tests": "test result: ok. 23 positive suites, 17 compile-fail suites passed.",
  "examples": 22,
  "notes": "release-notes/v2.4.4.md",
  "assets": [{"name": "…", "bytes": 0, "sha256": "…"}],
  "published_at": "…Z",
  "republish_count": 0,
  "previous_publication": null
}
```

It deliberately records **no commit hash**: the record is committed alongside the
release, so a stored hash would be invalidated by its own commit and every rerun
would look like a change. `git` already answers "which commit is this tag"; the
record answers "which bytes were published, and did they verify".

The verify stage compares freshly built artifacts against this record on every
`--clobber` run, which is what makes republication *provable*: either the rebuild is
byte-identical (nothing is written), or the drift is named asset by asset and must be
accepted with `--clobber`.

One bootstrap property worth knowing: the archive is built *before* the record for
the release being made, so `Morrow-vX.Y.Z-win64.zip` ships the records of earlier
releases plus its own `dist/vX.Y.Z/PUBLISH.txt`. The record for the release itself is
added to `releases/` by the publish stage and ships with the following release. That
is the same ordering a changelog has, and it keeps the packaged bytes and the
recorded hashes describing the same moment.

---

## 8. Relationship to CI and host tooling

`.github/workflows/ci.yml` is an optional safety net for forked copies, not the
release mechanism. It re-runs `forge test`, the example builds, `forge bench`, the
docs/version gate and the bootstrap fixed point on push. Releases are made locally
by this pipeline, which enforces the same gates before it tags anything.

Host CLIs (`gh`, `glab`, `hub`, …) are **never** used by `main`. They exist as a
convenience for people working from a fork — opening pull requests, or manually
attaching assets to their own release page. `scripts/pipeline.py` is git-only, and
preflight fails if a host-CLI invocation ever creeps into the release path.

**Status (2026-11-01, v2.3.1).** GitHub Actions is currently not executing this
workflow: every job since 2026-10-31 completes in ~2 s with no steps, and the
job annotation reads *"The job was not started because your account is locked due
to a billing issue."* The workflow file itself is valid and the same steps pass
locally. Until the account lock is cleared, the **local pipeline is the only
enforced gate** — treat a red/absent CI run as "not run", never as "passed".
