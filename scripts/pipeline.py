#!/usr/bin/env python3
"""
Morrow Local Release Pipeline
=============================
Standardized build, verify, package, and release orchestrator for Morrow.

Stages:
  1. preflight: check toolchain requirements (clang, git) and git hygiene.
  2. version:   bump the release version everywhere it is declared (compiler, docs, README, Makefile).
  3. build:     execute 3-stage self-hosting bootstrap and verify LLVM IR fixed-point parity.
  4. test:      run all positive and compile-fail test suites, plus build all shipped examples.
  5. bench:     run runtime benchmarks and C baselines.
  6. package:   generate canonical distribution archives, assets and release notes in dist/v{VER}/.
  7. verify:    compute SHA256SUMS.txt, test zip integrity, and cross-check docs/version agreement.
  8. publish:   commit, tag HEAD, push branch+tag with plain git, and verify the tag on origin.

Usage:
  python scripts/pipeline.py v2.3.0                 # full release pipeline
  python scripts/pipeline.py v2.3.0 --dry-run       # preview stages without side effects
  python scripts/pipeline.py v2.3.0 --from package  # resume from packaging
  python scripts/pipeline.py v2.3.0 --skip bench    # skip a specific stage
  python scripts/pipeline.py v2.3.0 --no-publish    # test and package locally without tagging/pushing
  python scripts/pipeline.py v2.3.0 --no-bump       # keep declared versions (they must already match)
  python scripts/pipeline.py v2.3.0 --force         # publish a dirty tree / move an existing tag
  python scripts/pipeline.py v2.3.0 --clobber       # republish a released version (rebuild + re-record)

Publishing is deliberately pure git: the release *is* the annotated tag, the artifacts
in dist/vX.Y.Z/ and the record in releases/vX.Y.Z.json. No host CLI, API client or CI
runner is invoked, so a release can be made on any machine with clang + git -- and
re-made later with --clobber, which re-verifies the artifacts byte-for-byte instead of
trusting them.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STAGES = ["preflight", "version", "build", "test", "bench", "package", "verify", "publish"]

# Files whose declared version must follow the release tag.
VERSION_DECLARATIONS = [
    ("compiler/forge.mw", r"forge \d+\.\d+\.\d+ \(morrow standalone 2026\)"),
    ("docs/SYNTAX.md", r"# Morrow Language & Syntax Reference \(v\d+\.\d+\.\d+\)"),
    ("docs/SYNTAX.md", r"`forge \d+\.\d+\.\d+ \(morrow standalone 2026\)`"),
    ("README.md", r"Release-v\d+\.\d+\.\d+_Standalone"),
    ("Makefile", r"TAG \?= v\d+\.\d+\.\d+"),
    ("ROADMAP.md", r"\*\*Status note \(v\d+\.\d+\.\d+\)\.\*\*"),
]


def log(msg: str, header: bool = False):
    if header:
        print(f"\n{'='*70}\n[PIPELINE] {msg}\n{'='*70}")
    else:
        print(f"[PIPELINE] {msg}")


def run(cmd, cwd=None, check=True, capture_output=False):
    if isinstance(cmd, str):
        shell = True
    else:
        shell = False
    res = subprocess.run(cmd, cwd=cwd or ROOT, shell=shell, text=True, capture_output=capture_output)
    if check and res.returncode != 0:
        err = res.stderr if capture_output else f"exit code {res.returncode}"
        raise RuntimeError(f"Command failed ({err}): {cmd}")
    return res


def compute_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest().upper()


class ReleasePipeline:
    def __init__(self, tag: str, dry_run: bool = False, no_publish: bool = False,
                 no_bump: bool = False, force: bool = False, clobber: bool = False):
        if not tag.startswith("v"):
            tag = f"v{tag}"
        self.tag = tag
        self.ver = tag.lstrip("v")
        self.dry_run = dry_run
        self.no_publish = no_publish
        self.no_bump = no_bump
        self.force = force
        self.clobber = clobber
        self.remote = None
        self.dist_dir = ROOT / "dist" / self.tag
        self.parity_hash = None
        self.suite_summary = None
        self.example_count = 0
        self.notes_path = ROOT / "release-notes" / f"{self.tag}.md"
        self.ledger_path = ROOT / "releases" / f"{self.tag}.json"

    # The release path on `main` is git and clang, nothing else. A host CLI used to
    # live here and made a "local" release depend on a remote service, so the rule is
    # now checked mechanically instead of trusted.
    HOST_CLI_PATTERN = re.compile(
        r"\b(?:gh|glab|hub)\s+(?:release|auth|api|repo|pr|issue|workflow|upload|secret|run)\b"
    )
    HOST_CLI_FILES = ("scripts/pipeline.py", "Makefile", ".github/workflows/ci.yml")

    def _assert_no_host_cli(self):
        offenders = []
        for rel in self.HOST_CLI_FILES:
            path = ROOT / rel
            if not path.exists():
                continue
            for lineno, line in enumerate(self._read_preserving(path).splitlines(), 1):
                if self.HOST_CLI_PATTERN.search(line):
                    offenders.append(f"{rel}:{lineno}: {line.strip()}")
        if offenders:
            raise RuntimeError(
                "The release path must not invoke a host CLI (git only). Found:\n  "
                + "\n  ".join(offenders)
            )
        log("Release path is host-CLI free (git + clang only).")

    def _load_ledger(self):
        """Read this tag's local release record, if one has been written."""
        if not self.ledger_path.exists():
            return {}
        try:
            return json.loads(self.ledger_path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}

    def _asset_digests(self):
        return [
            {
                "name": asset.name,
                "bytes": asset.stat().st_size,
                "sha256": compute_sha256(asset),
            }
            for asset in sorted(self.dist_dir.glob("*"))
            if asset.is_file() and asset.name not in ("SHA256SUMS.txt", "PUBLISH.txt")
        ]

    def stage_preflight(self):
        log("STAGE 1: Preflight checks", header=True)
        # Check clang
        res = run("clang --version", capture_output=True)
        clang_ver = res.stdout.splitlines()[0]
        log(f"C compiler: {clang_ver}")

        # Check forge/mwc binary
        forge_bin = ROOT / "bin" / "forge.exe"
        if not forge_bin.exists():
            raise FileNotFoundError(f"Missing {forge_bin}")
        res = run(f'"{forge_bin}" version', capture_output=True)
        forge_ver = res.stdout.strip()
        log(f"Current toolchain: {forge_ver}")
        if self.ver not in forge_ver:
            log(f"Note: toolchain version string '{forge_ver}' does not contain '{self.ver}'. Proceeding with caution.")

        # Check git status
        res = run("git status --porcelain", capture_output=True)
        if res.stdout.strip():
            log(f"Warning: working tree has modifications:\n{res.stdout.strip()}")
            # If not dry run, we still proceed if requested or warn

        # Check git wiring (local only -- releasing never leaves the machine's git).
        res = run("git rev-parse --is-inside-work-tree", capture_output=True, check=False)
        if res.returncode != 0:
            raise RuntimeError("Not inside a git work tree; cannot release.")
        branch = run("git branch --show-current", capture_output=True).stdout.strip()
        if not branch:
            raise RuntimeError("HEAD is detached; check out the release branch first.")
        log(f"Branch: {branch}")
        res = run("git remote get-url origin", capture_output=True, check=False)
        if res.returncode != 0:
            raise RuntimeError(
                "No 'origin' remote configured; add one (git remote add origin <url>) "
                "or run with --no-publish."
            )
        self.remote = res.stdout.strip()
        log(f"Remote: origin = {self.remote}")

        self._assert_no_host_cli()

        log("Preflight passed.")

    def _state_path(self) -> Path:
        return ROOT / "target" / "pipeline-state.json"

    def _save_state(self, **facts):
        """Persist build/test facts so a resumed run (`--from package`) can report them."""
        path = self._state_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        data = self._load_state()
        data["tag"] = self.tag
        data.update({k: v for k, v in facts.items() if v is not None})
        path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def _load_state(self):
        path = self._state_path()
        if not path.exists():
            return {}
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}
        return data if data.get("tag") == self.tag else {}

    def _recover_facts(self):
        """Restore parity/test facts recorded by earlier stages of the same release."""
        state = self._load_state()
        if not self.parity_hash:
            self.parity_hash = state.get("parity")
        if not self.suite_summary:
            self.suite_summary = state.get("suites")
        if not self.example_count:
            self.example_count = state.get("examples") or 0
        if self.parity_hash or self.suite_summary:
            log(f"Recovered recorded facts: fixed point {self.parity_hash or 'n/a'}, "
                f"tests {self.suite_summary or 'n/a'}")

    @staticmethod
    def _read_preserving(path: Path) -> str:
        # newline="" keeps the file's existing CRLF/LF endings intact so a version
        # bump does not rewrite every line of a document.
        with open(path, encoding="utf-8", newline="") as f:
            return f.read()

    @staticmethod
    def _write_preserving(path: Path, text: str):
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(text)

    def _bump_declarations(self):
        """Rewrite every version declaration to self.ver; return the list of edits."""
        changed = []
        for rel, pattern in VERSION_DECLARATIONS:
            path = ROOT / rel
            if not path.exists():
                continue
            text = self._read_preserving(path)
            new_text, n = re.subn(
                pattern,
                lambda m: m.group(0).replace(
                    re.search(r"\d+\.\d+\.\d+", m.group(0)).group(0), self.ver),
                text,
            )
            if n and new_text != text:
                self._write_preserving(path, new_text)
                changed.append(f"{rel} ({n} site{'s' if n > 1 else ''})")
        return changed

    def _read_declared_versions(self):
        found = {}
        for rel, pattern in VERSION_DECLARATIONS:
            path = ROOT / rel
            if not path.exists():
                continue
            for m in re.finditer(pattern, self._read_preserving(path)):
                found.setdefault(rel, set()).add(
                    re.search(r"\d+\.\d+\.\d+", m.group(0)).group(0))
        return found

    def stage_version(self):
        log("STAGE 2: Version bump & declaration consistency", header=True)
        if self.dry_run:
            log(f"[DRY-RUN] Would rewrite version declarations to {self.ver}.")
            return

        if self.no_bump:
            log("--no-bump requested; declarations are left untouched.")
        else:
            changed = self._bump_declarations()
            if changed:
                for item in changed:
                    log(f"  bumped {item}")
            else:
                log(f"Declarations already declare {self.ver}.")

        declared = self._read_declared_versions()
        stale = {
            rel: sorted(v - {self.ver}) for rel, v in declared.items() if v - {self.ver}
        }
        if stale:
            detail = "; ".join(f"{rel}: {', '.join(vs)}" for rel, vs in stale.items())
            raise RuntimeError(
                f"Version declarations do not match {self.tag} ({detail}). "
                f"Re-run without --no-bump or fix them by hand."
            )
        missing = [rel for rel, _ in VERSION_DECLARATIONS
                   if not (ROOT / rel).exists() or rel not in declared]
        if missing:
            raise RuntimeError(
                f"Missing version declarations in: {', '.join(sorted(set(missing)))}"
            )
        log(f"All {len(declared)} declared versions are {self.ver}.")

    def stage_build(self):
        log("STAGE 3: Self-hosting 3-Stage Bootstrap & Parity Verification", header=True)
        if self.dry_run:
            log("[DRY-RUN] Would execute 3-stage bootstrap fixed-point loop.")
            return

        target_dir = ROOT / "target"
        target_dir.mkdir(exist_ok=True)

        mwc_bin = ROOT / "bin" / "mwc.exe"
        stage1 = target_dir / "stage1.exe"
        stage2 = target_dir / "stage2.exe"
        stage3 = target_dir / "stage3.exe"

        log("Compiling stage 1 compiler (mwc -> stage1)...")
        run(f'"{mwc_bin}" compiler\\main.mw -o "{stage1}"')

        log("Compiling stage 2 compiler (stage1 -> stage2)...")
        run(f'"{stage1}" compiler\\main.mw -o "{stage2}"')

        log("Compiling stage 3 compiler (stage2 -> stage3)...")
        run(f'"{stage2}" compiler\\main.mw -o "{stage3}"')

        stage2_ll = target_dir / "stage2.exe.ll"
        stage3_ll = target_dir / "stage3.exe.ll"
        if not stage2_ll.exists() or not stage3_ll.exists():
            raise FileNotFoundError("Missing stage2.exe.ll or stage3.exe.ll")

        h2 = compute_sha256(stage2_ll)
        h3 = compute_sha256(stage3_ll)
        log(f"SHA256(stage2.ll) = {h2}")
        log(f"SHA256(stage3.ll) = {h3}")

        if h2 != h3:
            raise RuntimeError(f"Bootstrap fixed point diverged! {h2} != {h3}")

        self.parity_hash = h2
        self._save_state(parity=h2, version=self.ver)
        log("Bootstrap fixed-point closure verified (100% bitwise parity)!")

        # IR parity is necessary but not sufficient: stage2 and stage3 are linked from
        # the same .ll, so the executables themselves must match byte-for-byte. This is
        # what catches a non-deterministic linker (a PE TimeDateStamp, for instance),
        # which otherwise makes every rebuild dirty `bin/` with different bytes.
        exe2 = (target_dir / "stage2.exe").read_bytes()
        exe3 = (target_dir / "stage3.exe").read_bytes()
        if exe2 != exe3:
            off = next((i for i, (x, y) in enumerate(zip(exe2, exe3)) if x != y), None)
            raise RuntimeError(
                "Reproducibility failure: stage2.exe and stage3.exe are linked from "
                f"identical IR but differ ({len(exe2)} vs {len(exe3)} bytes"
                + (f", first difference at offset {off}" if off is not None else "")
                + "). Check the linker for timestamps or embedded paths."
            )
        log(f"stage2.exe == stage3.exe byte-for-byte ({len(exe2)} bytes).")

        # Update bin/
        shutil.copy2(stage3, ROOT / "bin" / "mwc.exe")
        shutil.copy2(stage3, ROOT / "bin" / "forge.exe")
        log("Updated bin/mwc.exe and bin/forge.exe with verified stage 3 binary.")

        # The freshly built binary must actually report the release version, and the
        # docs header must agree with it (same rule the CI gate enforces).
        res = run(f'"{ROOT / "bin" / "forge.exe"}" version', capture_output=True)
        reported = res.stdout.strip()
        log(f"Rebuilt toolchain reports: {reported}")
        if self.ver not in reported:
            raise RuntimeError(
                f"Rebuilt toolchain reports '{reported}' but the release tag is {self.tag}; "
                f"the version stage did not take effect."
            )
        header = (ROOT / "docs" / "SYNTAX.md").read_text(encoding="utf-8").splitlines()[0]
        if f"v{self.ver}" not in header:
            raise RuntimeError(
                f"docs/SYNTAX.md header does not declare v{self.ver}: {header}"
            )
        log(f"Toolchain and docs/SYNTAX.md agree on v{self.ver}.")

    def stage_test(self):
        log("STAGE 4: Running Test Suites & Example Builds", header=True)
        if self.dry_run:
            log("[DRY-RUN] Would run forge test and build examples.")
            return

        forge_bin = ROOT / "bin" / "forge.exe"
        log("Running native and compile-fail test suites...")
        res = run(f'"{forge_bin}" test', check=False, capture_output=True)
        combined = (res.stdout or "") + (res.stderr or "")
        log_file = ROOT / "target" / "pipeline-test.log"
        log_file.write_text(combined, encoding="utf-8")
        if res.returncode != 0:
            tail = "\n".join(combined.splitlines()[-40:])
            raise RuntimeError(
                f"forge test failed (exit {res.returncode}); last 40 lines of "
                f"{log_file}:\n{tail}"
            )
        summary = [l for l in combined.splitlines() if "test result:" in l]
        if not summary or "ok." not in summary[-1]:
            raise RuntimeError(f"forge test printed no successful summary (see {log_file})")
        self.suite_summary = summary[-1].strip()
        log(f"forge test: {self.suite_summary}")

        log("Building all shipped examples...")
        failed = []
        examples = sorted((ROOT / "examples").glob("*.mw"))
        for example in examples:
            out_exe = ROOT / "target" / f"release_test_{example.stem}.exe"
            res = run(f'"{forge_bin}" build "{example}" -o "{out_exe}"', check=False, capture_output=True)
            if res.returncode != 0:
                failed.append(example.name)
        if failed:
            raise RuntimeError(f"Examples failed to build: {', '.join(failed)}")
        self.example_count = len(examples)
        self._save_state(suites=self.suite_summary, examples=self.example_count)
        log(f"All {len(examples)} examples built successfully.")

    def stage_bench(self):
        log("STAGE 5: Runtime Benchmarking", header=True)
        if self.dry_run:
            log("[DRY-RUN] Would run forge bench.")
            return

        forge_bin = ROOT / "bin" / "forge.exe"
        run(f'"{forge_bin}" bench')
        log("Benchmarks completed successfully.")

    PRUNE_DIRS = {"target", "dist", "__pycache__", ".git", "node_modules", ".vs", "obj"}
    PRUNE_SUFFIXES = (".ll", ".obj", ".o", ".pdb", ".pyc", ".rlib", ".rmeta", ".lib", ".exp")
    # Fixed archive-entry metadata: without this, zip members record the mtime of the
    # working-tree files, so two packagings of identical content produce different
    # bytes and a released archive can never be reproduced or compared.
    ZIP_DATE = (1980, 1, 1, 0, 0, 0)

    @classmethod
    def _zip_add(cls, zf, path: Path, arcname: str):
        info = zipfile.ZipInfo(arcname.replace("\\", "/"), date_time=cls.ZIP_DATE)
        info.compress_type = zipfile.ZIP_DEFLATED
        info.create_system = 0          # MS-DOS, host independent
        info.external_attr = 0o644 << 16
        zf.writestr(info, path.read_bytes())

    def _distribution_files(self, root_dir: Path):
        """Files worth shipping: no nested build trees, no intermediates."""
        for root, dirs, files in os.walk(root_dir):
            dirs[:] = [d for d in sorted(dirs) if d not in self.PRUNE_DIRS]
            for name in sorted(files):
                if name.lower().endswith(self.PRUNE_SUFFIXES):
                    continue
                if name.lower().endswith(".exe") and Path(root) != ROOT / "bin":
                    continue
                yield Path(root) / name

    def stage_package(self):
        log("STAGE 6: Packaging Release Assets", header=True)
        if self.dry_run:
            log(f"[DRY-RUN] Would package release artifacts in {self.dist_dir}")
            return

        if self.dist_dir.exists() and (self.dist_dir / "PUBLISH.txt").exists():
            if not self.clobber:
                raise RuntimeError(
                    f"{self.dist_dir} already holds a published release record "
                    f"(PUBLISH.txt). Pass --clobber to rebuild and overwrite it, or "
                    f"cut a new version."
                )
            log(f"--clobber: overwriting the published artifacts in {self.dist_dir.name}/")
        if self.dist_dir.exists():
            shutil.rmtree(self.dist_dir)
        self.dist_dir.mkdir(parents=True, exist_ok=True)

        self._recover_facts()

        # 1. Standalone CLI binaries
        shutil.copy2(ROOT / "bin" / "forge.exe", self.dist_dir / "forge.exe")
        shutil.copy2(ROOT / "bin" / "mwc.exe", self.dist_dir / "mwc.exe")

        # 2. Complete distribution zip
        zip_name = f"Morrow-{self.tag}-win64.zip"
        zip_path = self.dist_dir / zip_name
        log(f"Creating distribution archive: {zip_name}...")

        include_dirs = ["bin", "std", "compiler", "lib", "examples", "tests", "benchmarks",
                        "docs", "scripts", "release-notes", "licenses", ".github"]
        include_files = ["README.md", "ROADMAP.md", "RELEASE_PROCESS.md",
                         "THIRD-PARTY-NOTICES.md", "Makefile", "LICENSE.md"]

        with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
            for fname in include_files:
                fpath = ROOT / fname
                if fpath.exists():
                    self._zip_add(zf, fpath, f"Morrow/{fname}")
            for dname in include_dirs:
                dpath = ROOT / dname
                if not dpath.exists():
                    continue
                for full_path in self._distribution_files(dpath):
                    rel_path = full_path.relative_to(ROOT)
                    self._zip_add(zf, full_path, f"Morrow/{rel_path.as_posix()}")

        zip_size_mb = zip_path.stat().st_size / (1024 * 1024)
        log(f"Packaged {zip_name} ({zip_size_mb:.2f} MB)")

        # 3. Release notes: authored notes win, otherwise a factual skeleton.
        notes_file = self.dist_dir / "RELEASE_NOTES.md"
        if self.notes_path.exists():
            shutil.copy2(self.notes_path, notes_file)
            log(f"Release notes taken from {self.notes_path.relative_to(ROOT)}")
        else:
            log(f"No release-notes/{self.tag}.md found; writing a generated skeleton.")
            notes_file.write_text(self._generated_notes(), encoding="utf-8")

    def _generated_notes(self) -> str:
        parity = self.parity_hash or "(build stage skipped in this run)"
        suites = self.suite_summary or "(test stage skipped in this run)"
        return f"""# Morrow {self.tag}

Built with the local release pipeline (`scripts/pipeline.py`, see `RELEASE_PROCESS.md`).

## Verification recorded by the pipeline

| Gate | Result |
|---|---|
| Self-hosting fixed point | `SHA256(stage2.exe.ll) == SHA256(stage3.exe.ll)` = `{parity}` |
| Test suites | {suites} |
| Distribution | `Morrow-{self.tag}-win64.zip`, `forge.exe`, `mwc.exe`, `SHA256SUMS.txt` |

> This skeleton was generated because `release-notes/{self.tag}.md` is missing.
> Commit authored notes for that tag to replace it.
"""

    def stage_verify(self):
        log("STAGE 7: Verification & Checksum Generation", header=True)
        if self.dry_run:
            log("[DRY-RUN] Would verify and hash release assets.")
            return

        required = [f"Morrow-{self.tag}-win64.zip", "forge.exe", "mwc.exe", "RELEASE_NOTES.md"]
        missing = [name for name in required if not (self.dist_dir / name).exists()]
        if missing:
            raise FileNotFoundError(
                f"Missing release assets in {self.dist_dir}: {', '.join(missing)}"
            )

        # Anything that is uploaded gets a checksum; the notes are the release body
        # (uploaded as an asset too, so they are hashed as well).
        assets = [p for p in sorted(self.dist_dir.glob("*"))
                  if p.is_file() and p.name != "SHA256SUMS.txt"]

        checksum_file = self.dist_dir / "SHA256SUMS.txt"
        lines = []
        for asset in sorted(assets):
            if asset.name == "SHA256SUMS.txt":
                continue
            sha = compute_sha256(asset)
            lines.append(f"{sha}  {asset.name}")
            log(f"  {asset.name} -> {sha}")

        with open(checksum_file, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
        log(f"Wrote {checksum_file}")

        # Cross-check against the local release record. On a --clobber rebuild this is
        # the proof that the artifacts are byte-identical to the ones already published,
        # or the warning that they are not.
        ledger = self._load_ledger()
        if ledger:
            recorded = {a["name"]: a["sha256"] for a in ledger.get("assets", [])}
            current = {a["name"]: a["sha256"] for a in self._asset_digests()}
            drift = sorted(n for n in current if n in recorded and recorded[n] != current[n])
            added = sorted(set(current) - set(recorded))
            removed = sorted(set(recorded) - set(current))
            if drift:
                if not self.clobber:
                    raise RuntimeError(
                        "Artifacts differ from the recorded publication of "
                        f"{self.tag} ({', '.join(drift)}). Pass --clobber to record the "
                        "new bytes, or investigate why the build changed."
                    )
                log("Warning: --clobber recording new bytes for: " + ", ".join(drift))
            if added or removed:
                log(f"Asset set changed vs {self.ledger_path.name}: "
                    f"+{added or '[]'} -{removed or '[]'}")
            if not drift and not added and not removed:
                log(f"Artifacts match the recorded publication of {self.tag} byte-for-byte.")

        # Verify zip integrity
        for z in self.dist_dir.glob("*.zip"):
            with zipfile.ZipFile(z, "r") as zf:
                bad_file = zf.testzip()
                if bad_file:
                    raise RuntimeError(f"Corrupted file in {z.name}: {bad_file}")
                # Licensing must travel with the artifacts: MUL requires its own
                # notice, and the bundled CRT imposes upstream attribution.
                names = set(zf.namelist())
                required = {
                    "Morrow/LICENSE.md",
                    "Morrow/THIRD-PARTY-NOTICES.md",
                    "Morrow/licenses/GPL-3.0.txt",
                    "Morrow/licenses/ZPL-2.1.txt",
                }
                missing = sorted(required - names)
                if missing:
                    raise RuntimeError(
                        f"{z.name} is missing licensing files: {', '.join(missing)}"
                    )
                log(f"Licensing present in {z.name}: LICENSE.md, "
                    f"THIRD-PARTY-NOTICES.md and {len([n for n in names if n.startswith('Morrow/licenses/')])} upstream texts")
            log(f"Verified archive integrity: {z.name}")

    def _release_title(self) -> str:
        if self.notes_path.exists():
            for line in self.notes_path.read_text(encoding="utf-8").splitlines():
                stripped = line.strip()
                if stripped.startswith("#"):
                    return stripped.lstrip("#").strip()
        return f"Morrow {self.tag}"

    def _split_dirty(self, lines):
        """Split `git status --porcelain` entries into pipeline-managed and not."""
        managed, unmanaged = [], []
        owned_files = {rel for rel, _ in VERSION_DECLARATIONS}
        owned_prefixes = ("bin/", "release-notes/", "releases/", "dist/")
        for line in lines:
            if len(line) < 4:
                continue
            # Porcelain v1 is exactly "XY<space>path"; never strip the leading space
            # of the first entry or a modified file loses its first character.
            path = line[3:].strip().strip('"')
            if " -> " in path:
                path = path.split(" -> ", 1)[1].strip().strip('"')
            path = path.replace("\\", "/")
            if path in owned_files or path.startswith(owned_prefixes):
                managed.append(path)
            else:
                unmanaged.append(path)
        return managed, unmanaged

    def _write_ledger(self, previous):
        """Record this release locally so the repo, not a host, holds the proof."""
        if not self.dist_dir.exists():
            raise FileNotFoundError(
                f"{self.dist_dir} does not exist; run the package stage first"
            )
        commit = run("git rev-parse HEAD", capture_output=True).stdout.strip()
        remote = self.remote or run(
            "git remote get-url origin", capture_output=True, check=False).stdout.strip()
        # Deliberately no commit hash in the record: the ledger is committed alongside the
        # release, so any hash it stored would be invalidated by its own commit (or an
        # amend) and every rerun would look like a change. git already answers
        # "which commit is this tag" -- the ledger answers "which bytes were published".
        entry = {
            "tag": self.tag,
            "version": self.ver,
            "remote": remote,
            "fixed_point": self.parity_hash,
            "tests": self.suite_summary,
            "examples": self.example_count or None,
            "notes": (f"release-notes/{self.tag}.md" if self.notes_path.exists() else None),
            "assets": self._asset_digests(),
            "published_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "republish_count": int(previous.get("republish_count", 0)) + (1 if previous else 0),
            "recorded_near_commit": commit,
            "previous_publication": (
                {
                    "published_at": previous.get("published_at"),
                    "assets": previous.get("assets"),
                }
                if previous else None
            ),
        }
        self.ledger_path.parent.mkdir(parents=True, exist_ok=True)

        # A republication that changes nothing must not create churn: if commit,
        # artifacts, fixed point and test summary all match the existing record,
        # leave the ledger (and therefore the tree) untouched.
        if previous:
            same = (
                previous.get("fixed_point") == entry["fixed_point"]
                and previous.get("tests") == entry["tests"]
                and previous.get("examples") == entry["examples"]
                and previous.get("assets") == entry["assets"]
            )
            if same:
                log(f"{self.ledger_path.name} already records this publication "
                    f"exactly (identical artifacts and verification facts); "
                    f"nothing to re-record.")
                return False

        self.ledger_path.write_text(json.dumps(entry, indent=2) + "\n", encoding="utf-8")
        log(f"Wrote local release record {self.ledger_path.relative_to(ROOT)} "
            f"(republish #{entry['republish_count']})")
        return True

    def stage_publish(self):
        log("STAGE 8: Publish Release", header=True)
        if self.dry_run:
            log("[DRY-RUN] Would tag and create/update the GitHub release.")
            return

        if self.no_publish:
            log("--no-publish requested; skipping git tag/push.")
            return

        self._recover_facts()

        # 0. Refuse to "re-publish" a tag that cannot describe what is being published,
        #    before touching the tree: otherwise a rerun commits regenerated binaries and
        #    only then discovers the tag cannot move, leaving a stray release commit.
        #    A tag at HEAD is the normal case; a tag that is an *ancestor* of HEAD is a
        #    republication, whose ledger commit necessarily lands on top of the release
        #    commit without moving the tag.
        head = run("git rev-parse HEAD", capture_output=True).stdout.strip()
        tag_exists = run(f"git rev-parse -q --verify refs/tags/{self.tag}",
                         check=False, capture_output=True).returncode == 0
        if tag_exists:
            tag_sha = run(f"git rev-list -n 1 {self.tag}", capture_output=True).stdout.strip()
            at_head = tag_sha == head
            ancestor = at_head or run(f"git merge-base --is-ancestor {self.tag} HEAD",
                                      check=False, capture_output=True).returncode == 0
            if not ancestor and not self.force:
                raise RuntimeError(
                    f"Tag {self.tag} points at {tag_sha[:8]}, which is not an ancestor "
                    f"of HEAD ({head[:8]}). Nothing was committed. Either check out the "
                    f"tagged commit, cut a new version, or pass --force to move the tag."
                )
            if not at_head and not self.clobber:
                raise RuntimeError(
                    f"Tag {self.tag} points at {tag_sha[:8]}, behind HEAD ({head[:8]}). "
                    f"That is a republication; pass --clobber to record it."
                )

        # 1b. An already-published version can only be refreshed deliberately.
        ledger = self._load_ledger()
        if ledger and not self.clobber:
            raise RuntimeError(
                f"{self.tag} is already published (local release record "
                f"{self.ledger_path.name}, publication "
                f"#{ledger.get('republish_count', 0)} at "
                f"{ledger.get('published_at')}). Pass --clobber to rebuild the "
                f"artifacts and record another publication."
            )
        self._write_ledger(ledger)

        # 1. The tag must describe the tree that produced the artifacts. Version
        #    bumps and rebuilt binaries are this pipeline's own output, so they are
        #    committed here; anything else must be a deliberate --force.
        dirty_lines = [l for l in run("git status --porcelain", capture_output=True)
                       .stdout.splitlines() if l.strip()]
        if dirty_lines:
            managed, unmanaged = self._split_dirty(dirty_lines)
            if unmanaged and not self.force:
                listing = "\n".join(unmanaged)
                raise RuntimeError(
                    "Uncommitted changes that the pipeline does not own; commit them "
                    "first or pass --force:\n" + listing
                )
            add_arg = "-A" if (self.force and unmanaged) else " ".join(
                f'"{p}"' for p in managed
            )
            if add_arg.strip():
                run(f"git add {add_arg}")
                suffix = " (republish)" if self.clobber else ""
                run(f'git commit -m "chore(release): {self.tag}{suffix}" -m '
                    f'"Version declarations, toolchain binaries and the local release '
                    f'record written by scripts/pipeline.py. Fixed point: '
                    f'{self.parity_hash or "n/a"}."')
                log(f"Committed {len(managed)} pipeline-managed change(s).")
        else:
            log("Working tree clean.")

        # 1. Tag HEAD (create, or move when --force).
        head = run("git rev-parse HEAD", capture_output=True).stdout.strip()
        res = run(f"git rev-parse -q --verify refs/tags/{self.tag}", check=False, capture_output=True)
        if res.returncode != 0:
            run(f'git tag -a {self.tag} -m "Morrow {self.tag}"')
            log(f"Created annotated tag {self.tag} at {head[:8]}")
        else:
            tag_sha = run(f"git rev-list -n 1 {self.tag}", capture_output=True).stdout.strip()
            if tag_sha != head:
                if self.force:
                    run(f'git tag -f -a {self.tag} -m "Morrow {self.tag}"')
                    log(f"Moved tag {self.tag} to {head[:8]}")
                elif self.clobber:
                    # Republication: the tag keeps identifying the release commit and
                    # the new publication record is committed on top of it.
                    log(f"Tag {self.tag} stays at {tag_sha[:8]} (release commit); "
                        f"publication recorded at {head[:8]}")
                else:
                    raise RuntimeError(
                        f"Tag {self.tag} points at {tag_sha[:8]} but HEAD is {head[:8]}; "
                        f"pass --force to move it, or --clobber to record a republication"
                    )
            else:
                log(f"Tag {self.tag} already points at HEAD")

        # 2. Push branch and tag (plain git; no CLI wrappers).
        branch = run("git branch --show-current", capture_output=True).stdout.strip() or "main"
        log(f"Pushing {branch} and {self.tag} to origin...")
        run(f"git push origin {branch}")
        run(f"git push origin {self.tag} --force")

        # 3. Prove the tag actually landed on the remote, comparing the peeled
        #    commit id reported by ls-remote with the commit we tagged. The peeled
        #    ref is matched in Python rather than on the command line, because "^" is
        #    a shell metacharacter on Windows and would silently drop the comparison.
        tag_obj = run(f"git rev-parse {self.tag}", capture_output=True).stdout.strip()
        res = run("git ls-remote --tags origin", capture_output=True, check=False)
        if res.returncode != 0:
            raise RuntimeError(f"git ls-remote failed:\n{res.stdout}{res.stderr}")
        tag_ref = f"refs/tags/{self.tag}"
        peeled_ref = tag_ref + "^{}"
        remote_obj, remote_sha = "", ""
        for line in res.stdout.splitlines():
            parts = line.split()
            if len(parts) != 2:
                continue
            if parts[1] == tag_ref:
                remote_obj = parts[0]
            elif parts[1] == peeled_ref:
                remote_sha = parts[0]
        if not remote_obj:
            raise RuntimeError(f"Remote does not report tag {self.tag} after push.")
        if remote_obj != tag_obj:
            raise RuntimeError(
                f"Remote tag object {remote_obj[:8]} != local {tag_obj[:8]} for {self.tag}"
            )
        # Compare against the commit the tag actually identifies, not HEAD: on a
        # republication the tag deliberately stays behind the record commit.
        tagged_commit = run(f"git rev-list -n 1 {self.tag}",
                            capture_output=True).stdout.strip()
        if remote_sha and remote_sha != tagged_commit:
            raise RuntimeError(
                f"Remote tag {self.tag} peels to {remote_sha[:8]}, expected "
                f"{tagged_commit[:8]}"
            )
        log(f"Remote tag {self.tag} verified "
            f"(object {remote_obj[:8]}, commit {remote_sha[:8] if remote_sha else tagged_commit[:8]})")

        # 4. Write the publish manifest next to the artifacts. Distribution artifacts
        #    are local by design; attaching them to a forge release page (if one is
        #    used at all) is a separate, optional step for whoever owns the fork.
        notes_file = self.dist_dir / "RELEASE_NOTES.md"
        if not notes_file.exists():
            raise FileNotFoundError(
                f"{notes_file} is missing; run the package stage before publishing"
            )
        remote = getattr(self, "remote", None) or run(
            "git remote get-url origin", capture_output=True, check=False).stdout.strip()
        manifest = self.dist_dir / "PUBLISH.txt"
        lines = [
            self._release_title(),
            "",
            f"commit:       {head}",
            f"branch:       {branch}",
            f"remote:       {remote}",
            f"fixed point:  {self.parity_hash or 'n/a'}",
            f"tests:        {self.suite_summary or 'n/a'}",
            "",
            "assets (SHA-256 in SHA256SUMS.txt):",
        ]
        for asset in sorted(self.dist_dir.glob("*")):
            if asset.is_file() and asset.name not in ("SHA256SUMS.txt", "PUBLISH.txt"):
                lines.append(f"  {asset.name}  ({asset.stat().st_size} bytes)")
        lines += [
            "",
            "Publishing = git tag + push. The toolchain that built these artifacts is",
            "checkout-able from that tag; no host service is required.",
            "",
        ]
        manifest.write_text("\n".join(lines), encoding="utf-8")
        log(f"Wrote {manifest.relative_to(ROOT)}")

        if "github.com" in remote:
            slug = remote.rstrip("/").removesuffix(".git").split("github.com")[-1].lstrip(":/")
            log(f"Tag page: https://github.com/{slug}/releases/tag/{self.tag} "
                f"(a tag, not a Release object -- nothing is uploaded to a host)")
        log(f"{self.tag} published: tag pushed and verified on origin ({len(list(self.dist_dir.glob('*')))} assets in {self.dist_dir.name}/).")


def main():
    parser = argparse.ArgumentParser(description="Morrow Release Pipeline")
    parser.add_argument("tag", help="Release tag (e.g. v2.3.0)")
    parser.add_argument("--dry-run", action="store_true", help="Preview stages without running commands")
    parser.add_argument("--from", dest="from_stage", choices=STAGES, default="preflight", help="Resume pipeline from stage")
    parser.add_argument("--skip", action="append", choices=STAGES, default=[], help="Skip specified stage (can be repeated)")
    parser.add_argument("--no-publish", action="store_true", help="Run all stages except git tag/push")
    parser.add_argument("--no-bump", action="store_true", help="Do not rewrite version declarations (must already be correct)")
    parser.add_argument("--force", action="store_true", help="Allow publishing a dirty tree / moving an existing tag")
    parser.add_argument("--clobber", action="store_true", help="Republish an already-released version: rebuild its artifacts and record another publication")

    args = parser.parse_args()

    pipeline = ReleasePipeline(tag=args.tag, dry_run=args.dry_run, no_publish=args.no_publish,
                               no_bump=args.no_bump, force=args.force, clobber=args.clobber)

    start_idx = STAGES.index(args.from_stage)
    active_stages = STAGES[start_idx:]

    log(f"Starting release pipeline for {pipeline.tag} from stage '{args.from_stage}'...")

    for stage in active_stages:
        if stage in args.skip:
            log(f"Skipping stage: {stage}")
            continue
        func = getattr(pipeline, f"stage_{stage}")
        func()

    log(f"Release pipeline for {pipeline.tag} completed successfully!", header=True)


if __name__ == "__main__":
    main()
