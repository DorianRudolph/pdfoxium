#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Verify drawing-tool smoke saves with native/Rust rendering (Pillow)."""
import argparse
import json
from pathlib import Path
import subprocess
from PIL import Image, ImageChops, ImageDraw
root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--input', type=Path, required=True)
parser.add_argument('--binary', type=Path, default=root/'target/bench-mac/release/pdfoxium-diff')
args = parser.parse_args()
images = {}
for backend in ['native','rust']:
    images[backend] = {}
    for name in ['source','preview-save','created','text-edited']:
        directory = root/'target/render-diff'/backend/'viewer-drawing-tools'/name/'dpi-144'
        directory.mkdir(parents=True, exist_ok=True)
        case = dict(id=f'viewer-drawing-tools/{name}', path=str((args.input/f'{name}.pdf').resolve()),
                    page_count=5, all_pages=True, dpi=144, flags=1)
        subprocess.run([str(args.binary),'_render',backend,json.dumps(case)],check=True,stdout=subprocess.DEVNULL)
        images[backend][name] = [Image.open(directory/f'{page:04}.png').convert('RGB') for page in range(5)]


def count(a,b):
    return sum(p != q for p,q in zip(a.get_flattened_data(),b.get_flattened_data()))


result = {}
for backend,pages in images.items():
    assert all(count(a,b)==0 for a,b in zip(pages['source'],pages['preview-save']))
    source,created,edited = [pages[name][0] for name in ['source','created','text-edited']]
    delta = ImageChops.difference(source,created)
    for box in [(40,40,180,100),(220,40,340,120),(40,240,160,340)]:
        assert delta.crop(box).getbbox() is not None
    draw = ImageDraw.Draw(delta)
    for box in [(39,39,181,101),(219,39,341,121),(39,239,161,341)]:
        draw.rectangle(box,fill=(0,0,0))
    assert delta.getbbox() is None, 'creation must only change the requested bounds'
    bbox = ImageChops.difference(created,edited).getbbox()
    assert bbox and bbox[0]>=40 and bbox[1]>=40 and bbox[2]<=180 and bbox[3]<=100, bbox
    assert count(created,edited)>1000
    for page in range(1,5):
        assert count(pages['source'][page],pages['created'][page]) == 0
        assert count(pages['source'][page],pages['text-edited'][page]) == 0
    result[backend] = dict(created_pixels=count(source,created), text_edit_pixels=count(created,edited), preview_pixels=0)
for name in ['created','text-edited']:
    native,rust = [images[backend][name][0] for backend in ['native','rust']]
    assert count(native.crop((220,40,340,120)),rust.crop((220,40,340,120))) == 0
    a,b = [image.crop((40,240,160,340)) for image in [native,rust]]
    error = max(abs(x-y) for p,q in zip(a.get_flattened_data(),b.get_flattened_data()) for x,y in zip(p,q))
    assert error<=1,error
result['ellipse_max_channel_error'] = error
(args.input/'render-comparison.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
