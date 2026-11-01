#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Verify that unsaved-change confirmation stays visible over an animated PDF (Pillow)."""
import argparse,hashlib,json,socket,subprocess,time
from pathlib import Path
from PIL import Image,ImageChops
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=ROOT/'target/release/pdfoxium-viewer')
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--action',choices=['save','discard'],default='save')
a=parser.parse_args();a.output.mkdir(parents=True,exist_ok=True)
assets=ROOT/'crates/pdfoxium-viewer/assets/demos';hashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in assets.glob('*.pdf')}
ep=a.output/'control.json';assert not ep.exists();report=[]
with (a.output/'app.log').open('w') as log:
    app=subprocess.Popen([str(a.binary),str(assets/'animation-demo.pdf'),'--control',str(ep.resolve()),'--control-only-input'],stdout=log,stderr=log)
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
        wait();command('view.zoom',zoom=1)
        command('view.annotations');command('annotation.note',text='Unsaved animation note',x=.85,y=.8)
        command('view.annotations');assert state()['forms']['dirty']
        click_field('animation');time.sleep(.2)
        def prompt():return bool(call('ui.inspect',query='document.cancel')['widgets'])
        def confirm_close():
            call('ui.quit')
            deadline=time.monotonic()+5
            while not prompt():
                assert time.monotonic()<deadline,'close confirmation never appeared'
                time.sleep(.01)
            missing=0
            for _ in range(60):
                missing+=not prompt();time.sleep(.01)
            report.append({'missing_dialog_frames':missing})
            assert missing==0,f'confirmation vanished in {missing} observations'
        confirm_close();call('ui.click',id='document.cancel');wait()
        assert call('ui.state')['departure'] is None and state()['forms']['dirty']
        confirm_close()
        if a.action=='save':
            call('ui.command',id='document.save',path=str((a.output/'animation-note.pdf').resolve()))
        else:call('ui.click',id='document.discard')
        assert app.wait(timeout=15)==0
        assert hashlib.sha256((assets/'animation-demo.pdf').read_bytes()).hexdigest()==hashes['animation-demo.pdf']
        print(f'Animated close/{a.action} passed: {len(report)} requests; artifacts in {a.output}')
    finally:
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
