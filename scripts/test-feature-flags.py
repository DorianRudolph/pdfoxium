#!/usr/bin/env python3
"""Validate the PDF/JS/XFA Cargo feature matrix, dependencies and C exports."""

import argparse
import ctypes
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CONFIGS = (("pdf", None), ("js", "js"), ("xfa", "xfa"), ("default", "default"))
XFA_EXPORTS = ("FPDF_BStr_Init", "FPDF_BStr_Set", "FPDF_BStr_Clear")
COMMON_EXPORTS = (
    "FPDF_CreateNewDocument", "FPDF_LoadXFA", "FPDF_RenderPageBitmap",
    "FPDFDoc_GetJavaScriptActionCount", "FPDFJavaScriptAction_GetScript",
    "FPDF_GetXFAPacketCount", "FPDF_GetXFAPacketContent",
)


def flags(features):
    if features == "default":
        return []
    return ["--no-default-features"] + (["--features", features] if features else [])


def exports(library, xfa):
    # A fresh process per shared library avoids mixing independent Rust-library
    # lifetimes. Semantic rendering/form/action tests run in cargo test below.
    loaded = ctypes.CDLL(str(Path(library).resolve()))
    for name in COMMON_EXPORTS:
        assert hasattr(loaded, name), f"missing common export {name}"
    for name in XFA_EXPORTS:
        assert hasattr(loaded, name) == xfa, f"unexpected XFA export state: {name}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/feature-flags")
    parser.add_argument("--config", nargs="+", choices=[name for name, _ in CONFIGS],
                        help="rerun selected configurations")
    parser.add_argument("--exports", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--xfa", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.exports:
        exports(args.exports, args.xfa)
        return
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT,
    ))
    for name in ("pdfoxium-core", "pdfoxium-capi"):
        package = next(p for p in metadata["packages"] if p["name"] == name)
        assert "js" in package["features"]["xfa"], f"{name}: xfa must imply js"
    target = Path(metadata["target_directory"])
    library = "pdfoxium_capi.dll" if sys.platform == "win32" else (
        "libpdfoxium_capi.dylib" if sys.platform == "darwin" else "libpdfoxium_capi.so"
    )
    selected = args.config or [name for name, _ in CONFIGS]
    summary = output / "summary.json"
    previous = json.loads(summary.read_text()) if summary.exists() else []
    rows = [row for row in previous if row["configuration"] not in selected]
    summary.write_text(json.dumps(rows, indent=2) + "\n")
    for name, features in CONFIGS:
        if name not in selected:
            continue
        feature_flags = flags(features)
        graph = subprocess.check_output(
            ["cargo", "tree", "-p", "pdfoxium-capi", "--edges", "normal"] + feature_flags,
            cwd=ROOT, text=True,
        )
        (output / f"{name}-dependencies.txt").write_text(graph)
        js, xfa = name != "pdf", name in ("xfa", "default")
        for dependency in ("boa_engine v", "boa_gc v"):
            assert (dependency in graph) == js, f"{name}: incorrect {dependency} dependency"
        for phase, command in (
            ("tests", ["cargo", "test", "-p", "pdfoxium-core", "-p", "pdfoxium-capi", "--lib"]),
            ("build", ["cargo", "build", "-p", "pdfoxium-capi"]),
        ):
            print(f"{name}: {phase}", flush=True)
            with (output / f"{name}-{phase}.log").open("w") as log:
                subprocess.run(command + feature_flags, cwd=ROOT, stdout=log,
                               stderr=subprocess.STDOUT, check=True)
        snapshot = output / "libraries" / name / library
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(target / "debug" / library, snapshot)
        subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "--exports", str(snapshot)]
            + (["--xfa"] if xfa else []), cwd=ROOT, check=True,
        )
        rows.append(dict(configuration=name, js=js, xfa=xfa, result="passed"))
        summary.write_text(json.dumps(rows, indent=2) + "\n")
    # SIMD is independent of interpreter selection. The full SIMD path runs
    # with defaults above; compile the two reduced configurations as well.
    for features in ("simd", "simd,js"):
        with (output / (features.replace(",", "-") + "-check.log")).open("w") as log:
            subprocess.run(
                ["cargo", "check", "-p", "pdfoxium-core", "-p", "pdfoxium-capi", "--all-targets"]
                + flags(features), cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True,
            )
    # Cargo can unify core features through another workspace dependency.
    # The C API's own disabled-js contract must still reject host opt-in.
    with (output / "unified-core-tests.log").open("w") as log:
        subprocess.run(
            ["cargo", "test", "-p", "pdfoxium-capi", "--lib", "--no-default-features",
             "--features", "pdfoxium-core/xfa", "feature_tests"], cwd=ROOT,
            stdout=log, stderr=subprocess.STDOUT, check=True,
        )
    if "default" not in selected:
        # Leave the ordinary library in the canonical debug location even
        # after a focused reduced-feature rerun.
        with (output / "restore-default.log").open("w") as log:
            subprocess.run(["cargo", "build", "-p", "pdfoxium-capi"], cwd=ROOT,
                           stdout=log, stderr=subprocess.STDOUT, check=True)
    print("Feature matrix passed", flush=True)


if __name__ == "__main__":
    main()
