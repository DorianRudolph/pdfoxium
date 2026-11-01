#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise embedded/fallback thumbnail previews, page labels and unchanged source bytes (Pillow)."""
import argparse
import json
from pathlib import Path
import socket
import subprocess
import time
import zlib
import hashlib
from PIL import Image


def thumbnail_pdf(path):
    objects = []
    def add(value):
        objects.append(value.encode() if isinstance(value, str) else value)
        return len(objects)
    def stream(dictionary, data):
        return add(f'<< {dictionary} /Length {len(data)} >>\nstream\n'.encode()+data+b'\nendstream')
    catalog = add('')
    pages = add('')
    pixels = bytearray()
    for y in range(512):
        for x in range(512):
            pixels.extend(([255,0,0] if x<256 else [0,255,0]) if y<256 else ([0,0,255] if x<256 else [255,255,0]))
    embedded = stream('/Type /XObject /Subtype /Image /Width 512 /Height 512 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /FlateDecode', zlib.compress(pixels))
    small = stream('/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB', bytes([0,255,0]*4))
    broken = stream('/Type /XObject /Subtype /Image /Width 0 /Height 0 /BitsPerComponent 8 /ColorSpace /DeviceRGB', b'')
    wrong_aspect = stream('/Type /XObject /Subtype /Image /Width 256 /Height 512 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /FlateDecode', zlib.compress(bytes([255,0,0])*256*512))
    content = stream('', b'0 0 1 rg 0 0 200 200 re f')
    page_ids = []
    for index in range(100):
        thumb = {0:embedded, 2:small, 3:broken, 4:wrong_aspect}.get(index)
        thumb = f'/Thumb {thumb} 0 R' if thumb else ''
        page_ids.append(add(f'<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 200 200] /Resources <<>> /Contents {content} 0 R {thumb} >>'))
    outlines = add('')
    bookmark = add(f'<< /Title () /Parent {outlines} 0 R /Dest [{page_ids[0]} 0 R /Fit] >>')
    objects[outlines-1] = f'<< /Type /Outlines /First {bookmark} 0 R /Last {bookmark} 0 R /Count 1 >>'.encode()
    objects[catalog-1] = f'<< /Type /Catalog /Pages {pages} 0 R /Outlines {outlines} 0 R /PageLabels << /Nums [0 << /S /r >> 2 << /S /D /St 1 >>] >> >>'.encode()
    objects[pages-1] = ('<< /Type /Pages /Kids [' + ' '.join(f'{page} 0 R' for page in page_ids) + '] /Count 100 >>').encode()
    pdf = bytearray(b'%PDF-1.7\n')
    offsets = []
    for index, obj in enumerate(objects):
        offsets.append(len(pdf))
        pdf.extend(f'{index+1} 0 obj\n'.encode()+obj+b'\nendobj\n')
    xref = len(pdf)
    pdf.extend(f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'.encode())
    for offset in offsets:
        pdf.extend(f'{offset:010} 00000 n \n'.encode())
    pdf.extend(f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
    path.write_bytes(pdf)


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
    pdf = (args.output / 'thumbnails.pdf').resolve()
    thumbnail_pdf(pdf)
    digest = hashlib.sha256(pdf.read_bytes()).digest()
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
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as conn:
                    conn.sendall(json.dumps({'jsonrpc': '2.0', 'id': len(report), 'token': endpoint['token'], 'method': method, 'params': params or {}}).encode() + b'\n')
                    response = json.loads(conn.makefile('rb').readline())
                report.append({'method': method, 'params': params or {}, 'response': response})
                if error:
                    assert 'error' in response, response
                    return response['error']
                assert 'error' not in response, response
                return response['result']

            def wait():
                state = call('ui.wait')
                assert state['settled'] and state['active']['render_complete']
                return state

            initial = wait()
            labels = call('ui.pages', {'limit': 5})['pages']
            assert [p['label'] for p in labels] == ['i', 'ii', '1', '2', '3'], labels
            def command(id, **params):
                call('ui.command', {'id': id, **params})
                return wait()
            call('ui.click', {'id': 'view.page'})
            call('ui.key', {'key':'A','modifiers':['command']})
            call('ui.type', {'text':'ii'})
            call('ui.key', {'key':'Enter'})
            wait()
            assert wait()['active']['page'] == 2
            command('view.page', page=1)
            call('ui.click', {'id': 'view.thumbnails'})
            state = wait()
            previews = call('ui.thumbnails')
            assert previews['open'] and previews['complete'] and previews['pending'] == 0
            rows = {row['page']:row for row in previews['rows']}
            assert rows[1]['source'] == 'embedded', previews
            assert rows[2]['source'] == 'rendered' and rows[3]['source'] == 'rendered', previews
            assert previews['cached'] < 100 and previews['cache_bytes'] <= previews['budget']
            def capture(name):
                path = (args.output / f'{name}.png').resolve()
                call('ui.screenshot', {'path': str(path)})
                return Image.open(path).convert('RGB')
            image = capture('01-embedded-and-rendered')
            scale = image.width/state['window']['width']
            def sample(row, dx, dy, expected):
                x,y,w,h = row['rect']
                actual = image.getpixel((round((x+w/2+dx)*scale),round((y+80+dy)*scale)))
                assert max(abs(a-b) for a,b in zip(actual,expected)) <= 3, (actual,expected)
            sample(rows[1],-40,-40,(255,0,0))
            sample(rows[1],40,-40,(0,255,0))
            sample(rows[2],0,0,(0,0,255))
            # Thumbnail clicks use the same history-bearing page navigation.
            x,y,w,h = rows[2]['rect']
            call('ui.click', {'position':[x+w/2,y+h/2]})
            assert wait()['active']['page'] == 2
            command('navigation.back')
            # Rotation reuses the preview cache and rotates its UVs correctly.
            command('view.rotate')
            state = wait()
            rows = {row['page']:row for row in call('ui.thumbnails')['rows']}
            image = capture('02-rotated-preview')
            scale = image.width/state['window']['width']
            sample(rows[1],-40,-40,(0,0,255))
            # Offscreen work and large lists remain bounded; absent, malformed,
            # undersized and wrong-aspect embedded images all render substitutes.
            command('thumbnails.page', page=4)
            rows = {row['page']:row for row in call('ui.thumbnails')['rows']}
            assert rows[4]['source'] == 'rendered' and rows[5]['source'] == 'rendered'
            for page in [30,60,90,1]:
                command('thumbnails.page', page=page)
                previews = call('ui.thumbnails')
                assert previews['cache_bytes'] <= previews['budget'] and previews['cached'] <= 64
                assert previews['pending'] == 0
            for page in [10,40,70]:
                call('ui.command', {'id':'thumbnails.page','page':page})
            wait()
            command('search.find', query='no-such-text')
            assert call('ui.thumbnails')['pending'] == 0
            command('search.clear')
            command('view.thumbnails')
            assert not call('ui.thumbnails')['open']
            command('view.outline')
            outline = call('ui.outline')['items'][0]
            assert outline['title'] == '' and outline['page_label'] == 'i'
            assert outline['destination_page'] == 1 and outline['rect'] is not None
            capture('03-outline-label')
            call('ui.open', {'path':str(pdf)})
            wait()
            assert call('ui.thumbnails')['cached'] == 0
            assert hashlib.sha256(pdf.read_bytes()).digest() == digest
            call('ui.quit')
            assert app.wait(timeout=15) == 0
            assert not endpoint_path.exists()
            assert hashlib.sha256(pdf.read_bytes()).digest() == digest
            print(f'Thumbnail viewer smoke passed: {len(report)} records; screenshots in {args.output}')
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
