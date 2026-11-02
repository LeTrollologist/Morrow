#!/usr/bin/env python3
"""
Attach the locally built release artifacts to the GitHub release page for a tag.

This script does NOT build, test, compile or run anything on the host's side. The
bytes it uploads were already built, verified and recorded by scripts/pipeline.py on
this machine; GitHub only stores them so humans have somewhere to download from. No
Actions workflow, no CI runner and no forge CLI is involved -- it speaks the REST API
directly with the same credential `git push` already uses.

Usage:
  python scripts/release_page.py v2.4.4
  python scripts/release_page.py --all-missing
  python scripts/release_page.py v2.4.4 --dry-run
"""

import argparse
import hashlib
import json
import mimetypes
import os
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
API = "https://api.github.com"


def log(msg):
    print(f"[RELEASE] {msg}")


def get_token():
    for var in ("MORROW_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"):
        val = os.environ.get(var)
        if val and val.strip():
            return val.strip()
    # Fall back to the credential git already uses for pushing.
    try:
        res = subprocess.run(
            ["git", "credential", "fill"], cwd=ROOT, text=True, capture_output=True,
            input="protocol=https\nhost=github.com\n\n", check=True,
        )
    except Exception:
        return None
    for line in res.stdout.splitlines():
        if line.startswith("password="):
            return line.split("=", 1)[1].strip()
    return None


def repo_slug():
    out = subprocess.run(["git", "remote", "get-url", "origin"], cwd=ROOT,
                         text=True, capture_output=True, check=True).stdout.strip()
    m = re.search(r"github\.com[:/]+(.+?)(?:\.git)?/?$", out)
    if not m:
        raise SystemExit(f"origin is not a GitHub remote: {out}")
    return m.group(1)


def call(tok, method, url, payload=None, raw=None, content_type="application/json"):
    data = raw if raw is not None else (json.dumps(payload).encode() if payload is not None else None)
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"token {tok}")
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("User-Agent", "morrow-local-pipeline")
    if data is not None:
        req.add_header("Content-Type", content_type)
    try:
        with urllib.request.urlopen(req, timeout=300) as resp:
            body = resp.read()
            return resp.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as exc:
        body = exc.read()
        try:
            parsed = json.loads(body) if body else {}
        except ValueError:
            parsed = {"message": body.decode(errors="replace")}
        return exc.code, parsed



def notes_body(tag):
    path = ROOT / "release-notes" / f"{tag}.md"
    if not path.exists():
        raise SystemExit(f"missing {path}; cannot publish a release page without notes")
    text = path.read_text(encoding="utf-8")
    title = None
    for line in text.splitlines():
        if line.strip().startswith("#"):
            title = line.strip().lstrip("#").strip()
            break
    return text, (title or f"Morrow {tag}")


def assets_for(tag):
    dist = ROOT / "dist" / tag
    if not dist.exists():
        return []
    return [p for p in sorted(dist.glob("*"))
            if p.is_file() and p.name != "PUBLISH.txt"]


def ensure_release(tok, slug, tag, dry_run):
    body, title = notes_body(tag)
    status, existing = call(tok, "GET", f"{API}/repos/{slug}/releases/tags/{tag}")
    if status == 200:
        log(f"{tag}: release exists (id {existing['id']}); updating title and notes")
        if not dry_run:
            st, res = call(tok, "PATCH", f"{API}/repos/{slug}/releases/{existing['id']}",
                           {"name": title, "body": body, "draft": False, "prerelease": False})
            if st not in (200, 201):
                raise SystemExit(f"update failed: {st} {res.get('message')}")
        return existing
    if status != 404:
        raise SystemExit(f"cannot look up {tag}: {status} {existing.get('message')}")
    log(f"{tag}: creating release '{title}' at tag {tag}")
    if dry_run:
        return {"id": 0, "upload_url": "https://uploads.github.com/repos/x/releases/0/assets{?name,label}"}
    st, res = call(tok, "POST", f"{API}/repos/{slug}/releases",
                   {"tag_name": tag, "name": title, "body": body,
                    "draft": False, "prerelease": False})
    if st not in (200, 201):
        raise SystemExit(f"create failed: {st} {res.get('message')}")
    log(f"{tag}: release created (id {res['id']})")
    return res


