#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise a real visible egui/wgpu window through its local control channel.

Requires a graphical desktop. Build first with cargo build --release -p pdfoxium-viewer.
Screenshots and a JSON report are retained in --output. No third-party Python
packages are required. This does not replace inspection of the screenshots.
"""
import argparse
import json
from pathlib import Path
import socket
import struct
import subprocess
import time
import zlib


def rgba_pixels(path):
    """Read our screenshot encoder's non-interlaced RGBA8 PNGs, including filters."""
    data = path.read_bytes()
    width, height = struct.unpack('>II', data[16:24])
    assert data[24:29] == bytes([8, 6, 0, 0, 0])
    payload = bytearray()
    offset = 8
    while offset < len(data):
        length = struct.unpack('>I', data[offset:offset + 4])[0]
        if data[offset + 4:offset + 8] == b'IDAT':
            payload.extend(data[offset + 8:offset + 8 + length])
        offset += length + 12
    raw = zlib.decompress(payload)
    stride = width * 4
    previous = bytearray(stride)
    for row in range(height):
        start = row * (stride + 1)
        kind = raw[start]
        current = bytearray(raw[start + 1:start + 1 + stride])
        for x in range(stride):
            left = current[x - 4] if x >= 4 else 0
            up = previous[x]
            corner = previous[x - 4] if x >= 4 else 0
            if kind == 0:
                prediction = 0
            elif kind == 1:
                prediction = left
            elif kind == 2:
                prediction = up
            elif kind == 3:
                prediction = (left + up) // 2
            elif kind == 4:
                p = left + up - corner
                distances = [abs(p - left), abs(p - up), abs(p - corner)]
                prediction = [left, up, corner][distances.index(min(distances))]
            else:
                raise AssertionError(f'unknown PNG filter {kind}')
            current[x] = (current[x] + prediction) & 255
        for x in range(0, stride, 4):
            yield tuple(current[x:x + 4])
        previous = current


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/release/pdfoxium-viewer')
    parser.add_argument('--pdf', type=Path, default=root / 'tests/render_corpus/pdfs/rectangles_multi_pages.pdf')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    endpoint_path = args.output / 'control.json'
    if endpoint_path.exists():
        raise RuntimeError(f'endpoint already exists: {endpoint_path}')
    report = []
    with (args.output / 'app.log').open('w') as log:
        app = subprocess.Popen([str(args.binary), str(args.pdf.resolve()), '--control', str(endpoint_path.resolve()), '--control-only-input'], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while not endpoint_path.exists():
                if app.poll() is not None:
                    raise RuntimeError(f'viewer exited: {app.returncode}; see app.log')
                if time.monotonic() > deadline:
                    raise RuntimeError('viewer did not create an endpoint')
                time.sleep(0.1)
            endpoint = json.loads(endpoint_path.read_text())

            def call(method, params=None, error=False):
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as conn:
                    request = {'jsonrpc': '2.0', 'id': len(report), 'token': endpoint['token'], 'method': method, 'params': params or {}}
                    conn.sendall(json.dumps(request).encode() + b'\n')
                    response = json.loads(conn.makefile('rb').readline())
                report.append({'method': method, 'params': params or {}, 'response': response})
                if error:
                    assert 'error' in response, response
                    return response
                if 'error' in response:
                    raise RuntimeError(response['error']['message'])
                return response['result']

            def wait():
                state = call('ui.wait')
                assert state['settled'] and not state['loading']
                assert state['active']['render_complete']
                assert not state['active']['errors']
                return state

            def screenshot(name):
                path = (args.output / f'{name}.png').resolve()
                capture = call('ui.screenshot', {'path': str(path)})
                png = path.read_bytes()
                assert png[:8] == b'\x89PNG\r\n\x1a\n'
                size = struct.unpack('>II', png[16:24])
                assert size == (capture['width'], capture['height'])
                assert min(size) >= 320 and len(png) > 4000
                return png

            initial = wait()
            page_count = initial['active']['page_count']
            assert page_count >= 1
            baseline = screenshot('01-open')
            # Exercise the shipped CLI client, not only the test's socket client.
            subprocess.run([str(args.binary), 'ui', '--control', str(endpoint_path), 'ui.state'], check=True, stdout=subprocess.DEVNULL)
            widgets = call('ui.inspect')['widgets']
            assert any(widget['id'] == 'view.zoom_in' for widget in widgets)
            call('ui.click', {'id': 'view.zoom_in'})
            zoomed = wait()
            assert zoomed['active']['zoom'] > initial['active']['zoom']
            assert screenshot('02-zoom-click') != baseline
            call('ui.command', {'id': 'view.zoom', 'zoom': 3.0})
            tiled = wait()
            assert abs(tiled['active']['zoom'] - 3.0) < 0.001
            call('ui.scroll', {'dx': 100, 'dy': 400})
            scrolled = wait()
            assert scrolled['active']['scroll'][1] > 0
            screenshot('03-tiles-scroll')
            if args.pdf.name == 'rectangles_multi_pages.pdf':
                # Assert visible PDF content, not just a PNG of empty UI chrome.
                colors = {(255, 255, 0, 255): 0, (0, 255, 255, 255): 0}
                for pixel in rgba_pixels(args.output / '03-tiles-scroll.png'):
                    if pixel in colors:
                        colors[pixel] += 1
                assert all(count > 1000 for count in colors.values()), colors
            call('ui.command', {'id': 'view.page', 'page': min(2, page_count)})
            wait()
            call('ui.command', {'id': 'view.rotate'})
            rotated = wait()
            assert rotated['active']['rotation'] == 90
            screenshot('04-rotated')
            call('ui.resize', {'width': 960, 'height': 720})
            call('ui.command', {'id': 'view.fit'})
            wait()
            screenshot('05-fit-resized')
            fitted = call('ui.state')
            for page in range(1, min(3, page_count) + 1):
                call('ui.command', {'id': 'view.page', 'page': page})
                assert abs(wait()['active']['zoom'] - fitted['active']['zoom']) < 0.001
            for _ in range(3):
                call('ui.command', {'id': 'view.rotate'})
            wait()
            # Zooming deep in a book must keep the same page-local point,
            # rather than scaling the fixed spacing of every preceding page.
            def point_on_page(state, point):
                active = state['active']
                for page in active['pages_on_screen']:
                    x, y, width, height = page['rect']
                    if y <= point[1] <= y + height:
                        return (page['page'], (point[0]-x)/active['zoom'], (point[1]-y)/active['zoom'])
                raise AssertionError(f'anchor is in a page gap: {point}')

            def check_anchor(expected, state, point, horizontal=False):
                page, x, y = expected
                rect = next(p['rect'] for p in state['active']['pages_on_screen'] if p['page'] == page)
                actual_y = rect[1] + y * state['active']['zoom']
                assert abs(actual_y - point[1]) <= 1.0, (page, actual_y, point)
                if horizontal:
                    actual_x = rect[0] + x * state['active']['zoom']
                    assert abs(actual_x - point[0]) <= 1.0, (page, actual_x, point)

            call('ui.command', {'id': 'view.zoom', 'zoom': 1.25})
            call('ui.command', {'id': 'view.page', 'page': min(300, max(1, page_count // 2))})
            call('ui.scroll', {'dy': 160})
            anchored = wait()
            vx, vy, vw, vh = anchored['active']['viewport']
            center = (vx + vw / 2, vy + vh / 2)
            if page_count >= 10:
                expected = point_on_page(anchored, center)
                for zoom in [2.0, 0.75, 3.0, 1.25]:
                    call('ui.command', {'id': 'view.zoom', 'zoom': zoom})
                    check_anchor(expected, wait(), center)
                screenshot('06-deep-book-zoom')
            # Inject precisely the egui Zoom event used by macOS pinch input.
            # Keep both axes scrollable so boundary clamping cannot move the anchor.
            call('ui.command', {'id': 'view.zoom', 'zoom': 6.0})
            call('ui.command', {'id': 'view.page', 'page': min(300, max(1, page_count // 2))})
            pinching = wait()
            vx, vy, vw, vh = pinching['active']['viewport']
            point = (vx + vw * 0.6, vy + vh * 0.45)
            expected = point_on_page(pinching, point)
            for factor in [1.1, 1.05, 1 / (1.1 * 1.05)]:
                call('ui.pinch', {'factor': factor, 'x': point[0], 'y': point[1]})
                state = wait()
                check_anchor(expected, state, point, horizontal=True)
            assert abs(state['active']['zoom'] - 6.0) < 0.001
            # A continuous gesture delivers several deltas before sharp rendering.
            for _ in range(5):
                call('ui.pinch', {'factor': 1.03, 'x': point[0], 'y': point[1]})
            check_anchor(expected, wait(), point, horizontal=True)
            call('ui.pinch', {'factor': 1 / 1.03**5, 'x': point[0], 'y': point[1]})
            state = wait()
            check_anchor(expected, state, point, horizontal=True)
            assert abs(state['active']['zoom'] - 6.0) < 0.001
            screenshot('07-pinch-roundtrip')
            # The UI itself must retain its scale during document pinch zoom.
            assert call('ui.inspect')['widgets'][0]['rect'][3] == widgets[0]['rect'][3]
            before = call('ui.state')
            call('ui.command', {'id': 'view.zoom', 'zoom': 0}, error=True)
            call('ui.command', {'id': 'view.page', 'page': 0}, error=True)
            assert call('ui.state')['active']['zoom'] == before['active']['zoom']
            call('ui.type', {'text': 'stale', 'revision': before['revision'] + 100}, error=True)
            call('ui.open', {'path': str(args.output / 'missing.pdf')})
            call('ui.wait', error=True)
            call('ui.open', {'path': str(args.pdf.resolve())})
            reopened = wait()
            assert reopened['active']['rotation'] == 0
            screenshot('06-reopened')
            call('ui.quit')
            assert app.wait(timeout=15) == 0
            assert not endpoint_path.exists()
            print(f'Viewer smoke passed: {len(report)} requests; screenshots in {args.output}')
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
