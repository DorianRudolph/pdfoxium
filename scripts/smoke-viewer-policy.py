#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise monitor-relative rendering hysteresis in the real release viewer (Pillow)."""
import argparse
import json
import math
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image, ImageChops


def thin_pdf(path, width):
    content = f'0.2 0.6 0.9 rg 0 0 {width} 20 re f'
    objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} 20] /Contents 4 0 R >>', f'<< /Length {len(content)} >>\nstream\n{content}\nendstream']
    pdf = '%PDF-1.7\n'
    offsets = []
    for i, obj in enumerate(objects):
        offsets.append(len(pdf))
        pdf += f'{i + 1} 0 obj\n{obj}\nendobj\n'
    xref = len(pdf)
    pdf += 'xref\n0 5\n0000000000 65535 f \n'
    pdf += ''.join(f'{offset:010} 00000 n \n' for offset in offsets)
    pdf += f'trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
    path.write_text(pdf)


def policy_pdf(path):
    content = '0 0 0 rg BT /F1 24 Tf 60 900 Td (Screen-relative rendering) Tj ET\n'
    for y in range(100, 900, 80):
        content += f'BT /F1 18 Tf 80 {y} Td (Same pixels in whole-page and tiled rendering.) Tj ET\n'
    content += '0.2 0.6 0.9 RG 2 w 20 20 m 980 980 l S'
    objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1000 1000] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>', f'<< /Length {len(content)} >>\nstream\n{content}\nendstream', '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
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
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    endpoint_path = args.output / 'control.json'
    assert not endpoint_path.exists()
    report = []
    pdf = (args.output / 'screen-policy.pdf').resolve()
    policy_pdf(pdf)
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
            policy = initial['active']['render_policy']
            assert policy['screen_source'] == 'monitor', policy
            monitor = policy['monitor_pixels']
            area = monitor[0] * monitor[1]
            full = policy['pages'][0]['full_pixels']
            base_area = full[0] * full[1] / initial['active']['zoom']**2

            def at_area(ratio, expected):
                zoom = math.sqrt(ratio * area / base_area)
                assert 0.1 <= zoom <= 8.0, 'test page cannot reach this monitor ratio within viewer zoom bounds'
                call('ui.command', {'id': 'view.zoom', 'zoom': zoom})
                state = wait()
                policy = state['active']['render_policy']
                assert policy['monitor_pixels'] == monitor
                page = policy['pages'][0]
                assert page['mode'] == expected, (ratio, page, policy)
                assert page['full_pixels'][0] <= policy['max_texture_side'] and page['full_pixels'][1] <= policy['max_texture_side'], 'this part of the test must exercise area policy, not GPU forcing'
                return state

            def capture(name):
                path = (args.output / f'{name}.png').resolve()
                call('ui.screenshot', {'path': str(path)})
                return Image.open(path).convert('RGB')

            at_area(2.5, 'whole')
            whole = at_area(3.5, 'whole')
            image_whole = capture('01-whole-band')
            at_area(4.5, 'tiled')
            tiled = at_area(3.5, 'tiled')
            image_tiled = capture('02-tiled-band')
            # Same view and same zoom, reached from opposite sides of hysteresis.
            assert abs(whole['active']['zoom'] - tiled['active']['zoom']) < 0.00001
            for a, b in zip(whole['active']['scroll'], tiled['active']['scroll']):
                assert abs(a - b) < 0.5, (a, b)
            scale = image_whole.width / whole['window']['width']
            vx, vy, vw, vh = whole['active']['viewport']
            crop = tuple(round(v * scale) for v in (vx + 2, vy + 2, vx + vw - 16, vy + vh - 16))
            difference = ImageChops.difference(image_whole.crop(crop), image_tiled.crop(crop))
            extrema = difference.getextrema()
            pixels = difference.get_flattened_data() if hasattr(difference, 'get_flattened_data') else difference.getdata()
            changed = sum(max(pixel) > 0 for pixel in pixels)
            assert max(v[1] for v in extrema) <= 2, extrema
            assert min(v[0] for v in image_whole.crop(crop).getextrema()) < 100, 'comparison must contain visible PDF ink'
            report.append({'whole_vs_tiled': {'max_channel_difference': max(v[1] for v in extrema), 'different_pixels': changed, 'crop': crop}})
            call('ui.scroll', {'dy': 80})
            assert wait()['active']['render_policy']['pages'][0]['mode'] == 'tiled'
            call('ui.resize', {'width': 960, 'height': 720})
            assert wait()['active']['render_policy']['monitor_pixels'] == monitor
            at_area(3.5, 'tiled')
            at_area(2.5, 'whole')
            at_area(3.5, 'whole')
            capture('03-whole-after-exit')

            # A narrow strip has tiny area, but one dimension cannot fit a texture.
            limit = policy['max_texture_side']
            ppp = policy['pixels_per_point']
            path = (args.output / 'gpu-limit-strip.pdf').resolve()
            thin_pdf(path, (limit + 1024) / ppp)
            call('ui.open', {'path': str(path)})
            wait()
            call('ui.command', {'id': 'view.zoom', 'zoom': 1.0})
            strip = wait()
            page = strip['active']['render_policy']['pages'][0]
            assert page['mode'] == 'tiled' and max(page['full_pixels']) > limit, page
            assert page['full_pixels'][0] * page['full_pixels'][1] < 3 * area
            capture('04-gpu-forced-tiles')
            call('ui.quit')
            assert app.wait(timeout=15) == 0
            assert not endpoint_path.exists()
            print(f'Policy viewer smoke passed: {len(report)} records; monitor {monitor}, GPU limit {limit}; screenshots in {args.output}')
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
