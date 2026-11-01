#!/usr/bin/env python3
"""Generate a Unicode-addressable test font from PDFium's exact pinned subset.

Developer-only dependency: fonttools==4.61.1. No engine/test-runner dependency.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import io
from pathlib import Path
import re

from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
import zlib

ROOT = Path(__file__).resolve().parents[3]
RESOURCES = ROOT / "upstream/pdfium/testing/resources"
OUTPUT = Path(__file__).with_name("hebrew_unicode.ttf")


def build() -> tuple[bytes, bytes]:
    fragment = (RESOURCES / "hebrew_truetype_font.fragment").read_bytes()
    original = zlib.decompress(base64.a85decode(fragment.strip().removesuffix(b"~>")))
    font = TTFont(io.BytesIO(original), recalcTimestamp=False)
    source = TTFont(io.BytesIO(original), recalcTimestamp=False)
    mac = next(t for t in font["cmap"].tables if (t.platformID, t.platEncID) == (1, 0))
    # The PDF's existing ToUnicode mapping is authoritative. Do not invent
    # Unicode labels, renumber glyphs, or replace any outlines/metrics.
    section = (RESOURCES / "hebrew_cmap.fragment").read_text().split("beginbfchar")[1].split("endbfchar")[0]
    pairs = re.findall(r"<([0-9a-fA-F]+)>\s*<([0-9a-fA-F]+)>", section)
    unicode_map = {int(uni, 16): mac.cmap[int(code, 16)] for code, uni in pairs}
    assert len(unicode_map) == 5
    assert font.getBestCmap() is None
    cmap = CmapSubtable.newSubtable(4)
    cmap.platformID, cmap.platEncID, cmap.language = 3, 1, 0
    cmap.cmap = unicode_map
    font["cmap"].tables.append(cmap)
    # Give this modified test fixture its own family, including in PDF/DR.
    # Keep the original copyright, authorship, version and OFL metadata.
    names = {1: "Pdfoxium Hebrew Test", 3: "PdfoxiumHebrewTest-Regular (PDFium subset Unicode cmap)",
             4: "Pdfoxium Hebrew Test Regular", 6: "PdfoxiumHebrewTest-Regular"}
    for record in font["name"].names:
        if record.nameID in names:
            record.string = names[record.nameID].encode(record.getEncoding())
    out = io.BytesIO()
    font.save(out)
    result = out.getvalue()
    checked = TTFont(io.BytesIO(result), recalcTimestamp=False)
    before, after = source.getGlyphOrder(), checked.getGlyphOrder()
    assert len(before) == len(after) == 6
    assert before[0] == after[0] == ".notdef"
    for old, new in zip(before, after):
        assert source["hmtx"][old] == checked["hmtx"][new]
        assert source["glyf"][old].compile(source["glyf"]) == checked["glyf"][new].compile(checked["glyf"])
    expected_gids = {int(uni, 16): source.getGlyphID(mac.cmap[int(code, 16)]) for code, uni in pairs}
    assert {uni: checked.getGlyphID(glyph) for uni, glyph in checked.getBestCmap().items()} == expected_gids
    for name_id in (0, 13):
        assert [r.string for r in source["name"].names if r.nameID == name_id] == [r.string for r in checked["name"].names if r.nameID == name_id]
    return original, result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the checked-in binary without writing")
    args = parser.parse_args()
    original, generated = build()
    if args.check:
        assert OUTPUT.read_bytes() == generated, "checked-in fixture differs; regenerate with FontTools 4.61.1"
    else:
        OUTPUT.write_bytes(generated)
    print("Decoded pinned source SHA256:", hashlib.sha256(original).hexdigest())
    print("Derived fixture SHA256:", hashlib.sha256(generated).hexdigest())
    print("Derived bytes:", len(generated))
    print("Verified all 6 glyph IDs, compiled outlines/instructions and advances (including .notdef).")


if __name__ == "__main__":
    main()
