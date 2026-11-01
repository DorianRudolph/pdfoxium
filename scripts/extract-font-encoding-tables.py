#!/usr/bin/env python3
"""Regenerate Rust PDF font encoding tables from the pinned PDFium source."""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "upstream/pdfium/core/fpdfapi/font/cpdf_fontencoding.cpp"
DEST = ROOT / "crates/pdfoxium-core/src/fpdfapi/font/cpdf_fontencoding_tables.rs"

UNICODE_TABLES = [
    ("kMSSymbolEncoding", "MS_SYMBOL"),
    ("kStandardEncoding", "STANDARD"),
    ("kMacRomanEncoding", "MAC_ROMAN"),
    ("kAdobeWinAnsiEncoding", "ADOBE_WIN_ANSI"),
    ("kMacExpertEncoding", "MAC_EXPERT"),
    ("kAdobeSymbolEncoding", "ADOBE_SYMBOL"),
    ("kZapfEncoding", "ZAPF"),
]
NAME_TABLES = [
    ("kStandardEncodingNames", "STANDARD_NAMES", 224),
    ("kAdobeWinAnsiEncodingNames", "ADOBE_WIN_ANSI_NAMES", 224),
    ("kMacRomanEncodingNames", "MAC_ROMAN_NAMES", 224),
    ("kMacExpertEncodingNames", "MAC_EXPERT_NAMES", 224),
    ("kPDFDocEncodingNames", "PDF_DOC_NAMES", 232),
    ("kAdobeSymbolEncodingNames", "ADOBE_SYMBOL_NAMES", 224),
    ("kZapfEncodingNames", "ZAPF_NAMES", 224),
]


def body(source: str, name: str) -> str:
    match = re.search(r"\b" + name + r"\b\s*(?:\[[^]]+\])?\s*=\s*\{(.*?)};", source, re.S)
    if match is None:
        raise ValueError(f"Missing table: {name}")
    return match[1]


def main() -> None:
    source = SOURCE.read_text()
    output = [
        "// Contains code adapted from PDFium for pdfoxium.",
        "// Adapted code is subject to the original license terms below.",
        "//",
        "// Copyright 2016 The PDFium Authors",
        "// Use of this source code is governed by a BSD-style license that can be",
        "// found in upstream/pdfium/LICENSE.",
        "// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com",
        "// Generated from the pinned core/fpdfapi/font/cpdf_fontencoding.cpp.",
        "",
    ]
    for cpp_name, rust_name in UNICODE_TABLES:
        values = re.findall(r"0x[0-9a-fA-F]+", body(source, cpp_name))
        if len(values) != 256:
            raise ValueError(f"Expected 256 values in {cpp_name}, found {len(values)}")
        output.append(f"pub(super) const {rust_name}: [u16; 256] = [")
        for start in range(0, 256, 8):
            output.append("    " + ", ".join(values[start : start + 8]) + ",")
        output.append("];")
        output.append("")
    for cpp_name, rust_name, size in NAME_TABLES:
        values = re.findall(r'"([^"]*)"|\b(nullptr)\b', body(source, cpp_name))
        if len(values) != size:
            raise ValueError(f"Expected {size} values in {cpp_name}, found {len(values)}")
        items = [f'Some("{name}")' if name else "None" for name, null in values]
        output.append(f"pub(super) const {rust_name}: [Option<&str>; {size}] = [")
        for start in range(0, size, 4):
            output.append("    " + ", ".join(items[start : start + 4]) + ",")
        output.append("];")
        output.append("")
    DEST.parent.mkdir(parents=True, exist_ok=True)
    DEST.write_text("\n".join(output))


if __name__ == "__main__":
    main()
