#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Verify the embedded demo launcher and annotation-driven wave animation (Pillow)."""
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
    app=subprocess.Popen([str(a.binary.resolve()),'--control',str(ep.resolve()),'--control-only-input'],cwd=a.output.resolve(),stdout=log,stderr=log)
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
            path=(a.output/f'{name}.png').resolve()
            for _ in range(10):
                r=raw('ui.screenshot',path=str(path))
                if 'error' not in r:return Image.open(path).convert('RGB')
                assert r['error']['message']=='view changed before screenshot completed',r
            raise AssertionError('no stable screenshot between frames')
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
        wait();assert call('ui.state')['demo_open']
        capture('launcher')
        for sample in ['editing','acroform','xfa','dynamic-xfa','animation']:
            call('ui.command',id='demo.open');wait()
            call('ui.click',id='demo.'+sample);wait()
            assert state()['xfa']==(sample in ['xfa','dynamic-xfa'])
        command('view.zoom',zoom=1)
        def visible_frame():
            return [f['name'] for f in fields() if f['name'].startswith('frame') and f['visible']]
        assert visible_frame()==['frame00']
        initial=capture('animation-initial')
        r=field('animation')['screen_rect'];scale=initial.width/call('ui.state')['window']['width'];box=tuple(round(v*scale) for v in [r[0]+2,r[1]+2,r[0]+r[2]-2,r[1]+r[3]-2])
        click_field('animation');time.sleep(.25);click_field('toggle')
        first=capture('animation-frame-a')
        click_field('animation');time.sleep(.25);click_field('toggle');second=capture('animation-frame-b')
        difference=ImageChops.difference(first.crop(box),second.crop(box))
        assert difference.getbbox()
        report.append({'different_pixels':sum(any(p) for p in difference.getdata()),'maxnorm':max(v[1] for v in difference.getextrema())})
        paused=capture('animation-paused');time.sleep(.35)
        assert paused.crop(box).tobytes()==capture('animation-still-paused').crop(box).tobytes()
        click_field('animation');command('view.page',page=2)
        assert state()['visible_pages']==[2]
        count=state()['pipeline']['results'];time.sleep(.35);wait();assert state()['pipeline']['results']==count
        capture('animation-offscreen');command('view.page',page=1)
        assert visible_frame()==['frame00'] and not state()['forms']['dirty']
        capture('animation-returned')
        call('ui.quit');assert app.wait(timeout=15)==0
        print(f'Demo launcher/animation passed: {len(report)} requests; artifacts in {a.output}')
    finally:
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
