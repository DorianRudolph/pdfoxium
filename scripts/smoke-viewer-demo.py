#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Repeatable page/content, AcroForm and interactive XFA demo walkthrough (Pillow)."""
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
    app=subprocess.Popen([str(a.binary.resolve()),str(assets/'editing-demo.pdf'),'--control',str(ep.resolve()),'--control-only-input'],cwd=a.output.resolve(),stdout=log,stderr=log)
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
        wait();command('view.zoom',zoom=.8);capture('annotation-demo')
        command('view.annotations');annots=call('ui.annotations')
        assert {x['kind'] for x in annots['items']}=={'Highlight','Underline','StrikeOut','Text','FreeText','Square','Circle'}
        text=next(x for x in annots['items'] if x['kind']=='FreeText')
        hit=next(x for p in annots['visible'] for x in p['items'] if x['kind']=='FreeText')
        r=hit['screen_rects'][0];before=capture('annotations-before-edit')
        scale=before.width/call('ui.state')['window']['width'];box=tuple(round(v*scale) for v in [r[0],r[1],r[0]+r[2],r[1]+r[3]])
        command('annotation.comment',index=text['index'],text='Edited demo text.\nThis annotation remains editable.')
        assert ImageChops.difference(before.crop(box),capture('annotations-after-edit').crop(box)).getbbox()
        command('view.annotations');initial=capture('editing-source')
        command('page.image',path=str((assets/'insert-chart.png').resolve()))
        assert state()['forms']['dirty'];image=capture('inserted-image');assert ImageChops.difference(initial,image).getbbox()
        inserted=save('inserted-image');assert not state()['forms']['dirty']
        open_pdf(inserted);command('view.annotations')
        rows=call('ui.annotations')['items'];assert len(rows)==7
        assert next(x for x in rows if x['kind']=='FreeText')['contents']=='Edited demo text.\nThis annotation remains editable.'
        command('view.annotations')
        command('page.rotate');assert state()['rotation']==0
        r=state()['pages_on_screen'][0]['rect'];assert r[2]>r[3]
        save('rotated-page');capture('rotated-page')
        command('view.page',page=2);command('page.delete');assert state()['page_count']==2
        command('page.import',path=str((assets/'import-page.pdf').resolve()),pages=[1]);assert state()['page_count']==3 and 3 in state()['visible_pages']
        capture('imported-page');save('assembled')
        command('page.rotate');assert state()['forms']['dirty']
        call('ui.command',id='page.image',path=str(a.output/'missing.png'));error=raw('ui.wait');assert 'error' in error
        assert state()['forms']['dirty'] and state()['page_count']==3
        save('failure-preserved')
        # AcroForm demo and formatting, then save/reopen actual canonical values.
        open_pdf(assets/'acroform-demo.pdf');assert not state()['xfa']
        type_field('name','Grace Hopper');type_field('amount','42.5')
        click_field('approved',True);command('form.blur')
        assert field('name')['value']=='Grace Hopper' and field('approved')['checked']
        capture('acroform-filled');filled=save('acroform-filled')
        open_pdf(filled);assert field('name')['value']=='Grace Hopper' and field('approved')['checked']
        # XFA uses the actual template, layout and data binding, not PDF fallback.
        open_pdf(assets/'xfa-demo.pdf');assert state()['xfa'] and len(fields())==8
        xfa_before=capture('xfa-source');type_field('customer','Grace Hopper')
        type_field('notes','First line\nSecond line')
        click_field('approved',True);command('form.blur');assert field('approved')['checked']
        click_field('confirm');command('form.blur');assert field('status')['value']=='Confirmed'
        # The combo is real FF input; choose Express with keyboard navigation.
        click_field('delivery');call('ui.key',key='ArrowDown');call('ui.key',key='Enter');wait();command('form.blur')
        assert field('customer')['value']=='Grace Hopper' and field('delivery')['value']=='Express'
        xfa_after=capture('xfa-filled');assert ImageChops.difference(xfa_before,xfa_after).getbbox()
        saved=save('xfa-filled');open_pdf(saved)
        assert field('customer')['value']=='Grace Hopper' and field('approved')['checked']
        assert field('notes')['value']=='First line\nSecond line';capture('xfa-reopened')
        for p in assets.glob('*.pdf'):assert hashlib.sha256(p.read_bytes()).hexdigest()==hashes[p.name]
        call('ui.quit');assert app.wait(timeout=15)==0
        print(f'Demo walkthrough passed: {len(report)} requests; artifacts in {a.output}')
    finally:
        (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
