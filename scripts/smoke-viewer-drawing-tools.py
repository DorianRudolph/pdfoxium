#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Create/edit shapes and free text through the viewer tools (requires Pillow)."""
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


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root/'target/release/pdfoxium-viewer')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location('text_fixture', root/'scripts/smoke-viewer-text.py')
    fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
    source = (args.output/'source.pdf').resolve(); fixture.fixture(source)
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

            def click(id):
                call('ui.click', id=id); wait()

            def point(page, unit, rotation=0):
                x,y = unit
                x,y = [(x,y),(1-y,x),(1-x,1-y),(y,1-x)][rotation]
                r = next(p['rect'] for p in state()['pages_on_screen'] if p['page'] == page)
                return [r[0]+r[2]*x, r[1]+r[3]*y]

            def pointer(action, position):
                call('ui.pointer', action=action, position=position); wait()

            def close(a, b):
                assert max(abs(x-y) for x, y in zip(a,b)) < 1e-4, (a,b)

            def capture(name):
                path=(args.output/f'{name}.png').resolve(); call('ui.screenshot',path=str(path))
                return Image.open(path).convert('RGB')

            def save(name):
                path=(args.output/f'{name}.pdf').resolve();command('document.save',path=str(path));return path

            tools = [('annotation.rectangle','Square',(.55,.1),(.85,.3)),
                     ('annotation.ellipse','Circle',(.1,.6),(.4,.85)),
                     ('annotation.text','FreeText',(.1,.1),(.45,.25))]

            def draw(tool,kind,a,b,page=1,rotation=0):
                click(tool)
                assert annotations()['tool'] in ['Text','Rectangle','Ellipse']
                call('ui.drag', **{'from':point(page,a,rotation),'to':point(page,b,rotation),'steps':4});wait()
                s=annotations();assert s['tool'] is None and s['selected']['kind']==kind,s
                row=next(i for i in s['items'] if i['index']==s['selected']['index'])
                close(row['rect'],[min(a[0],b[0]),min(a[1],b[1]),abs(a[0]-b[0]),abs(a[1]-b[1])])
                assert len(s['resize_handles']['corners'])==4
                return row

            wait();command('view.zoom',zoom=3.);command('view.annotations')
            capture('source')
            tool,kind,a,b=tools[0];click(tool)
            start,end=point(1,a),point(1,b)
            pointer('down',start);pointer('move',end)
            assert annotations()['dragging'] and annotations()['items']==[] and not state()['forms']['dirty']
            capture('creation-preview');save('preview-save')
            pointer('up',end)
            assert annotations()['selected']['kind']=='Square' and state()['forms']['dirty']
            for args2 in tools[1:]:draw(*args2)
            assert len(annotations()['items'])==3
            capture('created');save('created')
            # New text's placeholder is focused and selected, so typing replaces it.
            call('ui.type',text='Hello editor');wait()
            assert annotations()['selected']['contents']=='Hello editor',annotations()
            click('annotation.apply_comment')
            edited=save('text-edited');capture('text-edited')
            call('ui.open',path=str(edited));wait();command('view.zoom',zoom=3.)
            call('ui.annotations');wait();rows=annotations()['items']
            assert next(r for r in rows if r['kind']=='FreeText')['contents']=='Hello editor'
            assert len(rows)==3 and not state()['forms']['dirty']
            # Cancellation and undersized drags do not create or dirty anything.
            command('search.open')  # Active drawing takes Escape before Find.
            click('annotation.rectangle');start=point(1,(.5,.5));end=point(1,(.8,.8))
            pointer('down',start);pointer('move',end);assert annotations()['dragging']
            call('ui.key',key='Escape');wait();pointer('up',end)
            assert annotations()['tool'] is None and len(annotations()['items'])==3 and not state()['forms']['dirty']
            command('search.clear')
            click('annotation.ellipse')
            call('ui.drag',**{'from':start,'to':[start[0]+7,start[1]+7],'steps':2});wait()
            assert annotations()['tool']=='Ellipse' and len(annotations()['items'])==3 and not state()['forms']['dirty']
            call('ui.key',key='Escape');wait()
            # All view rotations and intrinsic crop/rotations; reverse drags too.
            for page in [1,2,3,4]:
                command('view.page',page=page)
                for rotation in range(4 if page==1 else 1):
                    for tool,kind,a,b in tools: draw(tool,kind,b,a,page,rotation)
                    if page==1:command('view.rotate')
            # Created objects use the same delete path and remain selected by ID.
            assert annotations()['selected']['kind']=='FreeText'
            count=len(annotations()['items']);call('ui.key',key='Escape');wait()
            # Escape exits focused text/selection; select the last ellipse by sidebar
            # first via page hit, then delete with canvas focus.
            hit=next(h for p in annotations()['visible'] if p['page']==4 for h in p['items'] if h['kind']=='Circle')
            r=hit['screen_rects'][0];call('ui.click',position=[r[0]+r[2]/2,r[1]+r[3]/2]);wait()
            assert annotations()['selected']['kind']=='Circle'
            call('ui.key',key='Delete');wait();assert len(annotations()['items'])==count-1
            save('rotations')
            assert hashlib.sha256(source.read_bytes()).digest()==source_hash
            call('ui.quit');assert app.wait(timeout=15)==0
            print(f'Drawing tools smoke passed: {len(report)} requests; artifacts in {args.output}')
        finally:
            (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
            if app.poll() is None:app.terminate();app.wait(timeout=10)


if __name__=='__main__':main()
