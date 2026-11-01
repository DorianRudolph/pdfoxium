#!/usr/bin/env python3
# Copyright (c) 2026 DorianRudolph
# SPDX-License-Identifier: MIT
"""Profile repeated searches on retained, evenly sampled real-book text pages.

Default: macOS sample stacks. Use --no-sample for unprofiled split-stage timing.
Document/page loading and text analysis finish before the ready/go handshake.
"""
import argparse
import hashlib
import json
import math
import os
import platform
import shlex
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOOKS = ["hefferon-toc", "obp.0526", "openstax-biology-2e"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--book", choices=BOOKS, action="append")
    parser.add_argument("--query", action="append")
    parser.add_argument("--flags", type=int, choices=[0, 1], default=0)
    parser.add_argument("--seconds", type=float, default=8)
    parser.add_argument("--pages", type=int, default=40)
    parser.add_argument("--repeats", type=int, default=1)
    parser.add_argument("--no-sample", action="store_true")
    parser.add_argument("--native-library", type=Path, help="override first library (also supports a preserved baseline)")
    parser.add_argument("--rust-library", type=Path, help="override second library")
    args = parser.parse_args()
    if not math.isfinite(args.seconds) or args.seconds <= 0 or args.pages < 1 or args.repeats < 1:
        parser.error("duration, pages and repeats must be positive")
    if not args.no_sample and platform.system() != "Darwin":
        parser.error("sample stacks require macOS; use --no-sample for timing")
    queries = args.query or ["function"]
    if any(not q or "\0" in q for q in queries):
        parser.error("invalid query")
    for book in args.book or BOOKS:
        path = ROOT / f"tests/render_corpus/books/{book}.pdf"
        if not path.is_file():
            parser.error(f"missing PDF: {path}; download the books first: cargo run -p pdfoxium-diff -- fetch books")
    out = args.output.resolve()
    if out.exists() and any(out.iterdir()):
        parser.error("output must be empty")
    out.mkdir(parents=True, exist_ok=True)
    compiler = shlex.split(os.environ.get("CXX", "c++"))
    worker = out / "text-benchmark"
    build = [*compiler, "-std=c++17", "-O3", "-g", "-DNDEBUG", "-Wall", "-Wextra", "-Werror",
             "-I", str(ROOT / "upstream/pdfium/public"), str(ROOT / "tests/capi/text_benchmark.cpp"), "-o", str(worker)]
    if platform.system() == "Linux":
        build.append("-ldl")
    subprocess.run(build, check=True)
    if platform.system() == "Darwin":
        libraries = {"native": ROOT / "vendor/pdfium/chromium-8100/mac-arm64/libpdfium.dylib",
                     "rust": ROOT / "target/release/libpdfoxium_capi.dylib"}
    else:
        libraries = {"native": ROOT / "vendor/pdfium/chromium-8100/linux-x64/libpdfium.so",
                     "rust": ROOT / "target/release/libpdfoxium_capi.so"}
    if args.native_library:
        libraries["native"] = args.native_library.resolve()
    if args.rust_library:
        libraries["rust"] = args.rust_library.resolve()
    metadata = {"platform": platform.platform(), "build": build,
                "git_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "worker_source_sha256": digest(ROOT / "tests/capi/text_benchmark.cpp"),
                "driver_sha256": digest(Path(__file__)), "seconds": args.seconds, "flags": args.flags,
                "requested_pages": args.pages, "repeats": args.repeats, "sampled": not args.no_sample,
                "queries": queries, "books": args.book or BOOKS,
                "libraries": {b: {"path": str(p), "sha256": digest(p)} for b, p in libraries.items()}}
    (out / "method.json").write_text(json.dumps(metadata, indent=2) + "\n")
    reports = []
    for book in args.book or BOOKS:
        for qi, query in enumerate(queries):
            for repeat in range(args.repeats):
                order = list(libraries) if repeat % 2 == 0 else list(reversed(libraries))
                semantics = {}
                for backend in order:
                    stem = f"{book}-q{qi}-{repeat}-{backend}"
                    query_path = out / f"{stem}.utf16"
                    query_path.write_bytes(query.encode("utf-16-le"))
                    ready, go = out / f"{stem}.ready", out / f"{stem}.go"
                    semantic = out / f"{stem}.bin"
                    command = [str(worker), str(libraries[backend]), str(ROOT / f"tests/render_corpus/books/{book}.pdf"),
                               "profile", str(query_path), str(semantic), str(args.pages), str(ready), str(go),
                               str(args.seconds + (1 if not args.no_sample else 0)), str(args.flags)]
                    with (out / f"{stem}.json").open("w") as stdout, (out / f"{stem}.stderr").open("w") as stderr:
                        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
                        sampler = None
                        try:
                            deadline = time.monotonic() + 120
                            while not ready.exists():
                                if process.poll() is not None or time.monotonic() > deadline:
                                    raise RuntimeError(f"worker failed preparing {stem}; see stderr")
                                time.sleep(.01)
                            if not args.no_sample:
                                with (out / f"{stem}.sampler.log").open("w") as log:
                                    sampler = subprocess.Popen(["sample", str(process.pid), str(math.ceil(args.seconds)), "1", "-file", str(out / f"{stem}.sample.txt")], stdout=log, stderr=log)
                                time.sleep(.35)
                            go.touch()
                            if process.wait(timeout=args.seconds + 30):
                                raise RuntimeError(f"matching worker failed: {stem}")
                            if sampler is not None and sampler.wait(timeout=20):
                                raise RuntimeError(f"sampler failed: {stem}")
                        finally:
                            if process.poll() is None:
                                process.kill()
                                process.wait()
                            if sampler is not None and sampler.poll() is None:
                                sampler.kill()
                                sampler.wait()
                    report = json.loads((out / f"{stem}.json").read_text())
                    report.update(book=book, query=query, backend=backend, repeat=repeat,
                                  semantic_sha256=digest(semantic))
                    semantics[backend] = semantic.read_bytes()
                    reports.append(report)
                    (out / "results.json").write_text(json.dumps(reports, indent=2) + "\n")
                    print(f"{stem}: start {report['start_ms']:.4f} scan {report['scan_ms']:.4f} close {report['search_close_ms']:.4f} ms/pass", flush=True)
                if semantics["native"] != semantics["rust"]:
                    raise RuntimeError(f"different match ranges: {book}, {query}")


if __name__ == "__main__":
    main()
