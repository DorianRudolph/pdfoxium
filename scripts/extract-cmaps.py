#!/usr/bin/env python3
"""Regenerate bundled CMap tables from the pinned PDFium source."""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "upstream/pdfium/core/fpdfapi/cmaps"
DEST = ROOT / "crates/pdfoxium-core/src/fpdfapi/cmaps/data"
GROUPS = ("GB1", "CNS1", "Japan1", "Korea1")

ARRAY_RE = re.compile(
    r"const\s+(uint16_t|DWordCIDMap)\s+(\w+)\s*\[(\d+)(?:\s*\*\s*([23]))?\]\s*=\s*\{(.*?)\};",
    re.S,
)
MAP_RE = re.compile(
    r'\{"([^"]+)",\s*(\w+),\s*(nullptr|\w+),\s*(\d+),\s*(\d+),\s*CMap::Type::k(Single|Range),\s*(-?\d+)\}',
    re.S,
)


def extract_group(group: str) -> str:
    source = SOURCE / group
    arrays = {}
    for path in sorted(source.glob("*.cpp")):
        contents = path.read_text()
        for kind, name, count, stride, body in ARRAY_RE.findall(contents):
            values = [int(value, 16) for value in re.findall(r"0x([0-9a-fA-F]+)", body)]
            width = 4 if kind == "DWordCIDMap" else int(stride)
            if len(values) != int(count) * width:
                raise ValueError(f"{path}: {name} has {len(values)} words, expected {int(count) * width}")
            arrays[name] = (width, values)

    metadata = (source / f"cmaps_{group.lower()}.inc").read_text()
    maps = MAP_RE.findall(metadata)
    if not maps:
        raise ValueError(f"Missing CMap entries for {group}")
    output = [
        "// Copyright 2014 The PDFium Authors",
        "// Use of this source code is governed by a BSD-style license that can be",
        "// found in upstream/pdfium/LICENSE.",
        "// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com",
        "// Generated from the pinned PDFium core/fpdfapi/cmaps/" + group + " sources.",
        "// Run scripts/extract-cmaps.py to regenerate.",
        "use super::fpdf_cmaps::*;",
        "",
    ]
    used = set()
    for name, word_name, dword_name, word_count, dword_count, kind, offset in maps:
        word_width = 2 if kind == "Single" else 3
        if word_name not in arrays or len(arrays[word_name][1]) != int(word_count) * word_width:
            raise ValueError(f"{group}: bad word table {word_name}")
        used.add(word_name)
        if dword_name != "nullptr":
            if dword_name not in arrays or len(arrays[dword_name][1]) != int(dword_count) * 4:
                raise ValueError(f"{group}: bad double-word table {dword_name}")
            used.add(dword_name)
        elif int(dword_count):
            raise ValueError(f"{group}: missing double-word table for {name}")

    if set(arrays) != used:
        raise ValueError(f"{group}: unreferenced arrays: {sorted(set(arrays) - used)}")

    for symbol in sorted(used):
        width, values = arrays[symbol]
        rust_name = symbol.upper()
        struct = {2: "SingleCmap", 3: "RangeCmap", 4: "DwordCidMap"}[width]
        fields = {2: ("code", "cid"), 3: ("low", "high", "cid"), 4: ("hi_word", "lo_word_low", "lo_word_high", "cid")}[width]
        output.append(f"const {rust_name}: [{struct}; {len(values) // width}] = [")
        for start in range(0, len(values), width):
            parts = ", ".join(f"{field}: 0x{value:04x}" for field, value in zip(fields, values[start : start + width]))
            output.append(f"    {struct} {{ {parts} }},")
        output.extend(("];", ""))

    output.append(f"pub static MAPS: [CMap; {len(maps)}] = [")
    for index, (name, word_name, dword_name, _, _, kind, offset) in enumerate(maps):
        if not 0 <= index + int(offset) < len(maps) and int(offset):
            raise ValueError(f"{group}: invalid use offset for {name}")
        dword = "&[]" if dword_name == "nullptr" else f"&{dword_name.upper()}"
        output.append(
            f'    CMap {{ name: "{name}", word_map: WordMap::{kind}(&{word_name.upper()}), '
            f"dword_map: {dword}, use_offset: {offset} }},"
        )
    output.extend(("];", ""))
    return "\n".join(output)


def main() -> None:
    DEST.mkdir(parents=True, exist_ok=True)
    for group in GROUPS:
        (DEST / f"{group.lower()}.rs").write_text(extract_group(group))
    unicode_output = [
        "// Copyright 2014 The PDFium Authors",
        "// Use of this source code is governed by a BSD-style license that can be",
        "// found in upstream/pdfium/LICENSE.",
        "// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com",
        "// Generated from the pinned Adobe-*-UCS2 inc files.",
        "",
    ]
    for group in GROUPS:
        files = list((SOURCE / group).glob("Adobe-*-UCS2_*.inc"))
        if len(files) != 1:
            raise ValueError(f"Expected one CID-to-Unicode table for {group}")
        contents = files[0].read_text()
        match = re.search(r"inline constexpr uint16_t\s+\w+\[\]\s*=\s*\{(.*?)\};", contents, re.S)
        if not match:
            raise ValueError(f"Missing CID-to-Unicode array in {files[0]}")
        values = re.findall(r"0x[0-9a-fA-F]+", match[1])
        unicode_output.append(f"pub static {group.upper()}: [u16; {len(values)}] = [")
        for start in range(0, len(values), 10):
            unicode_output.append("    " + ", ".join(values[start : start + 10]) + ",")
        unicode_output.extend(("];", ""))
    (DEST / "unicode.rs").write_text("\n".join(unicode_output))


if __name__ == "__main__":
    main()
