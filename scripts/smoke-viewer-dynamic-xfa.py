#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise dynamic XFA row insertion/removal, pagination and persistence (Pillow)."""
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
    app=subprocess.Popen([str(a.binary),str(assets/'dynamic-xfa-demo.pdf'),'--control',str(ep.resolve()),'--control-only-input'],stdout=log,stderr=log)
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
        assert state()['page_count']==1
        command('view.thumbnails');assert len(call('ui.thumbnails')['rows'])==1
        assert len([f for f in fields() if f['name']=='description'])==2
        capture('initial')
        type_field('description','Edited first row')
        click_field('add');click_field('add')
        assert state()['page_count']==2
        previews=call('ui.thumbnails');assert previews['complete'] and len(previews['rows'])==2
        assert all(r['ready'] and r['error'] is None for r in previews['rows'])
        capture('grown-page1')
        x,y,w,h=previews['rows'][1]['rect'];call('ui.click',position=[x+w/2,y+h/2]);wait()
        assert state()['page']==2
        capture('grown-page2')
        grown=save('grown');open_pdf(grown)
        assert state()['page_count']==2 and field('description')['value']=='Edited first row'
        command('thumbnails.page',page=1)
        previews=call('ui.thumbnails');x,y,w,h=previews['rows'][0]['rect']
        call('ui.click',position=[x+w/2,y+h/2]);wait();assert state()['page']==1
        click_field('remove');click_field('remove')
        assert state()['page_count']==1
        previews=call('ui.thumbnails');assert previews['complete'] and len(previews['rows'])==1
        assert previews['rows'][0]['ready'] and previews['rows'][0]['error'] is None
        assert len([f for f in fields() if f['name']=='description'])==2
        capture('shrunk');save('shrunk')
        assert hashlib.sha256((assets/'dynamic-xfa-demo.pdf').read_bytes()).hexdigest()==hashes['dynamic-xfa-demo.pdf']
        call('ui.quit');assert app.wait(timeout=15)==0
        print(f'Dynamic XFA walkthrough passed: {len(report)} requests; artifacts in {a.output}')
    finally:
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