def dist_is_consistent(tag, files):
    """Only ship artifacts we can prove match the recorded checksums.

    A release page is a distribution channel; uploading a zip whose hash disagrees with
    the SHA256SUMS.txt next to it would be worse than shipping no binary at all.
    """
    sums = ROOT / "dist" / tag / "SHA256SUMS.txt"
    if not sums.exists():
        return False, "no SHA256SUMS.txt"
    recorded = {}
    for line in sums.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) == 2:
            recorded[parts[1]] = parts[0].upper()
    if not recorded:
        return False, "empty SHA256SUMS.txt"
    for path in files:
        want = recorded.get(path.name)
        if want is None:
            continue
        got = hashlib.sha256(path.read_bytes()).hexdigest().upper()
        if got != want:
            return False, f"{path.name} hashes {got[:16]}… but SHA256SUMS.txt says {want[:16]}…"
    return True, ""


def upload_assets(tok, slug, tag, release, dry_run):
    files = assets_for(tag)
    if not files:
        log(f"{tag}: no local artifacts in dist/{tag}; release page left with notes only")
        return 0
    ok, why = dist_is_consistent(tag, files)
    if not ok:
        log(f"{tag}: local artifacts do not match their recorded checksums ({why}); "
            f"publishing the release page WITHOUT binaries rather than shipping unverified bytes")
        return 0
    status, current = call(tok, "GET",
                           f"{API}/repos/{slug}/releases/{release['id']}/assets?per_page=100")
    existing = {a["name"]: a["id"] for a in current} if status == 200 else {}
    upload_url = release["upload_url"].split("{")[0]
    uploaded = 0
    for path in files:
        if path.name in existing:
            log(f"{tag}: replacing existing asset {path.name}")
            if not dry_run:
                call(tok, "DELETE", f"{API}/repos/{slug}/releases/assets/{existing[path.name]}")
        ctype = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        url = f"{upload_url}?name={urllib.parse.quote(path.name)}"
        if dry_run:
            log(f"{tag}: would upload {path.name} ({path.stat().st_size} bytes)")
            continue
        st, res = call(tok, "POST", url, raw=path.read_bytes(), content_type=ctype)
        if st not in (200, 201):
            raise SystemExit(f"upload of {path.name} failed: {st} {res.get('message')}")
        log(f"{tag}: uploaded {path.name} ({path.stat().st_size} bytes)")
        uploaded += 1
    return uploaded


def tags_missing(tok, slug):
    tags = subprocess.run(["git", "tag", "--list", "v*", "--sort=v:refname"], cwd=ROOT,
                          text=True, capture_output=True, check=True).stdout.split()
    missing = []
    for tag in tags:
        st, _ = call(tok, "GET", f"{API}/repos/{slug}/releases/tags/{tag}")
        if st == 404:
            missing.append(tag)
        elif st != 200:
            raise SystemExit(f"cannot look up {tag}: {st}")
    return missing


def main():
    ap = argparse.ArgumentParser(description="Upload locally built artifacts to GitHub release pages")
    ap.add_argument("tag", nargs="?")
    ap.add_argument("--all-missing", action="store_true", help="every local tag without a release page")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    if not args.tag and not args.all_missing:
        ap.error("give a tag or --all-missing")

    tok = get_token()
    if not tok and not args.dry_run:
        raise SystemExit("no GitHub token: set MORROW_GITHUB_TOKEN or make sure 'git push' works")
    slug = repo_slug()
    log(f"repository {slug}")
    targets = tags_missing(tok, slug) if args.all_missing else [args.tag]
    if args.all_missing and not targets:
        log("every tag already has a release page")
    for tag in targets:
        if not (ROOT / "release-notes" / f"{tag}.md").exists():
            log(f"{tag}: skipped (no release-notes/{tag}.md in this repository)")
            continue
        release = ensure_release(tok, slug, tag, args.dry_run)
        upload_assets(tok, slug, tag, release, args.dry_run)
    log("done")


if __name__ == "__main__":
    main()
