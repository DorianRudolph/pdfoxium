#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise existing annotation styles, creation defaults and save/reopen (Pillow)."""
import argparse,hashlib,json,socket,subprocess,time
from pathlib import Path
from PIL import Image
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=ROOT/'target/release/pdfoxium-viewer')
parser.add_argument('--output',type=Path,required=True)
a=parser.parse_args();a.output.mkdir(parents=True,exist_ok=True)
assets=ROOT/'crates/pdfoxium-viewer/assets/demos';hashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in assets.glob('*.pdf')}
ep=a.output/'control.json';assert not ep.exists();report=[]
with (a.output/'app.log').open('w') as log:
    app=subprocess.Popen([str(a.binary),str(assets/'editing-demo.pdf'),'--control',str(ep.resolve()),'--control-only-input'],stdout=log,stderr=log)
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
        wait();command('view.zoom',zoom=.8);command('view.annotations')
        def annotations():return call('ui.annotations')
        def row(kind):return next(r for r in annotations()['items'] if r['kind']==kind)
        def style(kind):return row(kind)['style']
        capture('before')
        command('annotation.style',index=row('FreeText')['index'],font_size=18,color=[110,30,170])
        command('annotation.style',index=row('Square')['index'],border_color=[10,150,60],border_width=4,fill=[255,230,120])
        command('annotation.style',index=row('Circle')['index'],border_color=[30,90,210],border_width=6,fill=[250,170,210])
        command('annotation.style',index=row('Highlight')['index'],color=[30,170,240],opacity=.25)
        assert style('FreeText')['font_size']==18 and style('FreeText')['color']==[110,30,170]
        assert style('Square')['border_width']==4 and style('Square')['fill']==[255,230,120]
        assert abs(style('Highlight')['opacity']-.25)<.004
        capture('styled');styled=save('styled')
        command('annotation.style',index=row('Circle')['index'],fill=None)
        assert style('Circle')['fill'] is None
        capture('fill-removed');cleared=save('fill-removed')
        open_pdf(styled);command('view.annotations')
        assert style('Circle')['fill']==[250,170,210] and style('FreeText')['font_size']==18
        # Select a text box and use the actual numeric input and Apply style button.
        hit=next(h for p in annotations()['visible'] for h in p['items'] if h['kind']=='FreeText')
        r=hit['screen_rects'][0];call('ui.click',position=[r[0]+r[2]/2,r[1]+r[3]/2]);wait()
        inspect=call('ui.inspect',query='annotation.style.font_size')['widgets'];assert len(inspect)==1
        call('ui.click',id='annotation.style.font_size');call('ui.key',key='A',modifiers=['command']);call('ui.type',text='22');call('ui.key',key='Enter')
        assert annotations()['selected']['style']['font_size']==22 and style('FreeText')['font_size']==18
        call('ui.click',id='annotation.apply_style');wait();assert style('FreeText')['font_size']==22
        capture('style-controls')
        # New drawing tools pick up their configured defaults.
        command('view.page',page=2)
        command('annotation.defaults',kind='text',font_size=20,color=[20,80,180])
        command('annotation.text')
        p=next(p['rect'] for p in state()['pages_on_screen'] if p['page']==2)
        def drag(x,y,w,h):call('ui.drag',**{'from':[p[0]+p[2]*x,p[1]+p[3]*y],'to':[p[0]+p[2]*(x+w),p[1]+p[3]*(y+h)]});wait()
        drag(.15,.2,.35,.15);assert style('FreeText')['font_size']==20 and style('FreeText')['color']==[20,80,180]
        command('annotation.defaults',kind='shape',border_color=[140,30,170],border_width=3,fill=[220,240,255])
        command('annotation.rectangle');drag(.6,.2,.25,.15)
        assert style('Square')['border_width']==3 and style('Square')['fill']==[220,240,255]
        command('annotation.defaults',kind='highlight',color=[220,30,160],opacity=.4)
        command('selection.set',page=2,start=0,end=6)
        command('annotation.markup',kind='highlight');command('selection.clear')
        assert style('Highlight')['color']==[220,30,160] and abs(style('Highlight')['opacity']-.4)<.004
        capture('created-with-defaults');created=save('defaults')
        open_pdf(created);command('view.page',page=2);command('view.annotations')
        assert style('FreeText')['font_size']==20 and style('Square')['fill']==[220,240,255]
        assert abs(style('Highlight')['opacity']-.4)<.004
        for f in assets.glob('*.pdf'):assert hashlib.sha256(f.read_bytes()).hexdigest()==hashes[f.name]
        call('ui.quit');assert app.wait(timeout=15)==0
        print(f'Annotation style checks passed: {len(report)} requests; artifacts in {a.output}')
    finally:
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
