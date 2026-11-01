#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Compare saved note-drag smoke artifacts using both C API render oracles (Pillow)."""
import argparse
import json
from pathlib import Path
import subprocess
from PIL import Image, ImageChops

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--input', type=Path, required=True, help='smoke-viewer-note-drag.py output')
parser.add_argument('--binary', type=Path, default=root/'target/bench-mac/release/pdfoxium-diff')
args = parser.parse_args()
images = {}
for backend in ['native', 'rust']:
    images[backend] = {}
    for name in ['source', 'before', 'after', 'clamped']:
        directory = root/'target/render-diff'/backend/'viewer-note-drag'/name/'dpi-144'
        directory.mkdir(parents=True, exist_ok=True)
        case = dict(id=f'viewer-note-drag/{name}', path=str((args.input/f'{name}.pdf').resolve()),
                    page_count=5, all_pages=True, dpi=144, flags=1)
        subprocess.run([str(args.binary), '_render', backend, json.dumps(case)], check=True,
                       stdout=subprocess.DEVNULL)
        images[backend][name] = [Image.open(directory/f'{page:04}.png').convert('RGB') for page in range(5)]


def count(a, b):
    return sum(p != q for p, q in zip(a.get_flattened_data(), b.get_flattened_data()))


result = {}
for backend, pages in images.items():
    source, before, after = [pages[name][0] for name in ['source', 'before', 'after']]
    old = ImageChops.difference(source, before).getbbox()
    new = ImageChops.difference(source, after).getbbox()
    assert old and new
    assert count(source.crop(old), after.crop(old)) == 0, 'old icon region must restore source'
    assert new == tuple(c+(60 if i % 2 == 0 else 40) for i, c in enumerate(old)), (old, new)
    checked = 0
    # The destination overlaps page text. Compare AP pixels where the underlying
    # page agrees; transparent/AA edge pixels otherwise blend with different text.
    for y in range(old[1], old[3]):
        for x in range(old[0], old[2]):
            if source.getpixel((x, y)) == source.getpixel((x+60, y+40)):
                assert before.getpixel((x, y)) == after.getpixel((x+60, y+40))
                checked += 1
    assert checked > 1000
    for page in range(1, 5):
        assert count(pages['source'][page], pages['before'][page]) == 0
        assert count(pages['source'][page], pages['after'][page]) == 0
    result[backend] = dict(changed_pixels=count(before, after), identical_background_icon_pixels=checked)
# Clamp reaches fractional PDF bounds at the crop edge. Inspect that icon alone:
# native/Rust may differ by one coverage level, with no geometry/color mismatch.
a, b = [images[backend]['clamped'][0].crop((350, 0, 400, 50)) for backend in ['native', 'rust']]
error = max(abs(x-y) for p, q in zip(a.get_flattened_data(), b.get_flattened_data()) for x, y in zip(p, q))
assert error <= 1, error
result['clamped_icon_max_channel_error'] = error
(args.input/'render-comparison.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2))
