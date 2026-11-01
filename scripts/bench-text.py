#!/usr/bin/env python3
# Copyright (c) 2026 DorianRudolph
# SPDX-License-Identifier: MIT
"""Serial, alternating native/PDFoxium C API text/search benchmarks (macOS/Linux).

Build the Rust release C API first. The small C++ measurement worker uses pinned
public headers and dynamically loads exactly one implementation per process.
"""
import argparse
import csv
import hashlib
import json
import os
import platform
import shlex
import statistics
import struct
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOOKS = ["hefferon-toc", "obp.0526", "openstax-biology-2e"]
QUERIES = ["the", "function", "of the", "pdfoxium_absent_7f1c9e"]
STAGES = ["open_ms", "page_load_ms", "text_load_ms", "buffer_ms", "operation_ms", "close_ms"]


def sha256(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def records(path, operation):
    """Read exact UTF-16 or (character index, length) records; reject truncation."""
    data = path.read_bytes()
    offset, pages = 0, []
    while offset < len(data):
        if len(data) - offset < 12:
            raise ValueError("truncated semantic header")
        page, characters, count = struct.unpack_from("<III", data, offset)
        offset += 12
        end = offset + count * (2 if operation == "extract" else 8)
        if page != len(pages) or end > len(data):
            raise ValueError("invalid/truncated semantic page")
        pages.append((characters, data[offset:end]))
        offset = end
    return pages


def compare(native_path, rust_path, operation):
    native, rust = records(native_path, operation), records(rust_path, operation)
    if len(native) != len(rust) or not native:
        raise ValueError("different/empty page inventories")
    differences = []
    for page, ((nc, nt), (rc, rt)) in enumerate(zip(native, rust)):
        if (nc, nt) == (rc, rt):
            continue
        item = {"page": page, "native_characters": nc, "rust_characters": rc,
                "native_results": len(nt) // (2 if operation == "extract" else 8),
                "rust_results": len(rt) // (2 if operation == "extract" else 8)}
        if operation == "extract":
            # Diagnostic only. Equality above compares actual UTF-16 bytes.
            a, b = nt.decode("utf-16-le", errors="replace"), rt.decode("utf-16-le", errors="replace")
            at = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            item.update(first_difference=at, native_context=a[max(0, at-35):at+65],
                        rust_context=b[max(0, at-35):at+65])
        else:
            a, b = list(struct.iter_unpack("<II", nt)), list(struct.iter_unpack("<II", rt))
            at = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            item.update(first_difference=at, native_ranges=a[max(0, at-2):at+4],
                        rust_ranges=b[max(0, at-2):at+4])
        differences.append(item)
    return {"pages": len(native), "equal_pages": len(native)-len(differences),
            "equal": not differences, "differences": differences}


def totals(sample):
    out = {stage: sum(page.get(stage, 0) for page in sample["pages"]) for stage in STAGES}
    out["open_ms"] = sample["open_ms"]
    out["close_ms"] += sample["document_close_ms"]
    out["total_ms"] = sum(out.values())
    out["results"] = sum(page["results"] for page in sample["pages"])
    out["characters"] = sum(page["characters"] for page in sample["pages"])
    out["pages"] = len(sample["pages"])
    return out


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pdf", type=Path, action="append", help="repeat; default: all three big books")
    parser.add_argument("--query", action="append", help="repeat; default: common word, domain word, phrase, miss")
    parser.add_argument("--operation", choices=["extract", "search"], action="append", help="repeat; default: both")
    parser.add_argument("--iterations", type=int, default=6)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--max-pages", type=int, default=0, help="smoke checks only; zero scans the entire book")
    parser.add_argument("--timeout", type=float, default=300, help="seconds per worker")
    parser.add_argument("--native-library", type=Path)
    parser.add_argument("--rust-library", type=Path)
    args = parser.parse_args()
    if args.iterations < 1 or args.warmups < 0 or args.max_pages < 0 or args.timeout <= 0:
        parser.error("invalid iteration/warmup/page/timeout count")
    queries = args.query if args.query is not None else QUERIES
    operations = set(args.operation or ["extract", "search"])
    if any(not q or "\0" in q for q in queries) or len(queries) != len(set(queries)):
        parser.error("queries must be nonempty, unique and contain no NUL")
    pdfs = [p.resolve() for p in (args.pdf or [ROOT / f"tests/render_corpus/books/{b}.pdf" for b in BOOKS])]
    if len(set(p.stem for p in pdfs)) != len(pdfs):
        parser.error("PDF stems must be unique")
    system = platform.system()
    if system == "Darwin" and platform.machine() == "arm64":
        native, rust = "vendor/pdfium/chromium-8100/mac-arm64/libpdfium.dylib", "target/release/libpdfoxium_capi.dylib"
    elif system == "Linux" and platform.machine() == "x86_64":
        native, rust = "vendor/pdfium/chromium-8100/linux-x64/libpdfium.so", "target/release/libpdfoxium_capi.so"
    elif args.native_library and args.rust_library:
        native, rust = args.native_library, args.rust_library
    else:
        parser.error("supply both library paths on this platform")
    libraries = {"native": (args.native_library or ROOT / native).resolve(),
                 "rust": (args.rust_library or ROOT / rust).resolve()}
    for path in pdfs:
        if not path.is_file():
            parser.error(f"missing PDF: {path}; download the books first: cargo run -p pdfoxium-diff -- fetch books")
    for path in libraries.values():
        if not path.is_file():
            parser.error(f"missing input: {path}")
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error("output must be empty (keep runs separate)")
    output.mkdir(parents=True, exist_ok=True)
    source = ROOT / "tests/capi/text_benchmark.cpp"
    compiler = shlex.split(os.environ.get("CXX", "c++"))
    worker = output / "text-benchmark"
    build = [*compiler, "-std=c++17", "-O3", "-DNDEBUG", "-Wall", "-Wextra", "-Werror",
             "-I", str(ROOT / "upstream/pdfium/public"), str(source), "-o", str(worker)]
    if system == "Linux":
        build.append("-ldl")
    subprocess.run(build, check=True)
    method = {"timestamp_utc": datetime.now(timezone.utc).isoformat(), "platform": platform.platform(),
              "machine": platform.machine(), "cpu_count": os.cpu_count(),
              "git_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "git_status": subprocess.check_output(["git", "status", "--short"], cwd=ROOT, text=True),
              "upstream_commit": subprocess.check_output(["git", "-C", "upstream/pdfium", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "compiler": subprocess.check_output([*compiler, "--version"], text=True).splitlines()[0],
              "build": build, "worker_sha256": sha256(worker), "source_sha256": sha256(source),
              "driver_sha256": sha256(Path(__file__)), "iterations": args.iterations,
              "warmups": args.warmups, "max_pages": args.max_pages, "queries": queries,
              "operations": sorted(operations),
              "libraries": {b: {"path": str(p), "sha256": sha256(p)} for b, p in libraries.items()},
              "pdfs": [{"path": str(p), "sha256": sha256(p), "bytes": p.stat().st_size} for p in pdfs],
              "order": "serial; native/rust then rust/native on alternating rounds; fresh process/document per workload",
              "timing": "sum of measured API stages + extraction buffer resize; excludes file reads, library initialization, validation/dumps, process launch; search includes match range collection, no rectangles; UTF-16 extraction, no UTF-8 conversion"}
    write_json(output / "method.json", method)
    summary = []
    for pdf in pdfs:
        workloads = ([("extract", -1, "")] if "extract" in operations else [])
        if "search" in operations:
            workloads += [("search", i, q) for i, q in enumerate(queries)]
        for operation, query_index, query in workloads:
            scenario = f"{operation}-{query_index}" if operation == "search" else operation
            directory = output / "raw" / pdf.stem / scenario
            directory.mkdir(parents=True)
            query_file = directory / "query.utf16"
            query_file.write_bytes(query.encode("utf-16-le"))
            samples = {backend: [] for backend in libraries}
            page_samples = {backend: [] for backend in libraries}
            semantics = {}
            hashes = {}
            for iteration in range(-args.warmups, args.iterations):
                order = ["native", "rust"] if iteration % 2 == 0 else ["rust", "native"]
                for backend in order:
                    label = f"warmup{-iteration}" if iteration < 0 else f"round{iteration+1}"
                    semantic_path = directory / f"{backend}-{label}.bin"
                    command = [str(worker), str(libraries[backend]), str(pdf), operation,
                               str(query_file), str(semantic_path), str(args.max_pages)]
                    result = subprocess.run(command, capture_output=True, text=True, timeout=args.timeout)
                    if result.returncode:
                        raise RuntimeError(f"{backend} {pdf.stem} {scenario}: {result.stderr}")
                    sample = json.loads(result.stdout)
                    digest = sha256(semantic_path)
                    if backend in hashes and digest != hashes[backend]:
                        raise RuntimeError(f"unstable semantics: {backend} {pdf.stem} {scenario}")
                    if backend not in semantics:
                        semantics[backend], hashes[backend] = semantic_path, digest
                    else:
                        semantic_path.unlink()
                    sample.update(backend=backend, iteration=iteration, semantic_sha256=digest,
                                  totals=totals(sample), worker_stderr=result.stderr)
                    write_json(directory / f"{backend}-{label}.json", sample)
                    if iteration >= 0:
                        samples[backend].append(sample["totals"])
                        page_samples[backend].append(sample["pages"])
                print(f"{pdf.stem} {scenario} {iteration+1}/{args.iterations}", flush=True)
            equivalence = compare(semantics["native"], semantics["rust"], operation)
            item = {"book": pdf.stem, "operation": operation, "query": query, "semantics": equivalence}
            for backend in libraries:
                rows = samples[backend]
                item[backend] = {stage: statistics.median(r[stage] for r in rows) for stage in [*STAGES, "total_ms"]}
                item[backend].update(min_total_ms=min(r["total_ms"] for r in rows),
                                     max_total_ms=max(r["total_ms"] for r in rows),
                                     stdev_total_ms=statistics.stdev(r["total_ms"] for r in rows) if len(rows) > 1 else 0,
                                     results=rows[0]["results"], characters=rows[0]["characters"], pages=rows[0]["pages"])
                # Compare equal semantic work even if a book exposes an existing
                # extraction discrepancy. Exclude document open/close here.
                excluded = {difference["page"] for difference in equivalence["differences"]}
                equal_rows = [totals({"open_ms": 0, "document_close_ms": 0,
                                      "pages": [p for p in pages if p["page"] not in excluded]})
                              for pages in page_samples[backend]]
                item[backend]["equal_page_total_ms"] = statistics.median(r["total_ms"] for r in equal_rows)
            item["rust_over_native"] = item["rust"]["total_ms"] / item["native"]["total_ms"]
            summary.append(item)
            write_json(output / "summary.json", summary)
    with (output / "summary.csv").open("w", newline="") as stream:
        fields = ["book", "operation", "query", "equal_pages", "pages", "native_results", "rust_results", "rust_over_native",
                  "native_equal_page_total_ms", "rust_equal_page_total_ms"]
        fields += [f"{backend}_{stage}" for backend in libraries for stage in [*STAGES, "total_ms"]]
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        for item in summary:
            row = {key: item[key] for key in ["book", "operation", "query", "rust_over_native"]}
            row.update(equal_pages=item["semantics"]["equal_pages"], pages=item["semantics"]["pages"])
            for backend in libraries:
                row[f"{backend}_results"] = item[backend]["results"]
                row[f"{backend}_equal_page_total_ms"] = item[backend]["equal_page_total_ms"]
                row.update({f"{backend}_{stage}": item[backend][stage] for stage in [*STAGES, "total_ms"]})
            writer.writerow(row)
    print(f"Results: {output / 'summary.csv'}", flush=True)


if __name__ == "__main__":
    main()
