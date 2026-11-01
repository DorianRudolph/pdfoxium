#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Hefferon free-text resources and stationary sidebar-hover regression (Pillow)."""
import argparse
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image,ImageChops
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--book',type=Path,default=root/'tests/render_corpus/books/hefferon-toc.pdf')
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--render-binary',type=Path,help='optional pdfoxium-diff binary for native/Rust save comparisons')
args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
endpoint=args.output/'control.json';assert not endpoint.exists()
source_hash=hashlib.sha256(args.book.read_bytes()).digest();report=[]
with (args.output/'app.log').open('w') as log:
    app=subprocess.Popen([str(args.binary),str(args.book),'--control',str(endpoint.resolve()),'--control-only-input'],stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+30
        while not endpoint.exists():
            assert app.poll() is None and time.monotonic()<deadline;time.sleep(.02)
        e=json.loads(endpoint.read_text())
        def call(method,**params):
            with socket.create_connection(('127.0.0.1',e['port']),timeout=40) as connection:
                connection.sendall((json.dumps(dict(jsonrpc='2.0',id=len(report),token=e['token'],method=method,params=params))+'\n').encode())
                response=json.loads(connection.makefile('rb').readline())
            report.append(dict(method=method,params=params,response=response));assert 'error' not in response,response
            return response['result']
        def wait():
            result=call('ui.wait');assert result['settled'],result
        def command(id,**params):call('ui.command',id=id,**params);wait()
        def state():return call('ui.state')['active']
        def capture(name):
            path=(args.output/f'{name}.png').resolve();call('ui.screenshot',path=str(path));return Image.open(path).convert('RGB')
        wait();command('view.annotations');command('view.zoom',zoom=1.)
        call('ui.click',id='annotation.text');wait()
        r=next(p['rect'] for p in state()['pages_on_screen'] if p['page']==1)
        call('ui.drag',**{'from':[r[0]+r[2]*.15,r[1]+r[3]*.15],'to':[r[0]+r[2]*.65,r[1]+r[3]*.30],'steps':4});wait()
        assert state()['annotations']['selected']['kind']=='FreeText'
        call('ui.type',text='First line\nSecond line wraps naturally within the text box and continues on another line');wait();call('ui.click',id='annotation.apply_comment');wait()
        assert state()['annotations']['error'] is None
        capture('created-text')
        saved=(args.output/'edited.pdf').resolve();command('document.save',path=str(saved))
        call('ui.open',path=str(saved));wait();command('view.zoom',zoom=1.)
        call('ui.annotations');wait()
        assert any(i['kind']=='FreeText' and i['contents']=='First line\nSecond line wraps naturally within the text box and continues on another line' for i in state()['annotations']['items'])
        # The resize widget lies entirely inside the sidebar, adjacent to the viewport.
        handle=next(w for w in call('ui.inspect',query='annotations.resize')['widgets'] if w['id']=='annotations.resize')
        edge=handle['rect'][0]
        viewport=state()['viewport'];assert abs(viewport[0]+viewport[2]-edge)<1
        scale=capture('reopened').width/call('ui.state')['window']['width']
        crop=(round((edge-20)*scale),round(viewport[1]*scale),round((edge+4)*scale),round((viewport[1]+viewport[3])*scale))
        for dx in [-4,-1,0,1]:
            call('ui.pointer',position=[edge+dx,320]);time.sleep(.7)
            baseline=capture(f'hover-{dx}-0').crop(crop)
            for i in range(1,8):
                time.sleep(.1);image=capture(f'hover-{dx}-{i}').crop(crop)
                assert ImageChops.difference(baseline,image).getbbox() is None,(dx,i)
                assert state()['viewport']==viewport
        # The inside-only resize strip remains draggable without a document gap.
        y=viewport[1]+120
        call('ui.pointer',action='down',position=[edge+2,y]);wait()
        call('ui.pointer',action='move',position=[edge-28,y]);wait()
        call('ui.pointer',action='up',position=[edge-28,y]);wait()
        after=state()['viewport'];assert abs(after[2]-(viewport[2]-28))<2,(after,viewport)
        resized=next(w for w in call('ui.inspect',query='annotations.resize')['widgets'] if w['id']=='annotations.resize')
        assert abs(after[0]+after[2]-resized['rect'][0])<1
        assert hashlib.sha256(args.book.read_bytes()).digest()==source_hash
        call('ui.quit');assert app.wait(timeout=15)==0
        if args.render_binary:
            comparisons={}
            for backend in ['native','rust']:
                for page in [0,54]:
                    images=[]
                    for name,path in [('source',args.book.resolve()),('edited',saved)]:
                        directory=root/'target/render-diff'/backend/'viewer-text-sidebar'/name/'dpi-144'/f'page-{page}'
                        directory.mkdir(parents=True,exist_ok=True)
                        case=dict(id=f'viewer-text-sidebar/{name}',path=str(path),page=page,dpi=144,flags=1)
                        subprocess.run([str(args.render_binary),'_render',backend,json.dumps(case)],check=True,stdout=subprocess.DEVNULL)
                        images.append(Image.open(directory/f'{page:04}.png').convert('RGB'))
                    difference=ImageChops.difference(*images);bbox=difference.getbbox()
                    count=sum(p!=q for p,q in zip(images[0].get_flattened_data(),images[1].get_flattened_data()))
                    if page==54:assert bbox is None, 'existing animation page must remain unchanged'
                    else:
                        width,height=images[0].size
                        assert bbox and count>1000
                        assert bbox[0]>=width*.15-2 and bbox[1]>=height*.15-2 and bbox[2]<=width*.65+2 and bbox[3]<=height*.30+2,bbox
                        # Ignore the outer box strokes and prove visible multiline
                        # ink in the saved AP, not just a multiline Contents value.
                        inner=difference.crop((round(width*.15)+4,round(height*.15)+4,round(width*.65)-4,round(height*.30)-4)).convert('RGB')
                        active=[]
                        for y in range(inner.height):
                            row=[inner.getpixel((x,y)) for x in range(inner.width)]
                            if sum(any(p) for p in row)>2:active.append(y)
                        groups=sum(i==0 or y>active[i-1]+1 for i,y in enumerate(active))
                        assert groups>=3,(backend,groups)

                    comparisons[f'{backend}/page-{page+1}']=dict(changed_pixels=count,bounds=bbox)
            (args.output/'render-comparison.json').write_text(json.dumps(comparisons,indent=2)+'\n')
        print(f'Text/sidebar smoke passed: {len(report)} requests; 32 stable hover captures; artifacts in {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
