#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise on-page annotation picking, comment focus, outlines and Delete (Pillow)."""
import argparse
import json
import importlib.util
import sys
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image
sys.dont_write_bytecode=True
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
# The link overlaps part of the first line's later highlight. Links must win.
objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 260] /CropBox [10 20 290 240] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R /Annots [8 0 R] >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 260] /CropBox [10 20 290 240] /Rotate 90 /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
for content in ['BT /F1 12 Tf 30 160 Td (Hello banana BANANA) Tj 0 -25 Td (Second line text) Tj ET','BT /F1 12 Tf 30 160 Td (Next page) Tj ET']:
    objects.append(f'<< /Length {len(content)} >>\nstream\n{content}\nendstream')
objects.append('<< /Type /Annot /Subtype /Link /Rect [65 155 80 170] /Dest [4 0 R /Fit] >>')
source='%PDF-1.7\n';offsets=[]
for i,obj in enumerate(objects):offsets.append(len(source));source+=f'{i+1} 0 obj\n{obj}\nendobj\n'
xref=len(source);source+=f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'+''.join(f'{offset:010} 00000 n \n' for offset in offsets)
source+=f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
pdf=(args.output/'source.pdf').resolve();pdf.write_text(source)
endpoint_path=args.output/'control.json';assert not endpoint_path.exists();report=[]
with (args.output/'app.log').open('w') as log:
    app=subprocess.Popen([str(args.binary),str(pdf),'--control',str(endpoint_path.resolve()),'--control-only-input'],stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+30
        while not endpoint_path.exists():
            assert app.poll() is None and time.monotonic()<deadline;time.sleep(.02)
        endpoint=json.loads(endpoint_path.read_text())
        def call(method,params=None):
            with socket.create_connection(('127.0.0.1',endpoint['port']),timeout=40) as connection:
                connection.sendall((json.dumps({'jsonrpc':'2.0','id':len(report),'token':endpoint['token'],'method':method,'params':params or {}})+'\n').encode())
                response=json.loads(connection.makefile('rb').readline())
            report.append({'method':method,'params':params,'response':response});assert 'error' not in response,response
            return response['result']
        def wait():
            state=call('ui.wait');assert state['settled'],state;return state
        def command(id,**params):call('ui.command',{'id':id,**params});return wait()
        def state():return call('ui.state')['active']
        def snapshot():return state()['annotations']
        def capture(name):
            path=(args.output/f'{name}.png').resolve();call('ui.screenshot',{'path':str(path)});return Image.open(path).convert('RGB')
        def center(rect):return [rect[0]+rect[2]/2,rect[1]+rect[3]/2]
        def hit(page,kind):
            return next(h for p in snapshot()['visible'] if p['page']==page for h in p['items'] if h['kind']==kind)
        def click(position):call('ui.click',{'position':position});return wait()
        def blank(page):
            rect=next(p['rect'] for p in state()['pages_on_screen'] if p['page']==page)
            return [rect[0]+rect[2]*.9,rect[1]+rect[3]*.1]
        wait();command('view.zoom',zoom=2.5)
        command('view.annotations')
        command('selection.set',page=1,start=6,end=37)
        command('annotation.markup',kind='highlight')
        command('selection.clear')
        command('annotation.comment',index=1,text='Two-line comment')
        command('annotation.note',text='First note',x=.75,y=.35)
        command('annotation.note',text='Second note',x=.75,y=.65)
        command('view.page',page=2)
        command('annotation.note',text='Rotated note',x=.65,y=.4)
        saved=(args.output/'existing-annotations.pdf').resolve()
        command('document.save',path=str(saved));call('ui.open',{'path':str(saved)});wait()
        command('view.zoom',zoom=2.5)
        assert not state()['forms']['enabled'] and not snapshot()['open']
        highlight=hit(1,'Highlight');regions=highlight['screen_rects'];assert len(regions)==2
        # Click the second line, clear of the link on the first line.
        click(center(regions[1]))
        selected=snapshot()['selected']
        assert selected and selected['page']==1 and selected['contents']=='Two-line comment',snapshot()
        assert snapshot()['open'] and state()['forms']['enabled']
        selected_image=capture('selected-highlight')
        selected_rects=selected['screen_rects']
        click(blank(1));assert snapshot()['selected'] is None
        plain=capture('unselected-highlight')
        scale=selected_image.width/call('ui.state')['window']['width']
        for x,y,w,h in selected_rects:
            box=(round((x-5)*scale),round((y-5)*scale),round((x+w+5)*scale),round((y+h+5)*scale))
            a=selected_image.crop(box);b=plain.crop(box)
            accent=sum(p!=q and p[1]>p[0]+20 and p[2]>p[0]+10 and p[1]>p[2] for p,q in zip(a.get_flattened_data(),b.get_flattened_data()))
            assert accent>20,accent
        # The empty space inside the annotation's bounding box is not a hit.
        regions=hit(1,'Highlight')['screen_rects']
        gap=[regions[0][0]+regions[0][2]/2,(regions[0][1]+regions[0][3]+regions[1][1])/2]
        click(gap);assert snapshot()['selected'] is None
        click(center(regions[1]));assert snapshot()['selected']
        call('ui.click',{'id':'annotation.comment'});wait()
        call('ui.key',{'key':'A','modifiers':['command']});call('ui.key',{'key':'Backspace'});wait()
        assert snapshot()['selected'] is not None and len(call('ui.annotations')['items'])==3
        call('ui.type',{'text':'Changed by page click'});wait()
        call('ui.click',{'id':'annotation.apply_comment'});wait()
        assert snapshot()['selected']['contents']=='Changed by page click'
        command('document.save',path=str((args.output/'after-comment.pdf').resolve()))
        # Dragging across marked text selects/copies text rather than picking it.
        command('selection.set',page=1,start=6,end=7);first=center(call('ui.selection')['pages'][0]['screen_rects'][0])
        command('selection.set',page=1,start=11,end=12);last=center(call('ui.selection')['pages'][0]['screen_rects'][0])
        command('selection.clear');call('ui.drag',{'from':first,'to':last});wait()
        assert call('ui.selection')['text']=='banana' and snapshot()['selected'] is None
        command('selection.clear')
        links=call('ui.links',{'page':1});wait();links=call('ui.links',{'page':1})
        link=links['links'][0]
        click(center(link['screen_rects'][0]));assert state()['page']==2 and snapshot()['selected'] is None
        click(center(hit(2,'Text')['screen_rects'][0]));assert snapshot()['selected']['page']==2
        capture('selected-rotated-note')
        call('ui.key',{'key':'Escape'});wait();assert snapshot()['selected'] is None
        command('view.page',page=1);click(blank(1))
        # Select the last note, remove an earlier entry, and retain identity.
        notes=[h for p in snapshot()['visible'] if p['page']==1 for h in p['items'] if h['kind']=='Text']
        click(center(notes[-1]['screen_rects'][0]));assert snapshot()['selected']['contents']=='Second note'
        command('annotation.delete',index=1)  # Link is array index 0; highlight was 1.
        assert snapshot()['selected']['contents']=='Second note' and snapshot()['selected']['index']==2
        call('ui.key',{'key':'Delete'});wait();assert snapshot()['selected'] is None
        assert len(call('ui.annotations')['items'])==1
        click(center(hit(1,'Text')['screen_rects'][0]))
        call('ui.click',{'id':'annotation.selected_row'});wait()
        call('ui.key',{'key':'Backspace'});wait()
        assert call('ui.annotations')['items']==[]
        command('document.save',path=str((args.output/'after-deletion.pdf').resolve()))
        spec=importlib.util.spec_from_file_location('forms_fixture',root/'scripts/smoke-viewer-forms.py')
        forms_fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(forms_fixture)
        form_pdf=(args.output/'forms.pdf').resolve();forms_fixture.form_pdf(form_pdf)
        call('ui.open',{'path':str(form_pdf)});wait();command('view.zoom',zoom=1.5)
        command('annotation.note',text='Keep while editing form',x=.9,y=.9)
        command('annotation.note',text='Overlapping form note',x=.23,y=.11)
        click(center(hit(1,'Text')['screen_rects'][0]));assert snapshot()['selected']
        name=next(f for f in call('ui.forms')['fields'] if f['name']=='name')
        click(center(name['screen_rect']))
        assert call('ui.forms')['focus']['name']=='name' and snapshot()['selected'] is None
        call('ui.key',{'key':'A','modifiers':['command']});call('ui.key',{'key':'Backspace'});wait()
        assert call('ui.forms')['text']=='' and len(call('ui.annotations')['items'])==2
        call('ui.type',{'text':'Field survives annotation picking'});wait()
        assert call('ui.forms')['text']=='Field survives annotation picking'
        call('ui.quit');wait();call('ui.command',{'id':'document.discard'});assert app.wait(timeout=15)==0
        print(f'Annotation interaction smoke passed: {len(report)} requests; screenshots in {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
