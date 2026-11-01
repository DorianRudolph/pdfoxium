#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Note drag preview/commit/cancel, crop/rotation and save/reopen control checks (Pillow)."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import socket
import subprocess
import sys
import time
from PIL import Image

sys.dont_write_bytecode = True
root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=root / 'target/release/pdfoxium-viewer')
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
spec = importlib.util.spec_from_file_location('text_fixture', root / 'scripts/smoke-viewer-text.py')
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
source = (args.output / 'source.pdf').resolve()
fixture.fixture(source)
source_hash = hashlib.sha256(source.read_bytes()).digest()
endpoint_path = args.output / 'control.json'
assert not endpoint_path.exists()
report = []
with (args.output / 'app.log').open('w') as log:
    app = subprocess.Popen([str(args.binary), str(source), '--control', str(endpoint_path.resolve()),
                            '--control-only-input'], stdout=log, stderr=log)
    try:
        deadline = time.monotonic() + 30
        while not endpoint_path.exists():
            assert app.poll() is None and time.monotonic() < deadline
            time.sleep(.02)
        endpoint = json.loads(endpoint_path.read_text())

        def call(method, **params):
            with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as connection:
                connection.sendall((json.dumps(dict(jsonrpc='2.0', id=len(report), token=endpoint['token'],
                                                    method=method, params=params)) + '\n').encode())
                response = json.loads(connection.makefile('rb').readline())
            report.append(dict(method=method, params=params, response=response))
            assert 'error' not in response, response
            return response['result']

        def wait():
            result = call('ui.wait')
            assert result['settled'], result

        def command(id, **params):
            call('ui.command', id=id, **params)
            wait()

        def state():
            return call('ui.state')['active']

        def annotations():
            return state()['annotations']

        def note(page=1):
            return next(h for p in annotations()['visible'] if p['page'] == page
                        for h in p['items'] if h['kind'] == 'Text')

        def center(rect):
            return [rect[0] + rect[2] / 2, rect[1] + rect[3] / 2]

        def pointer(action, position):
            call('ui.pointer', action=action, position=position)
            wait()

        def capture(name):
            path = (args.output / f'{name}.png').resolve()
            call('ui.screenshot', path=str(path))
            return Image.open(path).convert('RGB')

        def close(a, b, tolerance=1e-4):
            assert len(a) == len(b) and max(abs(x-y) for x, y in zip(a, b)) < tolerance, (a, b)

        wait()
        command('view.zoom', zoom=3.)
        command('view.annotations')
        command('annotation.note', text='Drag me', x=.3, y=.3)
        before = (args.output / 'before.pdf').resolve()
        command('document.save', path=str(before))
        call('ui.open', path=str(before)); wait()
        command('view.zoom', zoom=3.)
        assert not annotations()['open'] and not state()['forms']['enabled']
        old = note(); start = center(old['screen_rects'][0]); end = [start[0]+90, start[1]+60]
        original_image = capture('before')
        pointer('down', start); pointer('move', end)
        preview = annotations()
        assert preview['dragging'] and not preview['open'], preview
        assert not state()['forms']['dirty']
        close(note()['rect'], old['rect'])
        close(preview['move_preview']['rect'][:2], [old['rect'][0]+.15, old['rect'][1]+.1])
        preview_image = capture('preview')
        assert preview_image.tobytes() != original_image.tobytes()
        pointer('up', end)
        moved = note()
        assert not annotations()['moving'] and not annotations()['open']
        assert state()['forms']['dirty']
        close(moved['rect'][2:], old['rect'][2:])
        close(moved['rect'][:2], [old['rect'][0]+.15, old['rect'][1]+.1])
        capture('moved')
        after = (args.output / 'after.pdf').resolve()
        command('document.save', path=str(after))
        call('ui.open', path=str(after)); wait(); command('view.zoom', zoom=3.)
        close(note()['rect'], moved['rect'])
        # Escape and pinch/toolbar zoom cancel without dirtying, even while held.
        for cancel in ['escape', 'zoom']:
            old = note(); start = center(old['screen_rects'][0]); end = [start[0]+35,start[1]+25]
            pointer('down', start); pointer('move', end)
            assert annotations()['dragging']
            if cancel == 'escape': call('ui.key', key='Escape'); wait()
            else: call('ui.pinch', factor=.9, x=end[0], y=end[1]); wait()
            assert not annotations()['dragging']
            pointer('move', [end[0]+10,end[1]+10]); pointer('up', end)
            close(note()['rect'], old['rect'])
            assert not state()['forms']['dirty']
            command('view.zoom', zoom=3.)
        # Every view rotation, plus intrinsic page rotation, preserves PDF geometry.
        for page in [1, 2]:
            command('view.page', page=page)
            if page == 2: command('annotation.note', text='Intrinsic rotation', x=.3,y=.3)
            if annotations()['open']: command('view.annotations')
            command('view.zoom', zoom=3.)
            for rotation in range(4):
                old = note(page); start = center(old['screen_rects'][0]); delta=[24.,18.]
                end=[start[0]+delta[0],start[1]+delta[1]]
                call('ui.drag', **{'from':start,'to':end,'steps':4}); wait()
                moved=note(page)
                dx,dy=delta[0]/600,delta[1]/600
                d=[(dx,dy),(dy,-dx),(-dx,-dy),(-dy,dx)][rotation]
                close(moved['rect'][:2],[old['rect'][0]+d[0],old['rect'][1]+d[1]])
                close(moved['rect'][2:],old['rect'][2:])
                command('view.rotate')
        command('view.page', page=1)
        old=note(); start=center(old['screen_rects'][0])
        # Clamp at original page boundary; no cross-page transfer.
        rect=next(p['rect'] for p in state()['pages_on_screen'] if p['page']==1)
        end=[rect[0]+rect[2]+50,rect[1]-30]
        call('ui.drag', **{'from':start,'to':end,'steps':4}); wait()
        clamped=note()
        close(clamped['rect'][:2],[1.-old['rect'][2],0.])
        close(clamped['rect'][2:],old['rect'][2:])
        command('document.save', path=str((args.output/'clamped.pdf').resolve()))
        assert hashlib.sha256(source.read_bytes()).digest()==source_hash
        call('ui.quit'); assert app.wait(timeout=15)==0
        print(f'Note drag smoke passed: {len(report)} requests; artifacts in {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None: app.terminate(); app.wait(timeout=10)
