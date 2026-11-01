#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise XFA popup mouse input, text selection and caret focus transfer (Pillow)."""
import argparse,hashlib,json,socket,subprocess,time
from pathlib import Path
from PIL import Image,ImageChops
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=ROOT/'target/release/pdfoxium-viewer')
parser.add_argument('--output',type=Path,required=True)
a=parser.parse_args();a.output.mkdir(parents=True,exist_ok=True)
assets=ROOT/'crates/pdfoxium-viewer/assets/demos';hashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in assets.glob('*.pdf')}
ep=a.output/'control.json';assert not ep.exists();report=[]
with (a.output/'app.log').open('w') as log:
    app=subprocess.Popen([str(a.binary),str(assets/'xfa-demo.pdf'),'--control',str(ep.resolve()),'--control-only-input'],stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+30
        while not ep.exists():assert app.poll() is None and time.monotonic()<deadline;time.sleep(.02)
        endpoint=json.loads(ep.read_text())
        def raw(method,**params):
            with socket.create_connection(('127.0.0.1',endpoint['port']),timeout=50) as s:
                s.sendall((json.dumps(dict(jsonrpc='2.0',id=len(report),token=endpoint['token'],method=method,params=params))+'\n').encode())
                r=json.loads(s.makefile('rb').readline())
            report.append(dict(method=method,params=params,response=r));return r
        def call(method,**params):
            r=raw(method,**params);assert 'error' not in r,r;return r['result']
        def wait():assert call('ui.wait')['settled']
        def command(id,**params):call('ui.command',id=id,**params);wait()
        def state():return call('ui.state')['active']
        def capture(name):
            path=(a.output/f'{name}.png').resolve();call('ui.screenshot',path=str(path));return Image.open(path).convert('RGB')
        def save(name):
            path=(a.output/f'{name}.pdf').resolve();command('document.save',path=str(path));return path
        def open_pdf(path):call('ui.open',path=str(path));wait();command('view.zoom',zoom=.8)
        def fields():return call('ui.forms')['fields']
        def field(name):return next(f for f in fields() if f['name']==name)
        def click_field(name,left=False):
            r=field(name)['screen_rect'];point=[r[0]+(8 if left else r[2]*.5),r[1]+r[3]*.5]
            call('ui.click',position=point);wait()
        def type_field(name,value):
            click_field(name);call('ui.key',key='A',modifiers=['command'])
            for index,line in enumerate(value.split('\n')):
                if index:call('ui.key',key='Enter')
                call('ui.type',text=line)
            wait();command('form.blur')
        wait();command('view.zoom',zoom=.8)
        assert state()['xfa']
        call('ui.click',id='sidebar.toggle');wait()
        previews=call('ui.thumbnails')
        assert previews['open'] and previews['complete'] and len(previews['rows'])==1
        assert all(r['ready'] and r['source']=='rendered' and r['error'] is None for r in previews['rows'])
        call('ui.click',id='view.outline');wait()
        outline=call('ui.outline');assert outline['open'] and outline['count']==0
        call('ui.click',id='view.thumbnails');wait()
        row=call('ui.thumbnails')['rows'][0]['rect'];x,y,w,h=row
        call('ui.click',position=[x+w/2,y+h/2]);wait();assert state()['page']==1
        command('thumbnails.page',page=1)
        initial=capture('initial')
        scale=initial.width/call('ui.state')['window']['width']
        thumb_box=tuple(round(v*scale) for v in [x+5,y+5,x+w-5,y+h-25])
        thumbnail=initial.crop(thumb_box)
        assert sum(max(p)-min(p)>20 or max(p)<180 for p in thumbnail.get_flattened_data())>100
        r=field('delivery')['screen_rect']
        call('ui.click',position=[r[0]+r[2]-4,r[1]+r[3]*.75]);wait()
        popup=capture('popup')
        box=(int(r[0]),int(r[1]+r[3]),int(r[0]+r[2]),int(r[1]+r[3]+26))
        assert ImageChops.difference(initial.crop(box),popup.crop(box)).getbbox()
        call('ui.click',position=[r[0]+20,r[1]+r[3]+20]);wait();command('form.blur')
        assert field('delivery')['value']=='Express'
        updated=capture('thumbnail-after-delivery')
        assert ImageChops.difference(thumbnail,updated.crop(thumb_box)).getbbox(), 'live XFA thumbnail must reflect the edited field'
        r=field('customer')['screen_rect'];y=r[1]+r[3]*.5
        call('ui.drag',**{'from':[r[0]+2,y],'to':[r[0]+100,y]});wait()
        assert call('ui.forms')['selected_text']=='Ada Lovelace'
        assert call('ui.command',id='selection.copy')['text']=='Ada Lovelace'
        capture('drag-selection')
        call('ui.click',position=[r[0]+8,y],count=2);wait()
        assert call('ui.forms')['selected_text']=='Ada'
        capture('double-click-selection')
        click_field('delivery');click_field('customer')
        r=field('delivery')['screen_rect']
        box=tuple(map(int,(r[0]+2,r[1]+2,r[0]+r[2]-2,r[1]+r[3]-2)))
        before=capture('focus-transfer').crop(box)
        for index in range(2):
            time.sleep(.65)
            assert capture(f'blink-{index}').crop(box).tobytes()==before.tobytes()
        save('filled')
        call('ui.quit');assert app.wait(timeout=15)==0
        print(f'XFA input walkthrough passed: {len(report)} requests; artifacts in {a.output}')
    finally:
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
