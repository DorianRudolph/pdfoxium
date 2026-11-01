#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise compact viewer menus, responsive chrome and contextual editing."""
import argparse
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image, ImageOps

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
pdf = root/'crates/pdfoxium-viewer/assets/demos/editing-demo.pdf'
source_hash = hashlib.sha256(pdf.read_bytes()).hexdigest()
endpoint_path = args.output/'control.json'
assert not endpoint_path.exists()
report = []
with (args.output/'app.log').open('w') as log:
    app = subprocess.Popen([str(args.binary), str(pdf), '--control', str(endpoint_path.resolve()),
                            '--control-only-input'], stdout=log, stderr=log)
    try:
        deadline = time.monotonic()+30
        while not endpoint_path.exists():
            assert app.poll() is None and time.monotonic() < deadline
            time.sleep(.02)
        endpoint = json.loads(endpoint_path.read_text())

        def call(method, **params):
            with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=50) as conn:
                conn.sendall((json.dumps(dict(jsonrpc='2.0', id=len(report), token=endpoint['token'],
                                             method=method, params=params))+'\n').encode())
                response = json.loads(conn.makefile('rb').readline())
            report.append(dict(method=method, params=params, response=response))
            assert 'error' not in response, response
            return response['result']

        def wait():
            assert call('ui.wait')['settled']

        def command(id, **params):
            call('ui.command', id=id, **params)
            wait()

        def click(id):
            call('ui.click', id=id)
            wait()

        def capture(name):
            path = (args.output/f'{name}.png').resolve()
            call('ui.screenshot', path=str(path))
            return Image.open(path).convert('RGB')

        def inspect(query=''):
            return call('ui.inspect', query=query)['widgets']

        def annotations():
            return call('ui.annotations')

        def edit(id, text):
            click(id)
            call('ui.key', key='A', modifiers=['command'])
            call('ui.type', text=text)

        wait()
        command('view.zoom', zoom=.8)
        initial = call('ui.state')
        viewport = initial['active']['viewport']
        assert viewport[1] <= 36, viewport  # One 28-point row plus panel margins.
        assert initial['window']['height'] - viewport[1] - viewport[3] <= 24, viewport
        assert not inspect('file.path')
        assert not inspect('annotation.comment')
        image = capture('toolbar')
        scale = image.width / initial['window']['width']
        assert image.getpixel((round(2*scale),round(8*scale))) == (247,245,238), 'warm toolbar surface'
        assert image.getpixel((round(2*scale),round((viewport[1]+20)*scale))) == (220,225,216), 'sage canvas'
        click('menu.file')
        assert any(w['id']=='file.open' for w in inspect())
        capture('file-menu')
        call('ui.key', key='Escape')
        click('menu.zoom')
        click('view.fit')
        assert call('ui.state')['active']['zoom'] != .8
        command('view.zoom', zoom=.8)
        click('sidebar.toggle')
        assert inspect('view.thumbnails') and inspect('view.outline')
        click('view.outline')
        capture('outline')
        click('view.thumbnails')
        capture('thumbnails')
        click('sidebar.toggle')
        click('view.annotations')
        assert not inspect('annotation.comment') and not inspect('annotation.style.font_size')
        image = capture('annotation-list')
        scale = image.width / call('ui.state')['window']['width']
        x, y, w, h = next(w['rect'] for w in inspect() if w['id']=='annotation.text')
        icon = image.crop(tuple(round(v*scale) for v in [x,y,x+w,y+h]))
        ink = icon.convert('L').point(lambda value: 255 if value < 180 else 0)
        assert ink.getbbox(), 'text-tool icon has no visible ink'
        icon = icon.crop(ink.getbbox())
        assert icon.tobytes() == ImageOps.mirror(icon).tobytes(), 'text-tool serifs must be symmetric'
        hit = next(h for p in annotations()['visible'] for h in p['items'] if h['kind']=='FreeText')
        x, y, w, h = hit['screen_rects'][0]
        call('ui.click', position=[x+w/2, y+h/2])
        wait()
        # Both drafts commit through the same visible button, and only then.
        edit('annotation.comment', 'Edited text and appearance together')
        edit('annotation.style.font_size', '18')
        call('ui.key', key='Enter')
        state = annotations()
        original = next(r for r in state['items'] if r['kind']=='FreeText')
        assert original['contents'] != 'Edited text and appearance together'
        assert original['style']['font_size'] != 18
        assert state['selected']['style']['font_size'] == 18
        apply = inspect('annotation.apply_comment')[0]
        assert apply['rect'] == inspect('annotation.apply_style')[0]['rect']
        click('annotation.apply_comment')
        row = next(r for r in annotations()['items'] if r['kind']=='FreeText')
        assert row['contents']=='Edited text and appearance together' and row['style']['font_size']==18
        capture('text-editor')
        command('document.save', path=str((args.output/'edited.pdf').resolve()))
        call('ui.key', key='Escape')
        click('annotation.rectangle')
        assert inspect('annotation.style.border_width') and not inspect('annotation.comment')
        capture('shape-tool')
        call('ui.key', key='Escape')
        click('annotation.place_note')
        assert annotations()['placing_note'] and not inspect('annotation.style.border_width')
        capture('note-tool')
        call('ui.key', key='Escape')
        for width in [720, 480, 1100]:
            call('ui.resize', width=width, height=720)
            wait()
            widgets = inspect()
            toolbar = [w for w in widgets if w['id'] in ['menu.file','sidebar.toggle','view.page','menu.zoom','view.annotations','search.open']]
            assert len(toolbar)==6
            assert max(w['rect'][1] for w in toolbar)-min(w['rect'][1] for w in toolbar)<12
            assert all(w['rect'][0]>=0 and w['rect'][0]+w['rect'][2]<=width for w in toolbar), toolbar
            ordered = sorted(toolbar,key=lambda w:w['rect'][0])
            assert all(a['rect'][0]+a['rect'][2]<=b['rect'][0] for a,b in zip(ordered,ordered[1:])), ordered
            click('search.open')
            edit('search.query', 'text')
            wait()
            capture(f'width-{width}-search')
            click('search.clear')
        assert hashlib.sha256(pdf.read_bytes()).hexdigest()==source_hash
        call('ui.quit')
        assert app.wait(timeout=15)==0
        print(f'Viewer chrome smoke passed: {len(report)} requests; artifacts in {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:
            app.terminate()
            app.wait(timeout=10)
