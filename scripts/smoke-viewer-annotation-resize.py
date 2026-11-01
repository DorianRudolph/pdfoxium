#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Resize existing free text/shapes through real pointer input (requires Pillow)."""
import argparse
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image


def fixture(path):
    objects = ['<< /Type /Catalog /Pages 2 0 R /AcroForm << /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 3 0 R >> >> >> >>', '',
               '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
    kids = []
    for rotation in [0, 90, 180, 270]:
        page = len(objects)+1
        kids.append(f'{page} 0 R')
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 260] /CropBox [10 20 210 220] /Rotate {rotation} /Resources << /Font << /F1 3 0 R >> >> /Contents {page+1} 0 R /Annots [{page+2} 0 R {page+3} 0 R {page+4} 0 R {page+5} 0 R {page+6} 0 R] >>')
        content = 'BT /F1 12 Tf 30 125 Td (Underlying page text) Tj ET'
        objects.append(f'<< /Length {len(content)} >>\nstream\n{content}\nendstream')
        objects.extend([
            '<< /Type /Annot /Subtype /FreeText /Rect [30 150 110 190] /Contents (Resizable free text) /DA (/Helv 12 Tf 0 g) /C [1 1 0.8] /BS << /W 1 /S /S >> >>',
            '<< /Type /Annot /Subtype /Square /Rect [130 150 190 190] /C [0.8 0.1 0.2] /IC [0.95 0.85 0.7] /BS << /W 2 /S /S >> >>',
            '<< /Type /Annot /Subtype /Circle /Rect [30 40 90 100] /C [0.1 0.4 0.8] /BS << /W 2 /S /S >> >>',
            '<< /Type /Annot /Subtype /Text /Rect [140 40 160 60] /Contents (Fixed icon) >>',
            '<< /Type /Annot /Subtype /Highlight /Rect [30 122 100 137] /QuadPoints [30 137 100 137 30 122 100 122] /C [1 1 0] >>',
        ])
    objects[1] = f'<< /Type /Pages /Kids [{" ".join(kids)}] /Count 4 >>'
    pdf = '%PDF-1.7\n'; offsets = []
    for i, value in enumerate(objects):
        offsets.append(len(pdf)); pdf += f'{i+1} 0 obj\n{value}\nendobj\n'
    xref = len(pdf)
    pdf += f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'
    pdf += ''.join(f'{offset:010} 00000 n \n' for offset in offsets)
    pdf += f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
    path.write_bytes(pdf.encode())


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root/'target/release/pdfoxium-viewer')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=True)
    source = (args.output/'source.pdf').resolve(); fixture(source)
    source_hash = hashlib.sha256(source.read_bytes()).digest()
    endpoint_path = args.output/'control.json'; assert not endpoint_path.exists()
    report = []
    with (args.output/'app.log').open('w') as log:
        app = subprocess.Popen([str(args.binary), str(source), '--control', str(endpoint_path.resolve()),
                                '--control-only-input'], stdout=log, stderr=log)
        try:
            deadline = time.monotonic()+30
            while not endpoint_path.exists():
                assert app.poll() is None and time.monotonic() < deadline
                time.sleep(.02)
            endpoint = json.loads(endpoint_path.read_text())

            def call(method, **params):
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as connection:
                    connection.sendall((json.dumps(dict(jsonrpc='2.0', id=len(report), token=endpoint['token'], method=method, params=params))+'\n').encode())
                    response = json.loads(connection.makefile('rb').readline())
                report.append(dict(method=method, params=params, response=response))
                assert 'error' not in response, response
                return response['result']

            def wait():
                result = call('ui.wait'); assert result['settled'], result

            def command(id, **params):
                call('ui.command', id=id, **params); wait()

            def state():
                return call('ui.state')['active']

            def annotations():
                result = state()['annotations']; assert not result['error'], result
                return result

            def item(kind, page=1):
                return next(h for p in annotations()['visible'] if p['page'] == page for h in p['items'] if h['kind'] == kind)

            def close(a, b):
                assert max(abs(x-y) for x, y in zip(a, b)) < 1e-4, (a, b)

            def select(kind, page=1):
                r = item(kind, page)['screen_rects'][0]
                call('ui.click', position=[r[0]+r[2]/2, r[1]+r[3]/2]); wait()
                selected = annotations()['selected']; assert selected['kind'] == kind, selected
                return item(kind, page)

            def corner(name):
                return next(h['position'] for h in annotations()['resize_handles']['corners'] if h['corner'] == name)

            def pointer(action, position):
                call('ui.pointer', action=action, position=position); wait()

            def capture(name):
                path = (args.output/f'{name}.png').resolve(); call('ui.screenshot', path=str(path))
                return Image.open(path).convert('RGB')

            wait(); command('view.zoom', zoom=3.)
            old = select('Square'); start = corner('BottomRight'); end = [start[0]+48,start[1]+36]
            original = capture('selected-before')
            pointer('down', start); pointer('move', end)
            assert annotations()['dragging'] and not state()['forms']['dirty']
            close(item('Square')['rect'], old['rect'])
            preview = annotations()['move_preview']['rect']
            close(preview, [old['rect'][0],old['rect'][1],old['rect'][2]+.08,old['rect'][3]+.06])
            assert capture('preview').tobytes() != original.tobytes()
            command('document.save', path=str((args.output/'preview-save.pdf').resolve()))
            assert annotations()['dragging'] and not state()['forms']['dirty']
            pointer('up', end)
            close(item('Square')['rect'], preview)
            assert state()['forms']['dirty'] and not annotations()['moving']
            capture('resized-square')
            command('document.save', path=str((args.output/'resized-square.pdf').resolve()))
            # Escape cancels; pulling through the opposite corner never inverts.
            old = item('Square'); start = corner('TopLeft'); end = [start[0]+30,start[1]+25]
            pointer('down', start); pointer('move', end); assert annotations()['dragging']
            call('ui.key', key='Escape'); wait(); pointer('up', end)
            close(item('Square')['rect'], old['rect']); assert not state()['forms']['dirty']
            # All three kinds, all view rotations, all intrinsic rotations/crops.
            for page in [1,2,3,4]:
                command('view.page', page=page)
                for rotation in range(4 if page == 1 else 1):
                    for kind in ['FreeText','Square','Circle']:
                        old = select(kind,page); start = corner('BottomRight')
                        delta = [(18,12),(-12,18),(-18,-12),(12,-18)][rotation]
                        end = [start[0]+delta[0],start[1]+delta[1]]
                        call('ui.drag', **{'from':start,'to':end,'steps':4}); wait()
                        close(item(kind,page)['rect'],[old['rect'][0],old['rect'][1],min(old['rect'][2]+.03,1.-old['rect'][0]),min(old['rect'][3]+.02,1.-old['rect'][1])])
                        assert annotations()['selected']['kind'] == kind
                    if page == 1: command('view.rotate')
            command('view.page',page=1)
            for kind in ['Text','Highlight']:
                select(kind); assert annotations()['resize_handles'] is None
            # Save/reopen actual geometry; overlays must not appear in saved AP.
            expected = {kind:item(kind)['rect'] for kind in ['FreeText','Square','Circle']}
            final = (args.output/'resized-all.pdf').resolve()
            command('document.save', path=str(final)); call('ui.open',path=str(final)); wait()
            command('view.zoom',zoom=3.)
            for kind,r in expected.items(): close(item(kind)['rect'],r)
            select('Circle'); capture('reopened')
            # Eight logical pixel minimum; fixed top-left, no flip under pointer.
            old=item('Circle'); start=corner('BottomRight'); fixed=corner('TopLeft')
            end=[fixed[0]-30,fixed[1]-30]
            call('ui.drag', **{'from':start,'to':end,'steps':4});wait()
            r=item('Circle')['rect'];close(r[:2],old['rect'][:2]);close(r[2:],[8/600,8/600])
            assert hashlib.sha256(source.read_bytes()).digest() == source_hash
            call('ui.quit');wait();call('ui.command',id='document.discard');assert app.wait(timeout=15)==0
            print(f'Annotation resize smoke passed: {len(report)} requests; artifacts in {args.output}')
        finally:
            (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
            if app.poll() is None: app.terminate(); app.wait(timeout=10)


if __name__ == '__main__':
    main()
