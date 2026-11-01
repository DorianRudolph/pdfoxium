#!/usr/bin/env python3
# Copyright (c) 2026 DorianRudolph
# SPDX-License-Identifier: MIT
"""Download the pinned native PDFium test oracle (Python standard library only)."""
import argparse
import hashlib
import platform
from pathlib import Path
import shutil
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
REVISION = "chromium-8100"
BASE_URL = "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8100"
PLATFORMS = {
    "linux-x64": {
        "library": "libpdfium.so",
        "archive_sha256": "78a68e46d871a52599c98af31a30a433f5441e670c03bba1f6b7a8444d645782",
        "library_sha256": "5fb3c50fd2b39cc479612c8c8100d83ea8ac6415c84020291e03bd252b57eab6",
    },
    "mac-arm64": {
        "library": "libpdfium.dylib",
        "archive_sha256": "6923e58ecb8dc332c01667efbbde258a71385fc3ef736bea4204b87c4d14a83c",
        "library_sha256": "a6c5d3f87e5ba0015d854eee6c9e7c27d2d5e2548ef9eeb5cd10c556b81cad7f",
    },
}


def detect_platform():
    host = (platform.system(), platform.machine().lower())
    if host[0] == "Linux" and host[1] in ("x86_64", "amd64"):
        return "linux-x64"
    if host[0] == "Darwin" and host[1] in ("arm64", "aarch64"):
        return "mac-arm64"
    raise ValueError(f"Unsupported oracle platform: {host[0]} {host[1]}; "
                     "use --platform to download for a supported target")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def download(target, force=False):
    config = PLATFORMS[target]
    destination = ROOT / "vendor/pdfium" / REVISION / target / config["library"]
    if not force and destination.is_file() and sha256(destination) == config["library_sha256"]:
        print(f"Verified existing oracle: {destination}")
        return destination

    cache = ROOT / "target/pdfium-downloads"
    cache.mkdir(parents=True, exist_ok=True)
    archive_name = f"pdfium-v8-{target}.tgz"
    archive_path = cache / archive_name
    with tempfile.TemporaryDirectory(prefix="download-", dir=cache) as folder:
        temporary = Path(folder)
        if not archive_path.is_file() or sha256(archive_path) != config["archive_sha256"]:
            url = f"{BASE_URL}/{archive_name}"
            print(f"Downloading {url}", flush=True)
            staged = temporary / archive_name
            with urllib.request.urlopen(url, timeout=60) as response, staged.open("wb") as output:
                shutil.copyfileobj(response, output)
            if sha256(staged) != config["archive_sha256"]:
                raise ValueError(f"Archive SHA-256 mismatch: {archive_name}")
            staged.replace(archive_path)

        # Read only the expected regular library member. Never extract archive
        # paths or links into the checkout; retained license notices stay intact.
        staged = temporary / config["library"]
        with tarfile.open(archive_path, "r:gz") as archive:
            member = archive.getmember("lib/" + config["library"])
            if not member.isfile():
                raise ValueError(f"Oracle library is not a regular archive member: {member.name}")
            with archive.extractfile(member) as source, staged.open("wb") as output:
                shutil.copyfileobj(source, output)
        if sha256(staged) != config["library_sha256"]:
            raise ValueError(f"Library SHA-256 mismatch: {config['library']}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        staged.chmod(0o755)
        staged.replace(destination)
    print(f"Installed verified oracle: {destination}")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=["auto", *PLATFORMS], default="auto",
                        help="target platform (default: autodetect this host)")
    parser.add_argument("--force", action="store_true",
                        help="reinstall even if the existing library is verified")
    args = parser.parse_args()
    try:
        target = detect_platform() if args.platform == "auto" else args.platform
        download(target, args.force)
    except (OSError, ValueError, KeyError, tarfile.TarError) as error:
        parser.exit(1, f"PDFium oracle download failed: {error}\n")


if __name__ == "__main__":
    main()
