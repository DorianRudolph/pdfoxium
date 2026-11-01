#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Assert visible saved annotation styles in native/Rust renders (Pillow)."""
import argparse
import json
from pathlib import Path
import subprocess
from PIL import Image

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--input', type=Path, required=True, help='annotation-style smoke output')
parser.add_argument('--binary', type=Path, default=root/'target/bench-mac/release/pdfoxium-diff')
args = parser.parse_args()
result = {}
for backend in ['native', 'rust']:
    images = {}
    for name in ['before', 'styled', 'fill-removed', 'defaults']:
        path = root/'crates/pdfoxium-viewer/assets/demos/editing-demo.pdf' if name == 'before' else args.input/f'{name}.pdf'
        case_id = f'viewer-annotation-style/{name}'
        directory = root/'target/render-diff'/backend/case_id/'dpi-144'
        directory.mkdir(parents=True, exist_ok=True)
        case = dict(id=case_id, path=str(path.resolve()), page_count=3,
                    all_pages=True, dpi=144, flags=1)
        subprocess.run([str(args.binary), '_render', backend, json.dumps(case)],
                       check=True, stdout=subprocess.DEVNULL)
        images[name] = [Image.open(directory/f'{page:04}.png').convert('RGB') for page in range(3)]
    before, styled, cleared = [images[name][0] for name in ['before', 'styled', 'fill-removed']]

    def pixel(image, x, y):
        # Sample in native PDF coordinates on the unrotated 612x792 fixture.
        return image.getpixel((int(x*2), int((792-y)*2)))

    assert pixel(styled, 440, 360) == (255, 230, 120), 'rectangle fill'
    assert pixel(styled, 347, 360) == (10, 150, 60), 'rectangle border'
    assert pixel(styled, 440, 260) == (250, 170, 210), 'ellipse fill'
    assert pixel(cleared, 440, 260) == pixel(before, 440, 260), 'removed fill restores background'
    highlight = pixel(styled, 80, 628)
    # Byte opacity 64/255, one normal blend over white; integer rounding differs
    # by at most one from the floating-point reference.
    expected = tuple(round(c*64/255 + 255*(1-64/255)) for c in (30, 170, 240))
    assert max(abs(a-b) for a, b in zip(highlight, expected)) <= 1, highlight
    changes = {}
    for kind, box in dict(text=(130, 804, 630, 1064), rectangle=(690, 794, 1070, 924),
                          ellipse=(760, 984, 1000, 1144), highlight=(80, 305, 594, 330)).items():
        changes[kind] = sum(a != b for a, b in zip(before.crop(box).get_flattened_data(),
                                                 styled.crop(box).get_flattened_data()))
        assert changes[kind] > 50, (backend, kind, changes[kind])
    # Newly drawn defaults on page two must also survive saving as visible ink.
    changes['new_annotations'] = sum(a != b for a, b in zip(
        images['before'][1].get_flattened_data(), images['defaults'][1].get_flattened_data()))
    assert changes['new_annotations'] > 1000
    result[backend] = dict(changes=changes, highlight_pixel=highlight)
(args.input/'render-comparison.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2))
