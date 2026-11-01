#!/usr/bin/env python3
"""Losslessly unpack pinned PDFium PNG goldens for the C API tests (developer-only)."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import struct

from PIL import Image

ROOT = Path(__file__).resolve().parents[4]
OUTPUT = Path(__file__).resolve().parent
PINNED_REVISION = "f51279699cb59754d1f49aef10eb8e825fe56532"
PIXEL_CASES = (
    "type3", "type3_xobject", "shade", "shade-tensor",
    "image_transformer_other", "icc_profile_bad_component", "icc_profile_bad_value",
    "jpxdecode_without_colorspace", "jpxdecode_with_mismatch_colorspace",
    "jpxdecode_smask_in_data_2",
)
EMBEDDER_PNGS = (
    "hello_world.png", "jpx_lzw.png", "jpx_lzw_grayscale.png",
    "embedded_images_33.png", "embedded_images_34.png", "embedded_images_35.png",
    "embedded_images_36.png", "embedded_images_37.png", "embedded_images_38.png",
    "embedded_images_33_rendered_agg.png", "embedded_images_33_rendered_transformed_agg.png",
    "image_clipped.png", "bug_1396264.png", "bug_1396264_reverse_byte.png",
    "bug_1558_agg.png", "bug_1574_agg.png", "bug_1658_annot_agg.png", "bug_1658_print.png",
    "blank_612x792.png", "bug_555784.png", "bug_664284_agg.png",
    "bug_664284_no_native_text.png", "bug_410996566.png", "pattern_fill.png",
    "pattern_fill_uncolored.png", "pattern_stroke.png", "pattern_stroke_uncolored.png",
)



def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify without writing")
    args = parser.parse_args()
    entries = []
    failures = []
    total = 0
    resources = Path("upstream/pdfium/testing/resources")
    pngs = [Path("pixel") / (stem + "_expected.pdf.0.png") for stem in PIXEL_CASES]
    pngs += [Path("embedder_tests") / name for name in EMBEDDER_PNGS]
    for relative in pngs:
        png = resources / relative
        with Image.open(ROOT / png) as original:
            # Palette/tRNS transparency is expanded losslessly; no resizing,
            # color-management, alpha compositing or image enhancement occurs.
            rgba = original.convert("RGBA")
            pixels = rgba.tobytes()
            data = struct.pack("<4sII", b"PDRG", rgba.width, rgba.height) + pixels
            minimum, maximum = rgba.getchannel("A").getextrema()
            entry = {
                "name": relative.as_posix(),
                "source_png": png.as_posix(),
                "source_png_sha256": sha256((ROOT / png).read_bytes()),
                "source_mode": original.mode,
                "width": rgba.width,
                "height": rgba.height,
                "alpha_min": minimum,
                "alpha_max": maximum,
                "raw_file": relative.with_suffix(".rgba").as_posix(),
                "rgba_sha256": sha256(pixels),
                "rgba_bytes": len(pixels),
                "raw_sha256": sha256(data),
                "raw_bytes": len(data),
            }
        if relative.parent.name == "pixel":
            stem = relative.name.removesuffix("_expected.pdf.0.png")
            template = resources / "pixel" / (stem + ".in")
            entry["source_template"] = template.as_posix()
            entry["source_template_sha256"] = sha256((ROOT / template).read_bytes())
        entries.append(entry)
        total += len(data)
        path = OUTPUT / entry["raw_file"]
        if args.check:
            if not path.is_file() or path.read_bytes() != data:
                failures.append(str(path.relative_to(ROOT)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    manifest = (json.dumps({
        "pinned_pdfium_revision": PINNED_REVISION,
        "encoding": "12-byte <4sII header: PDRG,width,height; top-down tightly packed straight RGBA8",
        "fixtures": entries,
    }, indent=2, sort_keys=True) + "\n").encode()
    path = OUTPUT / "manifest.json"
    if args.check:
        if not path.is_file() or path.read_bytes() != manifest:
            failures.append(str(path.relative_to(ROOT)))
    else:
        path.write_bytes(manifest)
    if failures:
        print("Pinned golden regeneration differs: " + ", ".join(failures), file=sys.stderr)
        return 1
    print(f"{'Checked' if args.check else 'Generated'} {len(entries)} pinned goldens, {total} header+RGBA bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
