#!/usr/bin/env python3
"""Collect and analyze production shared-library C API coverage (LLVM tools only)."""
import argparse
import contextlib
import csv
import datetime
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
SCOPES = {"core": "crates/pdfoxium-core/src/", "capi": "crates/pdfoxium-capi/src/"}


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def command(argv, output=None):
    if output:
        with output.open("w", encoding="utf-8") as stream:
            result = subprocess.run([str(v) for v in argv], cwd=ROOT, stdout=stream,
                                    stderr=subprocess.PIPE, text=True)
    else:
        result = subprocess.run([str(v) for v in argv], cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        raise ValueError(f"Command failed ({result.returncode}): {' '.join(map(str, argv))}\n{result.stderr}")
    if result.stderr:
        print(result.stderr.rstrip(), file=sys.stderr)
    return result.stdout or ""


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def tool(args, name):
    if args.llvm_bin:
        path = args.llvm_bin / name
        if path.is_file():
            return str(path)
    found = shutil.which(name)
    if not found:
        raise ValueError(f"Missing {name}; pass --llvm-bin from the instrumenting Rust toolchain")
    return found


def relative(path):
    try:
        return str(Path(path).resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def sources():
    return {str(path.relative_to(ROOT)): digest(path)
            for prefix in SCOPES.values() for path in sorted((ROOT / prefix).rglob("*.rs"))}


def has_coverage_sections(sections):
    # ELF/Mach-O and COFF use different section spellings.
    return (any(name in sections for name in ("__llvm_covmap", ".lcovmap"))
            and any(name in sections for name in ("__llvm_covfun", ".lcovfun")))


def prepare(args):
    run = args.run_dir.resolve()
    manifest = run / "manifest.json"
    if manifest.exists():
        return verify(args)
    profiles = run / "profiles"
    existing = list(profiles.rglob("*.profraw")) if profiles.exists() else []
    if existing and not args.adopt_existing:
        raise ValueError("Existing profiles have no provenance. Use a fresh run directory, or explicitly --adopt-existing only if they came from this exact binary.")
    if not args.binary:
        raise ValueError("--binary is required to prepare a run")
    binary = args.binary.resolve()
    sections = command([tool(args, "llvm-readobj"), "--sections", binary])
    if not has_coverage_sections(sections):
        raise ValueError("Binary has no LLVM coverage map/function sections. Rebuild the first-party crates with -Cinstrument-coverage; cached non-instrumented artifacts are not usable.")
    run.mkdir(parents=True, exist_ok=True)
    profiles.mkdir(exist_ok=True)
    snapshot = run / binary.name
    if snapshot.exists():
        raise ValueError(f"Unmanaged binary already exists: {snapshot}")
    before = digest(binary)
    shutil.copy2(binary, snapshot)
    if digest(snapshot) != before or digest(binary) != before:
        snapshot.unlink()
        raise ValueError("Binary changed while preparing coverage run")
    git = {"head": command(["git", "rev-parse", "HEAD"]).strip(),
           "status": command(["git", "status", "--porcelain"]),
           "production_diff_sha256": hashlib.sha256(command(["git", "diff", "HEAD", "--", "crates", "Cargo.toml", "Cargo.lock"]).encode()).hexdigest()}
    data = {"schema": 1, "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "binary": snapshot.name, "binary_sha256": before, "original_binary": str(binary),
            "git": git, "source_sha256": sources(), "profile_origin": "explicitly adopted external profiles" if existing else "prepared before collection",
            "llvm_cov_version": command([tool(args, "llvm-cov"), "--version"]).strip(),
            "build_environment": {key: os.environ.get(key) for key in ("RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_LLVM_COV", "CARGO_LLVM_COV_TARGET_DIR", "LLVM_PROFILE_FILE")},
            "runs": []}
    write_json(manifest, data)
    print(f"Prepared {run}: {before}")
    return data


def verify(args):
    run = args.run_dir.resolve()
    manifest = run / "manifest.json"
    if not manifest.exists():
        return prepare(args)
    data = json.loads(manifest.read_text())
    snapshot = run / data["binary"]
    if digest(snapshot) != data["binary_sha256"]:
        raise ValueError("Saved instrumented binary changed; do not mix incompatible builds")
    if args.binary and digest(args.binary.resolve()) != data["binary_sha256"]:
        raise ValueError("Requested binary differs from this run's recorded build; use a new run directory")
    if sources() != data["source_sha256"]:
        raise ValueError("Core/C API sources differ from the recorded build; analyze in its matching checkout or prepare a new run")
    return data


def comparison_dpi(value):
    try:
        dpi = int(value)
    except (TypeError, ValueError):
        raise argparse.ArgumentTypeError("comparison DPI must be an integer in 18..1152") from None
    if not 18 <= dpi <= 1152:
        raise argparse.ArgumentTypeError("comparison DPI must be in 18..1152")
    return dpi


def collect(args):
    dpi = comparison_dpi(args.comparison_dpi)
    data = verify(args)
    run = args.run_dir.resolve()
    if data["runs"] or list((run / "profiles").rglob("*.profraw")):
        raise ValueError("Collection directory already contains runs/profiles; use a distinct run directory")
    runner, native = args.runner.resolve(), args.native_library.resolve()
    common = [str(runner), "--native-library", str(native), "--rust-library", str(run / data["binary"]),
              "--comparison-dpi", str(dpi)]
    if not args.raster_diff:
        common.append("--skip-raster-diff")
    # Diagnostic scale sweeps are independent of the ordinary comparison render.
    # Do not accidentally multiply a coverage campaign's work through inherited settings.
    settings = {"comparison_dpi": dpi, "raster_diff": args.raster_diff,
                "form_scale_environment": {name: None for name in (
                    "PDFOXIUM_CAPI_FORM_SCALE_DIR", "PDFOXIUM_CAPI_FORM_SCALES")}}
    data["collection"] = settings
    write_json(run / "manifest.json", data)
    scenarios = [("suite", ["--gtest_filter=" + args.gtest_filter]),
                 ("system-fonts", ["--host-fonts", "--gtest_filter=SystemFonts.*"])]
    scenarios += [("lifecycle-" + config, ["--lifecycle-config", config])
                  for config in ("default", "1", "2", "3", "4", "5", "6", "7")]
    scenarios += [("lifecycle-brotli", ["--lifecycle-config", "6", "--brotli"]),
                  ("lifecycle-isolation", ["--lifecycle-config", "7", "--isolate"])]
    failures = []
    for label, extra in scenarios:
        environment = os.environ.copy()
        if label == "system-fonts":
            environment.pop("PDFOXIUM_CAPI_FONT_PATH", None)
        for name in settings["form_scale_environment"]:
            environment.pop(name, None)
        environment["LLVM_PROFILE_FILE"] = str(run / "profiles" / (label + "-%p-%m.profraw"))
        argv = common + extra
        print(f"Running {label}", flush=True)
        with (run / (label + ".log")).open("w") as log:
            result = subprocess.run(argv, cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT)
        data["runs"].append({"label": label, "argv": argv, "returncode": result.returncode,
                             "llvm_profile_file": environment["LLVM_PROFILE_FILE"],
                             "comparison_settings": settings,
                             "host_fonts": label == "system-fonts"})
        write_json(run / "manifest.json", data)
        if result.returncode:
            failures.append(label)
    verify(args)
    if failures:
        raise ValueError("Test failures retained with profiles: " + ", ".join(failures))


def scope_for(filename):
    filename = relative(filename).replace("\\", "/")
    for scope, prefix in SCOPES.items():
        if filename.startswith(prefix):
            remainder = filename[len(prefix):]
            parts = remainder.split("/")
            subsystem = parts[0] if len(parts) > 1 else "(crate root)"
            return filename, scope, subsystem
    return filename, None, None


def ranges(lines):
    result = []
    for line in sorted(lines):
        if result and line == result[-1][1] + 1:
            result[-1][1] = line
        else:
            result.append([line, line])
    return result


def classify_api_functions(functions, selected):
    observed = {}
    for function in functions:
        name = function["name"]
        # Extern exports have unmangled LLVM names. Do not infer execution
        # from another function whose mangled name merely mentions an API.
        if name in selected:
            observed[name] = observed.get(name, 0) + function["count"]
    return {"selected_count": len(selected), "executed_count": sum(value > 0 for value in observed.values()),
            "non_executed": sorted(name for name, count in observed.items() if count == 0),
            "missing_mapping": sorted(selected - observed.keys()), "counts": dict(sorted(observed.items()))}


def api_counts(path):
    spec = importlib.util.spec_from_file_location("capi_inventory", ROOT / "scripts/audit-capi-tests.py")
    inventory = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(inventory)
    with contextlib.redirect_stdout(io.StringIO()):
        inventory.check()
    selected = set(inventory.public_functions()) - set(inventory.EXCLUDED)
    return classify_api_functions(
        (function for unit in json.loads(path.read_text())["data"]
         for function in unit.get("functions", [])), selected)


def analyze(args):
    data = verify(args)
    run = args.run_dir.resolve()
    binary = run / data["binary"]
    profiles = sorted((run / "profiles").rglob("*.profraw"))
    if not profiles:
        raise ValueError("No .profraw files in run's profiles directory")
    # LLVM response files avoid command-length limits for large scenario sets.
    response = run / "profiles.rsp"
    response.write_text("\n".join(json.dumps(str(path)) for path in profiles) + "\n")
    command([tool(args, "llvm-profdata"), "merge", "--failure-mode=any", "-sparse", "@" + str(response), "-o", run / "coverage.profdata"])
    common = [tool(args, "llvm-cov"), "export", binary, "--instr-profile=" + str(run / "coverage.profdata"), "--check-binary-ids"]
    command(common + ["--summary-only"], run / "coverage.json")
    command(common + ["--format=lcov", "--skip-functions", "--skip-branches"], run / "coverage.lcov")
    command(common + ["--name-regex=(FPDF|FORM_|FSDK_)", "--skip-expansions"], run / "api-functions.json")
    missed = {}
    current = None
    with (run / "coverage.lcov").open() as stream:
        for line in stream:
            if line.startswith("SF:"):
                current = scope_for(line[3:].strip())[0]
            elif current and line.startswith("DA:"):
                number, count = line[3:].strip().split(",")[:2]
                if int(count) == 0:
                    missed.setdefault(current, set()).add(int(number))
    files = []
    totals = {scope: {"count": 0, "covered": 0} for scope in SCOPES}
    groups = {}
    for unit in json.loads((run / "coverage.json").read_text())["data"]:
        for entry in unit["files"]:
            filename, scope, subsystem = scope_for(entry["filename"])
            if not scope:
                continue
            counts = entry["summary"]["lines"]
            row = {"scope": scope, "subsystem": subsystem, "file": filename,
                   "count": counts["count"], "covered": counts["covered"],
                   "missed": counts["count"] - counts["covered"], "percent": counts["percent"],
                   "missed_ranges": ranges(missed.get(filename, set())),
                   "functions": entry["summary"]["functions"], "regions": entry["summary"]["regions"],
                   "branches": entry["summary"]["branches"]}
            files.append(row)
            for target in (totals[scope], groups.setdefault(scope + "/" + subsystem, {"count": 0, "covered": 0})):
                target["count"] += row["count"]
                target["covered"] += row["covered"]
                for metric in ("functions", "regions", "branches"):
                    counter = target.setdefault(metric, {"count": 0, "covered": 0})
                    counter["count"] += row[metric]["count"]
                    counter["covered"] += row[metric]["covered"]
    if any(not total["count"] for total in totals.values()):
        raise ValueError("Core or C API coverage mappings are missing; rebuild both first-party crates with instrumentation")
    for total in list(totals.values()) + list(groups.values()):
        total["missed"] = total["count"] - total["covered"]
        total["percent"] = 100 * total["covered"] / total["count"] if total["count"] else 0
        for metric in ("functions", "regions", "branches"):
            counter = total[metric]
            counter["missed"] = counter["count"] - counter["covered"]
            counter["percent"] = 100 * counter["covered"] / counter["count"] if counter["count"] else 0
    files.sort(key=lambda item: (-item["missed"], item["file"]))
    summary = {"binary_sha256": data["binary_sha256"], "profile_count": len(profiles),
               "scope": "Production shared object only; no source exclusions inside core or capi. Dependency crates reported by LLVM are outside these two crate totals.",
               "totals": totals, "subsystems": groups, "files": files,
               "selected_api_execution": api_counts(run / "api-functions.json"),
               "branch_reporting": "Branch records present" if any(total["branches"]["count"] for total in totals.values()) else "Not collected by this stable-Rust instrumentation; zero branch records do not mean complete branch coverage"}
    write_json(run / "summary.json", summary)
    with (run / "files.tsv").open("w", newline="") as stream:
        writer = csv.writer(stream, delimiter="\t")
        writer.writerow(["scope", "subsystem", "file", "count", "covered", "missed", "percent"])
        for item in files:
            writer.writerow([item[key] for key in ("scope", "subsystem", "file", "count", "covered", "missed", "percent")])
    with (run / "missed-ranges.tsv").open("w", newline="") as stream:
        writer = csv.writer(stream, delimiter="\t")
        writer.writerow(["scope", "subsystem", "file", "start", "end", "missed_lines", "source_excerpt"])
        for item in files:
            source = (ROOT / item["file"]).read_text(errors="replace").splitlines()
            for start, end in item["missed_ranges"]:
                excerpt = " ".join(line.strip() for line in source[start - 1:min(end, start + 3)] if line.strip())[:180]
                writer.writerow([item["scope"], item["subsystem"], item["file"], start, end, end - start + 1, excerpt])
    report = ["C API integration coverage of the exact production shared object", "Binary SHA256: " + data["binary_sha256"], ""]
    for scope, total in totals.items():
        report.append(f"Raw {scope}: {total['covered']}/{total['count']} lines ({total['percent']:.2f}%), {total['missed']} missed")
        for metric in ("functions", "regions"):
            counter = total[metric]
            report.append(f"  {metric}: {counter['covered']}/{counter['count']} ({counter['percent']:.2f}%)")
    report.append(summary["branch_reporting"])
    api = summary["selected_api_execution"]
    report += [f"Selected APIs executed: {api['executed_count']}/{api['selected_count']}; zero: {len(api['non_executed'])}; missing mappings: {len(api['missing_mapping'])}", "", "Subsystems:"]
    for name, total in sorted(groups.items(), key=lambda pair: -pair[1]["missed"]):
        report.append(f"{name}: {total['covered']}/{total['count']} ({total['percent']:.2f}%), {total['missed']} missed")
    report += ["", f"Top {args.top} files by missed lines:"]
    report += [f"{item['file']}: {item['missed']} missed ({item['percent']:.2f}%)" for item in files[:args.top]]
    text = "\n".join(report) + "\n"
    (run / "report.txt").write_text(text)
    print(text, end="")
    data["analysis"] = {"profiles": {str(path.relative_to(run)): digest(path) for path in profiles},
                        "profile_data_sha256": digest(run / "coverage.profdata")}
    write_json(run / "manifest.json", data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("prepare", "collect", "analyze"))
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--llvm-bin", type=Path)
    parser.add_argument("--adopt-existing", action="store_true")
    parser.add_argument("--runner", type=Path, default=ROOT / "target/capi/pdfoxium_capi_tests")
    parser.add_argument("--native-library", type=Path, default=ROOT / "vendor/pdfium/chromium-8100/linux-x64/libpdfium.so")
    parser.add_argument("--gtest-filter", default="*")
    parser.add_argument("--comparison-dpi", type=comparison_dpi, default=72,
                        help="comparison render DPI, 18..1152 (default: 72)")
    parser.add_argument("--raster-diff", action="store_true",
                        help="retain shared raster comparisons (skipped by default for coverage)")
    parser.add_argument("--top", type=int, default=30)
    args = parser.parse_args()
    try:
        {"prepare": prepare, "collect": collect, "analyze": analyze}[args.action](args)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"Coverage error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
