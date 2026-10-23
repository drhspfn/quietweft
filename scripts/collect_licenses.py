"""Collect dependency notices from locked Cargo metadata and installed npm packages.

Run after `npm ci` and with Cargo available. This does not copy package sources.
The index includes all Cargo-resolved platforms, which is broader than one binary.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
NOTICE = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense|authors)(?:[._-].*)?$", re.I)


def notice_files(folder: Path, declared: str | None = None) -> list[Path]:
    files = set()
    if declared:
        item = (folder / declared).resolve()
        if item.is_file():
            files.add(item)
    for item in folder.iterdir():
        if item.is_file() and NOTICE.match(item.name):
            files.add(item)
        elif item.is_dir() and item.name.lower() in {"license", "licenses", "licence", "licences"}:
            files.update(p for p in item.rglob("*") if p.is_file())
    return sorted(files)


def copy_notices(files: list[Path], folder: Path, destination: Path, output: Path) -> list[dict]:
    records = []
    for file in files:
        # Declared license files may live in an ancestor; never publish local paths.
        try:
            relative = file.relative_to(folder)
        except ValueError:
            relative = Path(file.name)
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(file, target)
        records.append({"path": target.relative_to(output).as_posix(), "sha256": hashlib.sha256(file.read_bytes()).hexdigest()})
    return records


def fetch_notices(package: dict, folder: Path, destination: Path, output: Path, cache: dict) -> list[dict]:
    repository = (package.get("repository") or "").removesuffix(".git").rstrip("/")
    match = re.fullmatch(r"https://github.com/([^/]+/[^/]+)", repository)
    vcs_file = folder / ".cargo_vcs_info.json"
    if not match or not vcs_file.is_file():
        return []
    vcs = json.loads(vcs_file.read_text(encoding="utf-8"))
    commit = vcs.get("git", {}).get("sha1", "")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        return []
    coordinate = (match[1], commit)
    if coordinate not in cache:
        url = f"https://api.github.com/repos/{match[1]}/git/trees/{commit}"
        try:
            request = Request(url, headers={"User-Agent": "Quietweft-license-collector/1.4.4"})
            with urlopen(request, timeout=25) as response:
                cache[coordinate] = json.load(response).get("tree", [])
        except (HTTPError, URLError, TimeoutError):
            cache[coordinate] = []
    records = []
    for item in cache[coordinate]:
        name = item["path"]
        if item["type"] != "blob" or not NOTICE.match(name):
            continue
        url = f"https://raw.githubusercontent.com/{match[1]}/{commit}/{quote(name)}"
        try:
            with urlopen(url, timeout=25) as response:
                content = response.read(512_001)
            if len(content) > 512_000:
                continue
        except (HTTPError, URLError, TimeoutError):
            continue
        destination.mkdir(parents=True, exist_ok=True)
        target = destination / name
        target.write_bytes(content)
        records.append({"path": target.relative_to(output).as_posix(), "sha256": hashlib.sha256(content).hexdigest(), "upstreamUrl": url, "upstreamCommit": commit, "note": "Repository-root notice; omitted from published crate archive"})
    return records


def collect(output: Path, fetch_missing: bool = False) -> dict:
    output.mkdir(parents=True, exist_ok=True)
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo.exe")
    rust = {}
    upstream_cache = {}
    for manifest in (ROOT / "rust/Cargo.toml", ROOT / "apps/desktop/src-tauri/Cargo.toml"):
        data = json.loads(subprocess.check_output([cargo, "metadata", "--locked", "--format-version", "1", "--manifest-path", str(manifest)], cwd=ROOT))
        for package in data["packages"]:
            if package["source"] is None:
                continue  # First-party license lives at the repository root.
            key = (package["name"], package["version"], package["source"])
            if key in rust:
                rust[key]["manifests"].append(manifest.relative_to(ROOT).as_posix())
                continue
            folder = Path(package["manifest_path"]).parent
            name = f'{package["name"]}-{package["version"]}'
            files = copy_notices(notice_files(folder, package.get("license_file")), folder, output / "rust" / name, output)
            if not files and fetch_missing:
                files = fetch_notices(package, folder, output / "rust" / name, output, upstream_cache)
            rust[key] = {"ecosystem": "rust", "name": package["name"], "version": package["version"], "license": package.get("license"), "licenseFile": package.get("license_file"), "source": package["source"], "repository": package.get("repository"), "manifests": [manifest.relative_to(ROOT).as_posix()], "files": files}
            # license_file is metadata, but must not expose an absolute source path.
            if rust[key]["licenseFile"] and Path(rust[key]["licenseFile"]).is_absolute():
                rust[key]["licenseFile"] = Path(rust[key]["licenseFile"]).name
    node = []
    desktop = ROOT / "apps/desktop"
    lock = json.loads((desktop / "package-lock.json").read_text(encoding="utf-8"))
    for relative, package in sorted(lock["packages"].items()):
        if not relative or package.get("dev"):
            continue
        folder = desktop / relative
        metadata = json.loads((folder / "package.json").read_text(encoding="utf-8"))
        name = metadata["name"]
        safe_name = name.replace("@", "").replace("/", "--") + "-" + package["version"]
        files = copy_notices(notice_files(folder), folder, output / "node" / safe_name, output)
        node.append({"ecosystem": "node", "name": name, "version": package["version"], "license": metadata.get("license", package.get("license")), "source": package.get("resolved"), "integrity": package.get("integrity"), "files": files})
    packages = sorted(rust.values(), key=lambda p: (p["name"], p["version"])) + node
    missing = [{"ecosystem": p["ecosystem"], "name": p["name"], "version": p["version"], "license": p["license"]} for p in packages if not p["files"]]
    report = {"schemaVersion": 1, "scope": "Cargo locked resolution across platforms for workspace and desktop; npm production dependencies only", "packages": packages, "missingLicenseTexts": missing}
    (output / "index.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    lines = ["# Dependency license texts", "", report["scope"] + ". Build dependencies are included in Cargo metadata; this is broader than the dependencies linked into a particular binary.", "", "Generated by `python scripts/collect_licenses.py --fetch-missing`. This optional flag recovers omitted root notices from the original GitHub repository at the exact commit recorded by Cargo. License expressions, source coordinates, retrieved URLs and text hashes are retained in [index.json](index.json). Runtime binary notices (ONNX Runtime, DirectML and MSVC) are maintained separately.", "", "| Ecosystem | Package | License metadata | Texts |", "|---|---|---|---|"]
    for p in packages:
        links = ", ".join(f'[{Path(f["path"]).name}]({f["path"]})' for f in p["files"]) or "**Missing from package**"
        lines.append(f'| {p["ecosystem"]} | {p["name"]} {p["version"]} | {p["license"] or "Not declared"} | {links} |')
    (output / "README.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "licenses")
    parser.add_argument("--fetch-missing", action="store_true", help="Fetch omitted repository-root notices at Cargo's exact recorded Git commit")
    args = parser.parse_args()
    result = collect(args.output.resolve(), args.fetch_missing)
    print(json.dumps({"packages": len(result["packages"]), "missingLicenseTexts": result["missingLicenseTexts"]}, indent=2))
