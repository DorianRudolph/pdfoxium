#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Live selection/search/highlight checks against the release viewer (requires Pillow)."""
import argparse
import json
from pathlib import Path
import socket
import subprocess
import sys
import time
from PIL import Image


def fixture(path):
    objects = ['<< /Type /Catalog /Pages 2 0 R >>', '', '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
    kids = []
    for index in range(5):
        page = len(objects) + 1
        kids.append(f'{page} 0 R')
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 260] /CropBox [10 20 210 220] /Rotate {index % 4 * 90} /Resources << /Font << /F1 3 0 R >> >> /Contents {page + 1} 0 R >>')
        content = ('/Span << /ActualText <FEFF004100E9D83DDE420042> >> BDC BT /F1 12 Tf 30 130 Td (ABCD) Tj ET EMC' if index == 4 else 'BT /F1 12 Tf 30 130 Td (Hello banana BANANA) Tj ET')
        objects.append(f'<< /Length {len(content)} >>\nstream\n{content}\nendstream')
    objects[1] = f'<< /Type /Pages /Kids [{" ".join(kids)}] /Count 5 >>'
    pdf = '%PDF-1.7\n'
    offsets = []
    for index, value in enumerate(objects):
        offsets.append(len(pdf))
        pdf += f'{index + 1} 0 obj\n{value}\nendobj\n'
    xref = len(pdf)
    pdf += f'xref\n0 {len(objects) + 1}\n0000000000 65535 f \n'
    pdf += ''.join(f'{offset:010} 00000 n \n' for offset in offsets)
    pdf += f'trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
    path.write_bytes(pdf.encode())


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/release/pdfoxium-viewer')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--book', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    endpoint_path = args.output / 'control.json'
    if endpoint_path.exists():
        raise RuntimeError('remove the old control endpoint before restarting')
    pdf = (args.output / 'text-crop-rotations.pdf').resolve()
    fixture(pdf)
    report = []
    clipboard = subprocess.check_output(['pbpaste']) if sys.platform == 'darwin' else None
    with (args.output / 'app.log').open('w') as log:
        app = subprocess.Popen([str(args.binary), str(pdf), '--control', str(endpoint_path.resolve()), '--control-only-input'], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while not endpoint_path.exists():
                assert app.poll() is None, 'viewer exited; see app.log'
                assert time.monotonic() < deadline, 'viewer did not start'
                time.sleep(0.05)
            endpoint = json.loads(endpoint_path.read_text())

            def call(method, params=None, error=False):
                start = time.monotonic()
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as conn:
                    conn.sendall(json.dumps({'jsonrpc': '2.0', 'id': len(report), 'token': endpoint['token'], 'method': method, 'params': params or {}}).encode() + b'\n')
                    response = json.loads(conn.makefile('rb').readline())
                report.append({'method': method, 'params': params or {}, 'seconds': time.monotonic() - start, 'response': response})
                if error:
                    assert 'error' in response, response
                    return response
                if 'error' in response:
                    raise RuntimeError(response['error']['message'])
                return response['result']

            def command(id, **params):
                return call('ui.command', {'id': id, **params})

            def wait():
                state = call('ui.wait')
                assert state['settled'] and state['active']['render_complete']
                return state

            def capture(name):
                path = (args.output / f'{name}.png').resolve()
                info = call('ui.screenshot', {'path': str(path)})
                image = Image.open(path).convert('RGB')
                assert image.size == (info['width'], info['height'])
                return image

            def selected_character(page, start, end):
                command('selection.set', page=page, start=start, end=end)
                wait()
                selection = call('ui.selection')
                rect = selection['pages'][0]['screen_rects'][0]
                return [rect[0] + rect[2] / 2, rect[1] + rect[3] / 2]

            def verify_overlay(image, baseline, rects, color):
                state = call('ui.state')
                scale = image.width / state['window']['width']
                tinted = ink = changed = 0
                for x, y, width, height in rects:
                    bounds = (max(0, int(x * scale)), max(0, int(y * scale)), min(image.width, int((x + width) * scale + 1)), min(image.height, int((y + height) * scale + 1)))
                    colored = image.crop(bounds)
                    plain = baseline.crop(bounds)
                    pixels_a = colored.get_flattened_data() if hasattr(colored, 'get_flattened_data') else colored.getdata()
                    pixels_b = plain.get_flattened_data() if hasattr(plain, 'get_flattened_data') else plain.getdata()
                    for a, b in zip(pixels_a, pixels_b):
                        ink += max(b) < 150
                        changed += a != b
                        if color == 'teal':
                            tinted += a[1] > a[0] + 15 and a[2] > a[0] + 10 and a[1] > a[2]
                        else:
                            tinted += a[0] > a[2] + 35 and a[1] > a[2] + 25
                assert ink > 10 and changed > 20 and tinted > 20, (color, ink, changed, tinted)

            wait()
            # Typing/refinement must highlight the current page without scrolling.
            command('view.page', page=3)
            wait()
            before = call('ui.state')['active']
            call('ui.key', {'key': 'F', 'modifiers': ['command']})
            wait()
            call('ui.type', {'text': 'banan'})
            wait()
            call('ui.type', {'text': 'a'})
            wait()
            after = call('ui.state')['active']
            result = call('ui.search')
            assert after['page'] == before['page'] == 3, (before, after)
            assert after['scroll'] == before['scroll'], (before['scroll'], after['scroll'])
            assert result['state']['active'] is None and result['state']['start_page'] == 3, result
            hit = result['hits'][0]
            assert hit['page'] == 3, hit
            highlighted = capture('00-current-page-search')
            command('search.find', query='')
            wait()
            plain = capture('00-current-page-plain')
            verify_overlay(highlighted, plain, hit['screen_rects'], 'amber')
            command('search.find', query='banana')
            wait()
            call('ui.click', {'id': 'search.query'})
            call('ui.key', {'key': 'Enter'})
            wait()
            assert call('ui.search')['state']['active']['page'] == 3
            # Walk every hit across the final page and through earlier pages.
            for expected_page in [3, 4, 4, 1, 1, 2, 2, 3]:
                command('search.next')
                wait()
                assert call('ui.search')['state']['active']['page'] == expected_page
                if expected_page == 1:
                    capture('00-wrapped-page-one')
            command('search.clear')
            command('view.page', page=1)
            wait()
            # Real keyboard focus, typing, and current-page search through the Find field.
            call('ui.key', {'key': 'F', 'modifiers': ['command']})
            wait()
            call('ui.type', {'text': 'banana'})
            wait()
            search = call('ui.search')
            assert search['state']['query'] == 'banana' and search['state']['count'] == 8, search
            call('ui.click', {'id': 'search.case'})
            wait()
            assert call('ui.search')['state']['count'] == 4
            assert call('ui.search')['state']['active'] is None
            command('search.next')
            wait()
            assert call('ui.search')['state']['active']['page'] == 1
            command('search.next')
            wait()
            assert call('ui.search')['state']['active']['page'] == 2
            command('search.previous')
            wait()
            assert call('ui.search')['state']['active']['page'] == 1

            call('ui.key', {'key': 'G', 'modifiers': ['command']})
            wait()
            assert call('ui.search')['state']['active']['page'] == 2
            call('ui.key', {'key': 'G', 'modifiers': ['command', 'shift']})
            wait()
            assert call('ui.search')['state']['active']['page'] == 1
            call('ui.click', {'id': 'search.query'})
            wait()
            call('ui.key', {'key': 'Enter'})
            wait()
            assert call('ui.search')['state']['active']['page'] == 2
            call('ui.key', {'key': 'Enter', 'modifiers': ['shift']})
            wait()
            assert call('ui.search')['state']['active']['page'] == 1

            # Drag between actual first/last glyph centers, then exercise native Copy input.
            first = selected_character(1, 6, 7)
            last = selected_character(1, 11, 12)
            command('selection.clear')
            wait()
            call('ui.drag', {'from': first, 'to': last})
            wait()
            selection = call('ui.selection')
            assert selection['text'] == 'banana', selection
            call('ui.key', {'key': 'C', 'modifiers': ['command']})
            wait()
            if clipboard is not None:
                assert subprocess.check_output(['pbpaste']).decode() == 'banana'
            assert command('selection.copy')['text'] == 'banana'
            command('search.find', query='')
            wait()
            selected_image = capture('01-selection')
            selection = call('ui.selection')
            command('selection.clear')
            wait()
            baseline = capture('02-no-selection')
            verify_overlay(selected_image, baseline, selection['pages'][0]['screen_rects'], 'teal')

            # Search highlights must cover visible glyph ink at all four view rotations.
            for rotation in range(4):
                if rotation:
                    command('view.rotate')
                command('search.find', query='banana', match_case=True)
                wait()
                command('search.next')
                wait()
                hit = call('ui.search')['state']['active']
                highlighted = capture(f'03-search-rotation-{rotation}')
                command('search.find', query='')
                wait()
                plain = capture(f'04-plain-rotation-{rotation}')
                verify_overlay(highlighted, plain, hit['screen_rects'], 'amber')
            # Restore normal view rotation and select backwards across intrinsically rotated pages.
            command('view.rotate')
            command('view.zoom', zoom=1.0)
            command('view.page', page=1)
            wait()
            first = selected_character(1, 13, 14)
            last = selected_character(2, 4, 5)
            command('selection.clear')
            wait()
            call('ui.drag', {'from': last, 'to': first})
            wait()
            selection = call('ui.selection')
            assert selection['text'] == 'BANANA\nHello' and len(selection['pages']) == 2, selection
            capture('05-cross-page-selection')

            command('search.find', query='é🙂')
            wait()
            assert call('ui.search')['state']['count'] == 1
            command('selection.set', page=5, start=1, end=7)
            wait()
            assert call('ui.selection')['text'] == 'é🙂'
            capture('06-unicode')
            command('selection.set', page=5, start=2, end=3)
            call('ui.wait', error=True)
            command('selection.clear')
            command('search.find', query='banana')
            command('search.find', query='not-present')
            wait()
            assert call('ui.search')['state']['count'] == 0
            command('search.clear')
            if args.book:
                call('ui.open', {'path': str(args.book.resolve())})
                wait()
                command('search.find', query='linear')
                call('ui.state')
                assert report[-1]['seconds'] < 1.0, 'UI blocked during background search'
                command('search.find', query='banana')
                command('search.find', query='basis')
                state = wait()
                search = call('ui.search')['state']
                assert search['count'] > 0 and search['scanned_pages'] == state['active']['page_count'] and not search['truncated'], search
                capture('07-book-search')
                command('search.clear')
            call('ui.open', {'path': str(pdf)})
            wait()
            assert call('ui.selection')['text'] == '' and call('ui.search')['state']['count'] == 0
            call('ui.quit')
            assert app.wait(timeout=15) == 0
            assert not endpoint_path.exists()
            print(f'Text viewer smoke passed: {len(report)} requests; screenshots in {args.output}')
        finally:
            (args.output / 'report.json').write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
            if app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()
            if clipboard is not None:
                subprocess.run(['pbcopy'], input=clipboard, check=True)


if __name__ == '__main__':
    main()
