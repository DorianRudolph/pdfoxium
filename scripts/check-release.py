#!/usr/bin/env python3
"""Check release inputs and optionally build/verify all Cargo archives locally.

Never publishes, tags, pushes, or creates a GitHub release.
"""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def require_clean_package_sources():
    changes = subprocess.check_output(
        ["git", "status", "--porcelain=v1", "--untracked-files=all"], cwd=ROOT)
    if changes:
        raise SystemExit("Packaging requires a clean source tree; commit the changes or use "
                         "--allow-dirty:\n" + changes.decode(errors="replace"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", action="store_true", help="Build and verify every publishable crate archive")
    parser.add_argument("--allow-dirty", action="store_true", help="Permit packaging a working tree under review")
    args = parser.parse_args()
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
    assert not workspace.get("patch"), "Correctness fixes must not depend on consumer workspace patches"
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT))
    packages = metadata["packages"]
    names = {p["name"] for p in packages}
    for package in packages:
        manifest = Path(package["manifest_path"])
        data = tomllib.loads(manifest.read_text())
        assert package["repository"], f"Missing repository: {manifest}"
        assert package["description"], f"Missing description: {manifest}"
        assert package["license"], f"Missing SPDX license: {manifest}"
        assert "license-file" not in data["package"], f"Use SPDX metadata and ordinary notice files: {manifest}"
        if package["publish"] != []:
            assert "THIRD_PARTY_LICENSES.md" in data["package"].get("include", []), manifest
        for dependency in package["dependencies"]:
            assert not (dependency["source"] or "").startswith("git+"), dependency
            if dependency.get("path") and package["publish"] != []:
                # Cargo strips unversioned dev-only paths (e.g. the native oracle).
                if dependency["kind"] == "dev" and dependency["req"] == "*":
                    continue
                assert dependency["req"] != "*", f"Unversioned dependency: {manifest}"
                assert dependency["name"] in names, f"Unpackaged dependency: {dependency}"
    pinned = ROOT / "upstream/pdfium/public"
    bundled = ROOT / "crates/pdfoxium-capi/include/public"
    expected = {p.name: p.read_bytes() for p in pinned.glob("*.h")}
    actual = {p.name: p.read_bytes() for p in bundled.glob("*.h")}
    assert expected and expected == actual, "Bundled C headers differ from the pinned PDFium headers"
    run("python3", "scripts/generate-licenses.py", "--check")
    if args.package:
        if not args.allow_dirty:
            require_clean_package_sources()
        run("python3", "scripts/generate-licenses.py", "--sync-packages")
        # Explicit Cargo include lists make even ignored generated notices
        # count as dirty. Check the sources before staging, then permit these
        # ordinary package files in the archives.
        command = ["cargo", "package", "--workspace", "--exclude", "pdfoxium-diff",
                   "--locked", "--no-verify", "--allow-dirty"]
        run(*command)
        # Cargo's temporary registry can reuse extracted sources from a previous
        # package with the same name/version, even after its tarball changes.
        # Verify freshly extracted archives together, patching only these exact
        # not-yet-published artifacts into an isolated consumer workspace.
        publishable = [p for p in packages if p["publish"] != []]
        for package in packages:
            if package["publish"] == []:
                continue
            stem = f"{package['name']}-{package['version']}"
            archive = ROOT / "target/package" / (stem + ".crate")
            with tarfile.open(archive) as tar:
                for member in tar.getmembers():
                    path = Path(member.name)
                    assert path.name != ".DS_Store" and "__pycache__" not in path.parts, archive
                    assert path.suffix not in (".pyc", ".pyo", ".pyd"), archive
                text = tar.extractfile(stem + "/THIRD_PARTY_LICENSES.md").read()
                assert text == (ROOT / "target/licenses/THIRD_PARTY_LICENSES.md").read_bytes(), archive
                manifest = tomllib.loads(tar.extractfile(stem + "/Cargo.toml").read().decode())
                assert not manifest.get("patch"), archive
                assert manifest["package"].get("license"), archive
                assert "license-file" not in manifest["package"], archive
        with tempfile.TemporaryDirectory(prefix="release-check-", dir=ROOT / "target") as folder:
            stage = Path(folder)
            members = []
            patches = []
            for package in publishable:
                stem = f"{package['name']}-{package['version']}"
                with tarfile.open(ROOT / "target/package" / (stem + ".crate")) as tar:
                    tar.extractall(stage, filter="data")
                members.append(stem)
                patches.append(f"{package['name']} = {{ path = {json.dumps(stem)} }}")
            (stage / "Cargo.toml").write_text(
                '[workspace]\nresolver = "2"\nmembers = ' + json.dumps(members)
                + '\n[patch.crates-io]\n' + '\n'.join(patches) + '\n')
            shutil.copyfile(ROOT / "Cargo.lock", stage / "Cargo.lock")
            staged = json.loads(subprocess.check_output(
                ["cargo", "metadata", "--offline", "--format-version", "1", "--manifest-path", str(stage / "Cargo.toml")],
                cwd=ROOT))
            locked = tomllib.loads((ROOT / "Cargo.lock").read_text())
            originals = {(p["name"], p["version"], p.get("source")) for p in locked["package"]}
            for package in staged["packages"]:
                if package["source"]:
                    assert (package["name"], package["version"], package["source"]) in originals, package
            run("cargo", "build", "--workspace", "--locked", "--offline", "--manifest-path", str(stage / "Cargo.toml"),
                "--target-dir", str(ROOT / "target"))
        print("All release archives built, verified and contain the complete license document.")
    print("Release input checks passed; nothing was published.")


if __name__ == "__main__":
    main()
