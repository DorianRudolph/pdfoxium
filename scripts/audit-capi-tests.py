#!/usr/bin/env python3
"""Validate the pinned public C API selection used by the typed test harness.

This checks declaration drift and explicit exclusions, not runtime coverage.
"""
import csv
import hashlib
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
PUBLIC = ROOT / "upstream/pdfium/public"
EXCLUSIONS = ROOT / "tests/capi/exclusions.tsv"
DECLARATION = re.compile(r"FPDF_EXPORT\s+[^;{}]*?\bFPDF_CALLCONV\s+(\w+)\s*\(")
EXCLUDED = {
    "FPDF_SetPrintMode": "fpdfview.h",
    "FPDF_RenderPage": "fpdfview.h",
    "FPDF_RenderPageSkia": "fpdfview.h",
    "FPDF_FFLDrawSkia": "fpdf_formfill.h",
    "FPDF_GetRecommendedV8Flags": "fpdfview.h",
    "FPDF_GetArrayBufferAllocatorSharedInstance": "fpdfview.h",
    "FSDK_SetTimeFunction": "fpdf_ext.h",
    "FSDK_SetLocaltimeFunction": "fpdf_ext.h",
}
EXPECTED_DECLARATIONS_SHA256 = "f8c61865ed656027f6998b49f004c82c2e88658e4733b4bf33aa566b7637558e"


def public_functions():
    functions = {}
    for path in sorted(PUBLIC.glob("*.h")):
        text = re.sub(r"/\*.*?\*/|//[^\n]*", "", path.read_text(), flags=re.DOTALL)
        declarations = list(DECLARATION.finditer(text))
        if not declarations and path.name != "fpdf_fwlevent.h":
            raise ValueError(f"No recognized exports in {path.name}; review its declarations")
        for match in declarations:
            name = match[1]
            if name in functions:
                raise ValueError(f"Duplicate public declaration: {name}")
            functions[name] = path.name
    if not functions:
        raise ValueError("No public functions found; initialize the pinned submodule")
    return functions


def check():
    functions = public_functions()
    fingerprint = hashlib.sha256("".join(
        f"{header}\t{name}\n" for name, header in sorted(functions.items())
    ).encode()).hexdigest()
    if fingerprint != EXPECTED_DECLARATIONS_SHA256:
        raise ValueError("Pinned public declarations changed; review scope before updating their fingerprint")
    with EXCLUSIONS.open(newline="") as stream:
        reader = csv.DictReader(stream, delimiter="\t")
        if reader.fieldnames != ["function", "header", "reason"]:
            raise ValueError("Unexpected C API exclusion columns")
        rows = list(reader)
    if len({row["function"] for row in rows}) != len(rows):
        raise ValueError("Duplicate C API exclusion")
    recorded = {row["function"]: row["header"] for row in rows}
    if recorded != EXCLUDED:
        raise ValueError("C API exclusions differ from the reviewed portable scope")
    for row in rows:
        if functions.get(row["function"]) != row["header"] or not (row["reason"] or "").strip():
            raise ValueError(f"Invalid exclusion: {row['function']}")
    print(f"{len(functions)} public functions; {len(functions) - len(rows)} selected; {len(rows)} excluded")
    print("Declaration/scope consistency only; execution and coverage require runtime checks")


if __name__ == "__main__":
    try:
        check()
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
