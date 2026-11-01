#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Live annotation edit/save/reopen and visible-pixel checks (Pillow)."""
import argparse
import hashlib
import json
from pathlib import Path
from collections import Counter
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
def fragmented_fixture(path):
    # Alternate faces within 'begone', with ascender/descender/x-height runs.
    # Old per-object markup visibly steps at each run.
    objects = ['<< /Type /Catalog /Pages 2 0 R >>', '',
        '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
        '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>']
    kids = []
    for index in range(5):
        page = len(objects)+1
        kids.append(f'{page} 0 R')
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 260] /CropBox [10 20 210 220] /Rotate {index%4*90} /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {page+1} 0 R >>')
        content = 'BT /F1 12 Tf 30 130 Td (Hello be) Tj /F2 12 Tf (go) Tj /F1 12 Tf (ne BANANA) Tj ET'
        if index == 4:
            content = 'BT /F1 12 Tf 30 130 Td (Hello first line) Tj 0 -25 Td (Second line text) Tj 0 -25 Td (Third line) Tj ET'
        objects.append(f'<< /Length {len(content)} >>\nstream\n{content}\nendstream')
    objects[1] = '<< /Type /Pages /Kids ['+' '.join(kids)+'] /Count 5 >>'
    pdf = '%PDF-1.7\n'
    offsets = []
    for index,value in enumerate(objects):
        offsets.append(len(pdf)); pdf += f'{index+1} 0 obj\n{value}\nendobj\n'
    xref = len(pdf)
    pdf += f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'+''.join(f'{offset:010} 00000 n \n' for offset in offsets)
    pdf += f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
    path.write_bytes(pdf.encode())
