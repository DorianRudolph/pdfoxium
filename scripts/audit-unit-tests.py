#!/usr/bin/env python3
"""Scan pinned PDFium unit tests and validate explicit scope exclusions."""

import argparse
import csv
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = ROOT / "upstream/pdfium"
MANIFEST = ROOT / "tests/upstream-unit-exclusions.tsv"
FIELDS = ["source", "case", "reason"]
CASE = re.compile(
    r"^[ \t]*(TEST(?:_F|_P)?|TYPED_TEST(?:_P)?)\s*\(\s*(\w+)\s*,\s*(\w+)\s*\)",
    re.MULTILINE,
)

# These native fixture implementations have no test declarations. Keep them
# visible when scanning the complete tree, without inventing test-case rows.
FIXTURE_ONLY = {
    "testing/fxgc_unittest.cpp": "cppgc test fixture and collection helpers",
    "testing/fxv8_unittest.cpp": "V8 test fixture and isolate/context helpers",
}


def unit_sources():
    return sorted(
        path for path in UPSTREAM.rglob("*_unittest.*")
        if path.suffix in {".cc", ".cpp"}
    )


def native_cases():
    rows = []
    for path in unit_sources():
        source = path.relative_to(UPSTREAM).as_posix()
        text = path.read_text()
        # Preserve source positions when discarding block comments.
        text = re.sub(r"/\*.*?\*/", lambda m: "\n" * m[0].count("\n"), text,
                      flags=re.DOTALL)
        text = re.sub(r"^[ \t]*//[^\n]*", "", text, flags=re.MULTILINE)
        start = len(rows)
        for match in CASE.finditer(text):
            rows.append(dict(source=source, case=f"{match[2]}.{match[3]}",
                             kind=match[1], line=str(text.count("\n", 0, match.start()) + 1)))
        assertions = list(re.finditer(r"^[ \t]*static_assert\s*\(", text, re.MULTILINE))
        if assertions:
            rows.append(dict(source=source, case="__static_assertions__",
                             kind="STATIC_ASSERT_GROUP",
                             line=str(text.count("\n", 0, assertions[0].start()) + 1)))
        if len(rows) == start and source not in FIXTURE_ONLY:
            raise ValueError(f"Unit source has no recognized declarations: {source}; "
                             "review its test macros or identify fixture-only support")
    keys = [(row["source"], row["case"]) for row in rows]
    if len(keys) != len(set(keys)):
        raise ValueError("Duplicate source/case identifiers; disambiguate conditional definitions")
    return sorted(rows, key=lambda row: (row["source"], int(row["line"])))


def read_manifest():
    with MANIFEST.open(newline="") as stream:
        reader = csv.DictReader(stream, delimiter="\t")
        if reader.fieldnames != FIELDS:
            raise ValueError("Unexpected upstream exclusion columns")
        return list(reader)


def validate(rows, native):
    expected = {(row["source"], row["case"]) for row in native}
    seen = set()
    for row in rows:
        key = (row["source"], row["case"])
        if key in seen:
            raise ValueError(f"Duplicate upstream exclusion: {key}")
        if key not in expected:
            raise ValueError(f"Unknown upstream exclusion: {key}")
        if not (row["reason"] or "").strip():
            raise ValueError(f"Exclusion needs a specific reason: {key}")
        seen.add(key)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate exclusions (the default)")
    parser.add_argument("--list-sources", action="store_true", help="list native unit sources, including fixture-only support")
    args = parser.parse_args()
    native = native_cases()
    rows = read_manifest()
    validate(rows, native)
    sources = unit_sources()
    case_sources = {row["source"] for row in native}
    if args.list_sources:
        for path in sources:
            source = path.relative_to(UPSTREAM).as_posix()
            role = "test cases" if source in case_sources else FIXTURE_ONLY[source]
            print(f"{source}\t{role}")
    print(f"{len(sources)} native unit sources; {len(native)} test/static-assert records; {len(rows)} explicit exclusions")
    print("Exclusion consistency only; this does not infer test coverage for other native cases")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
