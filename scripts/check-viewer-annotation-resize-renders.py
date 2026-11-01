#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Verify saved resize smoke artifacts against native/Rust rendering (Pillow)."""
import argparse
import json
from pathlib import Path
import subprocess
from PIL import Image, ImageChops, ImageDraw

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--input', type=Path, required=True, help='smoke-viewer-annotation-resize.py output')
parser.add_argument('--binary', type=Path, default=root/'target/bench-mac/release/pdfoxium-diff')
args = parser.parse_args()
images = {}
for backend in ['native', 'rust']:
    images[backend] = {}
    for name in ['source', 'preview-save', 'resized-square', 'resized-all']:
        directory = root/'target/render-diff'/backend/'viewer-annotation-resize'/name/'dpi-144'
        directory.mkdir(parents=True, exist_ok=True)
        case = dict(id=f'viewer-annotation-resize/{name}', path=str((args.input/f'{name}.pdf').resolve()),
                    page_count=4, all_pages=True, dpi=144, flags=1)
        subprocess.run([str(args.binary), '_render', backend, json.dumps(case)], check=True,
                       stdout=subprocess.DEVNULL)
        images[backend][name] = [Image.open(directory/f'{page:04}.png').convert('RGB') for page in range(4)]


def count(a, b):
    return sum(p != q for p, q in zip(a.get_flattened_data(), b.get_flattened_data()))


result = {}
for backend, pages in images.items():
    assert all(count(a, b) == 0 for a, b in zip(pages['source'], pages['preview-save'])), 'preview must never enter the saved PDF'
    source, square, all_shapes = [pages[name][0] for name in ['source', 'resized-square', 'resized-all']]
    # Original/right/down-expanded square, with unchanged border width/fill.
    difference = ImageChops.difference(source, square)
    assert difference.getbbox() == (240, 60, 392, 164), difference.getbbox()
    assert count(source, square) > 5000
    assert all(count(pages['source'][i], pages['resized-square'][i]) == 0 for i in range(1, 4))
    # All edits on page one stay within old/new free-text, square and circle bounds.
    difference = ImageChops.difference(source, all_shapes)
    draw = ImageDraw.Draw(difference)
    for box in [(39, 59, 249, 173), (239, 59, 400, 197), (39, 239, 209, 393)]:
        draw.rectangle(box, fill=(0, 0, 0))
    assert difference.getbbox() is None, difference.getbbox()
    changes = [count(a, b) for a, b in zip(pages['source'], pages['resized-all'])]
    assert all(n > 1000 for n in changes), changes
    result[backend] = dict(preview_changed_pixels=0, square_changed_pixels=count(source, square), all_changed_pixels=changes)
# Square appearances are pixel-identical. The ellipse has two one-level AA
# differences at fractional curve coordinates; free text retains baseline font
# rasterization differences between engines and is validated by bounds/changes.
native, rust = [images[backend]['resized-all'][0] for backend in ['native', 'rust']]
assert count(native.crop((240, 60, 400, 196)), rust.crop((240, 60, 400, 196))) == 0
circle = [image.crop((38, 238, 210, 394)) for image in [native, rust]]
error = max(abs(x-y) for p, q in zip(circle[0].get_flattened_data(), circle[1].get_flattened_data()) for x, y in zip(p, q))
assert error <= 1, error
result['ellipse_max_channel_error'] = error
(args.input/'render-comparison.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2))
