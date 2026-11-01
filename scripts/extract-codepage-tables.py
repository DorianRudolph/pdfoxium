#!/usr/bin/env python3
"""Regenerate the eight high-byte charset tables from pinned PDFium."""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "upstream/pdfium/core/fxcrt/fx_codepage.cpp"
DEST = ROOT / "crates/pdfoxium-core/src/fxcrt/fx_codepage/tables.rs"

TABLES = (
    ("kFX_MSDOSThaiUnicodes", "Thai"),
    ("kFX_MSWinEasternEuropeanUnicodes", "MswinEasternEuropean"),
    ("kFX_MSWinCyrillicUnicodes", "MswinCyrillic"),
    ("kFX_MSWinGreekUnicodes", "MswinGreek"),
    ("kFX_MSWinTurkishUnicodes", "MswinTurkish"),
    ("kFX_MSWinHebrewUnicodes", "MswinHebrew"),
    ("kFX_MSWinArabicUnicodes", "MswinArabic"),
    ("kFX_MSWinBalticUnicodes", "MswinBaltic"),
)


def main() -> None:
    source = SOURCE.read_text()
    output = [
        "// Copyright 2018 The PDFium Authors",
        "// Use of this source code is governed by a BSD-style license that can be",
        "// found in upstream/pdfium/LICENSE.",
        "// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com",
        "// Generated from the pinned core/fxcrt/fx_codepage.cpp.",
        "",
        "use super::FxCharset;",
        "",
    ]
    for name, _ in TABLES:
        match = re.search(r"\b" + name + r"\[128\]\s*=\s*\{(.*?)\};", source, re.S)
        if not match:
            raise ValueError(f"Missing table {name}")
        values = re.findall(r"0x[0-9a-fA-F]+", match[1])
        if len(values) != 128:
            raise ValueError(f"Expected 128 values in {name}; found {len(values)}")
        output.append(f"const {name.upper()}: [u16; 128] = [")
        for start in range(0, 128, 8):
            output.append("    " + ", ".join(values[start : start + 8]) + ",")
        output.extend(("];", ""))
    output.append("pub static CHARSET_UNICODES: [(FxCharset, &[u16]); 8] = [")
    for name, charset in TABLES:
        output.append(f"    (FxCharset::{charset}, &{name.upper()}),")
    output.extend(("];", ""))
    DEST.parent.mkdir(parents=True, exist_ok=True)
    DEST.write_text("\n".join(output))


if __name__ == "__main__":
    main()