pdf = (args.output/'annotations-source.pdf').resolve()
fragmented_fixture(pdf)
original_hash = hashlib.sha256(pdf.read_bytes()).hexdigest()
endpoint_path = args.output/'control.json'
assert not endpoint_path.exists()
report = []
with (args.output/'app.log').open('w') as log:
    app = subprocess.Popen([str(args.binary),str(pdf),'--control',str(endpoint_path.resolve()),'--control-only-input'],stdout=log,stderr=log)
    try:
        deadline = time.monotonic()+30
        while not endpoint_path.exists():
            assert app.poll() is None and time.monotonic()<deadline
            time.sleep(.02)
        endpoint = json.loads(endpoint_path.read_text())
        def call(method, params=None):
            with socket.create_connection(('127.0.0.1',endpoint['port']),timeout=40) as connection:
                connection.sendall((json.dumps({'jsonrpc':'2.0','id':len(report),'token':endpoint['token'],'method':method,'params':params or {}})+'\n').encode())
                response = json.loads(connection.makefile('rb').readline())
            report.append({'method':method,'params':params,'response':response})
            assert 'error' not in response,response
            return response['result']
        def wait():
            state = call('ui.wait')
            assert state['settled'] and state['active']['render_complete'],state
            return state
        def command(id, **params):
            call('ui.command',{'id':id,**params})
            return wait()
        def capture(name):
            path = (args.output/f'{name}.png').resolve()
            call('ui.screenshot',{'path':str(path)})
            return Image.open(path).convert('RGB')
        def crop_page(image, state):
            active = state['active']
            page = next(p for p in active['pages_on_screen'] if p['page']==active['page'])
            x,y,w,h = page['rect']
            scale = image.width/state['window']['width']
            vx,vy,vw,vh = active['viewport']
            return image.crop((int(max(x,vx)*scale),int(max(y,vy)*scale),int(min(x+w,vx+vw)*scale),int(min(y+h,vy+vh)*scale)))
        def colored_points(image, kind):
            return [(i % image.width, i // image.width) for i,(r,g,b) in enumerate(image.get_flattened_data())
                    if ((r>180 and g>150 and b<100) if kind=='highlight' else (r>150 and g<100 and b<100))]
        def verify_band(image, kind):
            points = colored_points(image,kind)
            assert len(points)>20,(kind,len(points))
            vertical = kind=='underline'  # Page 2 has an intrinsic 90-degree rotation.
            primary = [p[1 if vertical else 0] for p in points]
            rows = Counter(p[0 if vertical else 1] for p in points)
            span = max(primary)-min(primary)+1
            if kind=='highlight':
                assert min(rows[min(rows)],rows[max(rows)])/span>.95,('uneven highlight band',rows,span)
            else:
                assert max(rows.values())/span>.98,('disjoint markup',kind,rows,span)
        wait()
        call('ui.click',{'id':'view.annotations'})
        wait()
        assert call('ui.annotations')['items']==[]
        for page,kind in [(1,'highlight'),(2,'underline'),(3,'strikeout')]:
            command('view.page',page=page)
            # Hold layout constant while comparing the annotated page pixels.
            state = command('view.zoom',zoom=2.0)
            plain = crop_page(capture(f'{page}-plain'),state)
            command('selection.set',page=page,start=0,end=21 if page==3 else 19)
            command('annotation.markup',kind=kind)
            state = command('selection.clear')
            annotated = crop_page(capture(f'{page}-{kind}'),state)
            assert plain.size == annotated.size
            changed = sum(a!=b for a,b in zip(plain.get_flattened_data(),annotated.get_flattened_data()))
            assert changed>20,(page,kind,changed)
            verify_band(annotated,kind)
            items = call('ui.annotations')['items']
            assert len(items)==1 and items[0]['kind'].lower()==kind,items
            command('annotation.comment',index=items[0]['index'],text=f'{kind} comment é🙂')
            assert call('ui.annotations')['items'][0]['contents']==f'{kind} comment é🙂'
        command('view.page',page=5)
        command('selection.set',page=5,start=6,end=41)
        command('annotation.markup',kind='underline')
        state = command('selection.clear')
        multiline = crop_page(capture('multiline-underline'),state)
        assert len(call('ui.annotations')['items'])==1  # One annotation, three lines.
        points = colored_points(multiline,'underline')
        bands = []
        for y in sorted(set(y for x,y in points)):
            if not bands or y>bands[-1][-1]+1: bands.append([])
            bands[-1].append(y)
        assert len(bands)==3,bands
        widths = []
        for band in bands:
            xs = [x for x,y in points if y in band]
            rows = Counter(y for x,y in points if y in band)
            width = max(xs)-min(xs)+1
            assert max(rows.values())/width>.98,(band,rows,width)
            widths.append(width)
        assert widths[0]<widths[1] and widths[2]<widths[0],widths
        command('view.page',page=3)
        state = command('annotation.place_note',text='Rotated note')
        rect = next(p['rect'] for p in state['active']['pages_on_screen'] if p['page']==3)
        call('ui.click',{'position':[rect[0]+rect[2]*.5,rect[1]+rect[3]*.5]})
        wait()
        items = call('ui.annotations')['items']
        assert len(items)==2 and items[-1]['kind']=='Text',items
        command('annotation.delete',index=items[-1]['index'])
        assert len(call('ui.annotations')['items'])==1
        command('view.page',page=1)
        command('annotation.note',text='Saved note',x=.5,y=.5)
        state = wait()
        assert state['active']['forms']['dirty'],state
        saved = (args.output/'annotations-saved.pdf').resolve()
        command('document.save',path=str(saved))
        assert not call('ui.forms')['dirty']
        call('ui.open',{'path':str(saved)})
        wait()
        call('ui.annotations');wait()
        items = call('ui.annotations')['items']
        assert len(items)==2 and items[0]['contents']=='highlight comment é🙂' and items[1]['contents']=='Saved note',items
        capture('saved-reopened')
        # Deleting an earlier array entry must preserve the other annotation.
        command('annotation.delete',index=items[0]['index'])
        assert call('ui.annotations')['items'][0]['contents']=='Saved note'
        call('ui.quit');wait()
        command('document.cancel')
        assert call('ui.forms')['dirty']
        call('ui.quit');wait()
        call('ui.command',{'id':'document.discard'})
        assert app.wait(timeout=15)==0
        assert hashlib.sha256(pdf.read_bytes()).hexdigest()==original_hash
        print(f'Annotation smoke passed: {len(report)} requests; saved PDF and screenshots in {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:
            app.terminate();app.wait(timeout=10)
