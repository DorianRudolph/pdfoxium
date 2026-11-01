#!/usr/bin/env python3
"""Fail on compiler warnings in workspace/local patched crates, without linting dependencies."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
WASM_TOOLCHAIN = "nightly-2026-10-29"  # Keep aligned with build-viewer-wasm.sh.


def check(command):
    print("+ " + " ".join(command), flush=True)
    warnings = 0
    with subprocess.Popen(command + ["--message-format=json"], cwd=ROOT,
                          stdout=subprocess.PIPE, text=True) as process:
        for line in process.stdout:
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                print(line, end="")
                continue
            if item.get("reason") != "compiler-message":
                continue
            message = item["message"]
            if message.get("rendered"):
                print(message["rendered"], end="", file=sys.stderr)
            if (message["level"] == "warning"
                    and item["package_id"].startswith("path+")):
                warnings += 1
        status = process.wait()
    if warnings:
        print(f"{warnings} compiler warning(s) in local crates", file=sys.stderr)
    return status or bool(warnings)


def check_manifests():
    # Manifest diagnostics are plain stderr, outside Cargo's compiler JSON.
    # Check them explicitly so packaging metadata cannot flood normal builds.
    metadata = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1", "--color", "never"],
        cwd=ROOT, capture_output=True, text=True)
    if metadata.stderr:
        print(metadata.stderr, end="", file=sys.stderr)
    if metadata.returncode or any(line.startswith("warning:") for line in metadata.stderr.splitlines()):
        return metadata.returncode or 1
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wasm", action="store_true", help="check the threaded browser viewer")
    args = parser.parse_args()
    status = check_manifests()
    if status:
        return status
    if args.wasm:
        commands = [["cargo", f"+{WASM_TOOLCHAIN}", "check", "--locked", "--release",
                     "--target", "wasm32-unknown-unknown", "-Z", "build-std=std,panic_abort",
                     "-p", "pdfoxium-viewer", "--features", "web-test"]]
    else:
        # moxcms exposes an upstream nightly-only SVE feature. Keep all engine
        # features on stable, then check the colour crate's targets separately
        # with the same defaults/options/direct pipeline used by the core.
        commands = [["cargo", "check", "--locked", "--workspace", "--all-targets",
                     "--all-features", "--exclude", "pdfoxium-moxcms"],
                    ["cargo", "check", "--locked", "-p", "pdfoxium-moxcms",
                     "--all-targets", "--features", "options,any_to_any"]]
        for features in ([], ["--features", "js"], ["--features", "xfa"]):
            commands.append(["cargo", "check", "--locked", "-p", "pdfoxium-core",
                             "-p", "pdfoxium-capi", "--all-targets", "--no-default-features"]
                            + features)
    return any(check(command) for command in commands)


if __name__ == "__main__":
    sys.exit(main())
