#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Compare native viewer pixels with the exact renderer bitmap after fractional placement (Pillow)."""
import argparse
import json
import math
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image, ImageChops


def pixel_pdf(path):
    content = '0 0 0 rg BT /F1 24 Tf 60 900 Td (Screen-relative rendering) Tj ET\n'
    for y in range(100, 900, 80):
        content += f'BT /F1 18 Tf 80 {y} Td (Same pixels in whole-page and tiled rendering.) Tj ET\n'
    content += '0.2 0.6 0.9 RG 2 w 20 20 m 980 980 l S'
    objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1000.3 1000.7] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>', f'<< /Length {len(content)} >>\nstream\n{content}\nendstream', '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
    pdf = '%PDF-1.7\n'
    offsets = []
    for i, obj in enumerate(objects):
        offsets.append(len(pdf))
        pdf += f'{i + 1} 0 obj\n{obj}\nendobj\n'
    xref = len(pdf)
    pdf += 'xref\n0 6\n0000000000 65535 f \n'
    pdf += ''.join(f'{offset:010} 00000 n \n' for offset in offsets)
    pdf += f'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
    path.write_text(pdf)


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/release/pdfoxium-viewer')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--render', type=Path, default=root / 'target/release/examples/render')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    endpoint_path = args.output / 'control.json'
    assert not endpoint_path.exists()
    report = []
    pdf = (args.output / 'pixel-placement.pdf').resolve()
    pixel_pdf(pdf)
    with (args.output / 'app.log').open('w') as log:
        app = subprocess.Popen([str(args.binary), str(pdf), '--control', str(endpoint_path.resolve()), '--control-only-input'], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while not endpoint_path.exists():
                assert app.poll() is None, 'viewer exited; see app.log'
                assert time.monotonic() < deadline, 'viewer did not start'
                time.sleep(0.05)
            endpoint = json.loads(endpoint_path.read_text())

            def call(method, params=None):
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as conn:
                    conn.sendall(json.dumps({'jsonrpc': '2.0', 'id': len(report), 'token': endpoint['token'], 'method': method, 'params': params or {}}).encode() + b'\n')
                    response = json.loads(conn.makefile('rb').readline())
                report.append({'method': method, 'params': params or {}, 'response': response})
                assert 'error' not in response, response
                return response['result']

            def wait():
                state = call('ui.wait')
                assert state['settled'] and state['active']['render_complete']
                return state

            initial = wait()
            ppp = initial['active']['render_policy']['pixels_per_point']
            for zoom in [0.375, 1.125, 1.25]:
                call('ui.command', {'id': 'view.zoom', 'zoom': zoom})
                call('ui.scroll', {'dx': 17.375, 'dy': 29.25})
                state = wait()
                rect = state['active']['pages_on_screen'][0]['rect']
                physical = [value * ppp for value in rect]
                assert all(abs(value - round(value)) < 0.002 for value in physical), physical
                expected_dir = args.output / f'render-{zoom}'
                subprocess.run([str(args.render), str(pdf), '--dpi', str(72 * zoom * ppp), '--output', str(expected_dir)], check=True)
                expected = Image.open(expected_dir / 'page-0001.png').convert('RGB')
                assert [round(physical[2]), round(physical[3])] == list(expected.size)
                path = (args.output / f'view-{zoom}.png').resolve()
                call('ui.screenshot', {'path': str(path)})
                image = Image.open(path).convert('RGB')
                vx, vy, vw, vh = state['active']['viewport']
                x, y, w, h = map(round, physical)
                left = max(x + 3, math.ceil(vx * ppp) + 3)
                top = max(y + 3, math.ceil(vy * ppp) + 3)
                right = min(x + w - 3, math.floor((vx + vw) * ppp) - 20)
                bottom = min(y + h - 3, math.floor((vy + vh) * ppp) - 20)
                shown = image.crop((left, top, right, bottom))
                reference = expected.crop((left-x, top-y, right-x, bottom-y))
                difference = ImageChops.difference(shown, reference)
                maximum = max(v[1] for v in difference.getextrema())
                assert min(v[0] for v in reference.getextrema()) < 100, 'comparison needs PDF ink'
                assert maximum <= 1, (zoom, maximum)
                report.append({'bitmap_comparison': {'zoom': zoom, 'physical_page': physical, 'max_channel_difference': maximum}})
            call('ui.quit')
            assert app.wait(timeout=15) == 0
            assert not endpoint_path.exists()
            print(f'Pixel alignment viewer smoke passed: {len(report)} records; screenshots in {args.output}')
        finally:
            (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
            if app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()


if __name__ == '__main__':
    main()
