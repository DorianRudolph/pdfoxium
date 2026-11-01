#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Test live JS forms (fixture emitted by forms_javascript.rs), optionally the showcase."""
import argparse
import json
import re
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image, ImageChops
import pymupdf

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--pdf', type=Path, required=True)
parser.add_argument('--showcase', type=Path)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
endpoint = args.output/'control.json'
assert not endpoint.exists()
report = []
with (args.output/'app.log').open('w') as log:
    app = subprocess.Popen([str(args.binary), str(args.pdf), '--control', str(endpoint.resolve()), '--control-only-input'], stdout=log, stderr=log)
    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert app.poll() is None and time.monotonic()<deadline
            time.sleep(.02)
        address = json.loads(endpoint.read_text())
        def call(method, params=None, error=False):
            with socket.create_connection(('127.0.0.1', address['port']), timeout=40) as conn:
                conn.sendall((json.dumps({'jsonrpc':'2.0','id':len(report),'token':address['token'],'method':method,'params':params or {}})+'\n').encode())
                response = json.loads(conn.makefile('rb').readline())
            report.append({'method':method,'params':params or {},'response':response})
            assert ('error' in response)==error, response
            return response.get('error') if error else response['result']
        def wait():
            return call('ui.wait')
        def command(id, **params):
            call('ui.command', {'id':id, **params})
            return wait()
        def forms():
            return call('ui.forms')
        def field(name):
            return next(f for f in forms()['fields'] if f['name']==name)
        def reveal(name):
            f=field(name);x,y,w,h=f['screen_rect'];v=call('ui.state')['active']['viewport']
            call('ui.scroll', {'dx':max(0,x-v[0]-50), 'dy':y-v[1]-100});wait()
            return field(name)
        def click(name):
            x,y,w,h=field(name)['screen_rect']
            call('ui.click', {'position':[x+w/2,y+h/2]})
            return wait()
        def edit(name, text):
            click(name);call('ui.key', {'key':'A','modifiers':['command']});wait()
            call('ui.type', {'text':text});wait()
        def capture(name):
            path=(args.output/(name+'.png')).resolve()
            call('ui.screenshot', {'path':str(path)})
            return Image.open(path).convert('RGB')
        def reply(response, text=None):
            dialog=forms()['dialog'];assert dialog
            params={'dialog_id':dialog['id'],'response':response}
            if text is not None:params['text']=text
            return command('form.dialog.respond', **params)
        def ap_text(path, name):
            with pymupdf.open(path) as pdf:
                for page in pdf:
                    for widget in page.widgets() or []:
                        if widget.field_name==name:
                            _, ap=pdf.xref_get_key(widget.xref,'AP/N')
                            stream=pdf.xref_stream(int(ap.split()[0]))
                            # These fixtures use Helvetica/WinAnsi. Accept the
                            # PDF writer's hexadecimal text strings as well as
                            # the source document's literal strings.
                            return stream+b' '+b' '.join(bytes.fromhex(h.decode()) for h in re.findall(rb'<([0-9a-fA-F\s]+)>',stream))
            raise AssertionError('missing appearance '+name)
        wait();command('view.zoom',zoom=1.5)
        assert forms()['javascript']
        assert field('startup')['value']=='ready 42 open'
        initial=capture('01-initial')
        edit('amount','25.5')
        call('ui.type',{'text':'x'});wait();assert forms()['text']=='25.5'
        command('form.blur');assert field('amount')['value']=='25.5'
        assert field('total')['value']=='51'
        formatted=capture('02-formatted')
        assert ImageChops.difference(initial,formatted).getbbox()
        edit('validated','reject');command('form.blur');assert field('validated')['value']=='keep'
        click('alert');d=forms()['dialog'];assert d['kind']=='alert' and d['message']=='Hello'
        assert not call('ui.state')['settled']
        capture('03-alert')
        call('ui.command',{'id':'form.dialog.respond','dialog_id':d['id']+100,'response':'ok'},error=True)
        reply('ok');assert field('qty')['value']=='1'
        click('prompt');assert forms()['dialog']['kind']=='response';capture('04-prompt')
        call('ui.key',{'key':'A','modifiers':['command']});wait()
        call('ui.type',{'text':'7'});wait()
        call('ui.key',{'key':'Enter'});wait();assert field('qty')['value']=='7'
        # Script errors are observable and a later input operation can recover.
        x,y,w,h=field('bad')['screen_rect'];call('ui.click',{'position':[x+w/2,y+h/2]})
        call('ui.wait',error=True);assert 'boom' in forms()['error']
        edit('amount','25.5');command('form.blur');assert not forms()['error']
        saved=(args.output/'filled.pdf').resolve();command('form.save',path=str(saved))
        assert b'$25.50' in ap_text(saved,'amount'),ap_text(saved,'amount')
        if args.showcase:
            command('file.open',path=str(args.showcase.resolve()))
            if call('ui.state')['departure']:command('document.discard')
            command('view.page',page=13);command('view.zoom',zoom=1.5)
            reveal('amount');capture('05-showcase-initial')
            edit('amount','42.75');call('ui.type',{'text':'x'});wait();assert forms()['text']=='42.75'
            command('form.blur');assert field('amount')['value']=='42.75'
            capture('06-showcase-currency')
            output=(args.output/'showcase-filled.pdf').resolve();command('form.save',path=str(output))
            assert b'$42.75' in ap_text(output,'amount'),ap_text(output,'amount')
            reveal('hello');click('hello');d=forms()['dialog'];assert 'Hello from the PdfCraft showcase' in d['message']
            capture('07-showcase-alert');reply('ok')
        dirty=forms()['dirty'];call('ui.quit')
        if dirty:wait();command('document.discard')
        assert app.wait(timeout=15)==0
        print(f'JavaScript viewer smoke passed: {len(report)} records; {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:
            app.terminate();app.wait(timeout=10)
