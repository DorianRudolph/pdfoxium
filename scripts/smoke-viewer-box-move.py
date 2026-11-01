#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Move selected annotation boxes by handle/body, retaining corner resizing (Pillow)."""
import argparse
import importlib.util
import json
from pathlib import Path
import socket
import subprocess
import sys
import time
from PIL import Image,ImageChops
sys.dont_write_bytecode=True
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--render-binary',type=Path,help='optional pdfoxium-diff binary for native/Rust comparison')
args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
spec=importlib.util.spec_from_file_location('resize_fixture',root/'scripts/smoke-viewer-annotation-resize.py')
fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)
source=(args.output/'source.pdf').resolve();fixture.fixture(source)
endpoint=args.output/'control.json';assert not endpoint.exists();report=[]
with (args.output/'app.log').open('w') as log:
    app=subprocess.Popen([str(args.binary),str(source),'--control',str(endpoint.resolve()),'--control-only-input'],stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+30
        while not endpoint.exists():
            assert app.poll() is None and time.monotonic()<deadline;time.sleep(.02)
        e=json.loads(endpoint.read_text())
        def call(method,**params):
            with socket.create_connection(('127.0.0.1',e['port']),timeout=40) as s:
                s.sendall((json.dumps(dict(jsonrpc='2.0',id=len(report),token=e['token'],method=method,params=params))+'\n').encode())
                response=json.loads(s.makefile('rb').readline())
            report.append(dict(method=method,params=params,response=response));assert 'error' not in response,response
            return response['result']
        def wait():assert call('ui.wait')['settled']
        def command(id,**params):call('ui.command',id=id,**params);wait()
        def state():return call('ui.state')['active']
        def annotations():
            a=state()['annotations'];assert not a['error'],a;return a
        def item(kind,page=1):return next(h for p in annotations()['visible'] if p['page']==page for h in p['items'] if h['kind']==kind)
        def center(r):return [r[0]+r[2]/2,r[1]+r[3]/2]
        def select(kind,page=1):
            call('ui.click',position=center(item(kind,page)['screen_rects'][0]));wait()
            assert annotations()['selected']['kind']==kind
            return item(kind,page)
        def close(a,b):assert max(abs(x-y) for x,y in zip(a,b))<1e-4,(a,b)
        def pointer(action,p):call('ui.pointer',action=action,position=p);wait()
        def capture(name):call('ui.screenshot',path=str((args.output/f'{name}.png').resolve()))
        def save(name):command('document.save',path=str((args.output/f'{name}.pdf').resolve()))
        wait();command('view.zoom',zoom=3.)
        old=select('Square');save('before');capture('selected')
        start=center(annotations()['move_handle']['rect']);end=[start[0]+36,start[1]+24]
        pointer('down',start);pointer('move',end)
        assert annotations()['dragging'] and not state()['forms']['dirty']
        close(item('Square')['rect'],old['rect']);capture('preview')
        pointer('up',end)
        moved=item('Square');close(moved['rect'],[old['rect'][0]+.06,old['rect'][1]+.04,*old['rect'][2:]])
        assert annotations()['selected']['kind']=='Square' and state()['forms']['dirty']
        save('moved');capture('moved')
        # All types through both body and move handle at all view rotations,
        # plus intrinsic rotations/crops on the remaining fixture pages.
        for page in [1,2,3,4]:
            command('view.page',page=page)
            for rotation in range(4 if page==1 else 1):
                for kind in ['FreeText','Square','Circle']:
                    old=select(kind,page)
                    handle=annotations()['move_handle']
                    start=center(handle['rect']) if kind=='Square' and handle else center(old['screen_rects'][0])
                    delta=[(12,9),(-9,12),(-12,-9),(9,-12)][rotation]
                    end=[start[0]+delta[0],start[1]+delta[1]]
                    call('ui.drag',**{'from':start,'to':end,'steps':4});wait()
                    close(item(kind,page)['rect'],[min(old['rect'][0]+.02,1-old['rect'][2]),min(old['rect'][1]+.015,1-old['rect'][3]),*old['rect'][2:]])
                    assert annotations()['selected']['kind']==kind
                if page==1:command('view.rotate')
        command('view.page',page=1);old=select('FreeText')
        start=center(old['screen_rects'][0]);end=[start[0]+24,start[1]+18]
        pointer('down',start);pointer('move',end);call('ui.key',key='Escape');wait();pointer('up',end)
        close(item('FreeText')['rect'],old['rect'])
        # Corner handles still resize instead of translating the box.
        old=select('Circle');start=next(c['position'] for c in annotations()['resize_handles']['corners'] if c['corner']=='BottomRight')
        call('ui.drag',**{'from':start,'to':[start[0]+18,start[1]+12],'steps':4});wait()
        close(item('Circle')['rect'],[*old['rect'][:2],old['rect'][2]+.03,old['rect'][3]+.02])
        expected={kind:item(kind)['rect'] for kind in ['FreeText','Square','Circle']}
        save('final');call('ui.open',path=str((args.output/'final.pdf').resolve()));wait();command('view.zoom',zoom=3.)
        for kind,rect in expected.items():close(item(kind)['rect'],rect)
        call('ui.quit');assert app.wait(timeout=15)==0
        if args.render_binary:
            comparisons={}
            for backend in ['native','rust']:
                images={}
                for name in ['before','moved']:
                    directory=root/'target/render-diff'/backend/'viewer-box-move'/name/'dpi-144'
                    directory.mkdir(parents=True,exist_ok=True)
                    case=dict(id=f'viewer-box-move/{name}',path=str((args.output/f'{name}.pdf').resolve()),all_pages=True,dpi=144,flags=1)
                    subprocess.run([str(args.render_binary),'_render',backend,json.dumps(case)],check=True,stdout=subprocess.DEVNULL)
                    images[name]=[Image.open(directory/f'{page:04}.png').convert('RGB') for page in range(4)]
                a,b=images['before'][0],images['moved'][0]
                difference=ImageChops.difference(a,b);assert difference.getbbox()==(240,60,384,156),difference.getbbox()
                assert ImageChops.difference(a.crop((240,60,360,140)),b.crop((264,76,384,156))).getbbox() is None
                assert all(p==(255,255,255) for p in b.crop((240,60,262,140)).get_flattened_data())
                for page in range(1,4):assert ImageChops.difference(images['before'][page],images['moved'][page]).getbbox() is None
                comparisons[backend]=dict(changed_pixels=sum(p!=q for p,q in zip(a.get_flattened_data(),b.get_flattened_data())),bounds=difference.getbbox())
            (args.output/'render-comparison.json').write_text(json.dumps(comparisons,indent=2)+'\n')
        print(f'Box move smoke passed: {len(report)} requests; artifacts in {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
