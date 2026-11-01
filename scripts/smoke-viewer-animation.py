#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Verify Hefferon's annotation-driven animate frames and visibility cleanup (Pillow)."""
import argparse
import json
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image, ImageChops

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--book', type=Path, default=root/'tests/render_corpus/books/hefferon-toc.pdf')
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
endpoint = args.output/'control.json'
assert not endpoint.exists()
report = []
with (args.output/'app.log').open('w') as log:
    app = subprocess.Popen([str(args.binary), str(args.book), '--control', str(endpoint.resolve()), '--control-only-input'], stdout=log, stderr=log)
    try:
        deadline=time.monotonic()+30
        while not endpoint.exists():
            assert app.poll() is None and time.monotonic()<deadline
            time.sleep(.02)
        address=json.loads(endpoint.read_text())
        def call(method,params=None):
            with socket.create_connection(('127.0.0.1',address['port']),timeout=40) as conn:
                conn.sendall((json.dumps({'jsonrpc':'2.0','id':len(report),'token':address['token'],'method':method,'params':params or {}})+'\n').encode())
                response=json.loads(conn.makefile('rb').readline())
            report.append({'method':method,'params':params or {},'response':response})
            return response
        def ok(method,params=None):
            response=call(method,params);assert 'error' not in response,response
            return response['result']
        def wait():
            state=ok('ui.wait');assert state['settled'],state
            return state
        def command(id,**params):
            ok('ui.command',{'id':id,**params});return wait()
        def forms():
            f=ok('ui.forms');assert not f['error'],f['error'];return f
        def frame():
            return [f['name'] for f in forms()['fields'] if f['name'].startswith('0.') and f['visible']]
        def capture(name):
            path=(args.output/(name+'.png')).resolve()
            # Animation can invalidate a strict revision capture between request
            # and GPU readback. Retry a fresh capture; never accept stale pixels.
            for _ in range(8):
                result=call('ui.screenshot',{'path':str(path)})
                if 'error' not in result:return Image.open(path).convert('RGB')
                assert result['error']['message']=='view changed before screenshot completed',result
            raise AssertionError('no stable capture between animation frames')
        wait();command('view.page',page=55);command('view.zoom',zoom=1.5)
        f=next(f for f in forms()['fields'] if f['name']=='anm0');x,y,w,h=f['screen_rect']
        viewport=ok('ui.state')['active']['viewport']
        ok('ui.scroll',{'dy':max(0,y-viewport[1]-100)});wait()
        f=next(f for f in forms()['fields'] if f['name']=='anm0');x,y,w,h=f['screen_rect']
        assert frame()==['0.0'],frame()
        before=capture('01-initial')
        ok('ui.click',{'position':[x+w/2,y+h/2]});wait()
        deadline=time.monotonic()+3
        while frame()==['0.0']:
            assert time.monotonic()<deadline
            time.sleep(.1)
        a=capture('02-running-a');time.sleep(.5);b=capture('03-running-b')
        scale=a.width/ok('ui.state')['window']['width']
        crop=tuple(round(v*scale) for v in [x+2,y+2,x+w-2,y+h-2])
        difference=ImageChops.difference(a.crop(crop),b.crop(crop))
        pixels=sum(any(pixel) for pixel in difference.getdata())
        assert pixels>20,(pixels,frame())
        report.append({'animation_crop':crop,'different_pixels':pixels,'maxnorm':max(v[1] for v in difference.getextrema())})
        command('view.page',page=1)
        count=ok('ui.state')['active']['pipeline']['results']
        time.sleep(.8);wait()
        assert ok('ui.state')['active']['pipeline']['results']==count,'offscreen animation kept rendering'
        command('view.page',page=55);assert frame()==['0.0'],frame()
        capture('04-returned')
        dirty=forms()['dirty'];ok('ui.quit')
        if dirty:wait();ok('ui.command',{'id':'document.discard'})
        assert app.wait(timeout=15)==0
        print(f'Animation viewer smoke passed: {len(report)} records, {pixels} changing frame pixels; {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
