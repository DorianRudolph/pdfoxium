#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Exercise basic AcroForm editing, live drawing, focus and save/reopen through control input (Pillow)."""
import argparse
import json
from pathlib import Path
import socket
import subprocess
import time
import hashlib
import math
from PIL import Image, ImageChops


def form_pdf(path, cropped=False, large=False):
    objects = ['']*5
    fields = [
        '/FT /Tx /T (name) /Rect [20 330 180 360] /V (Initial)',
        '/FT /Tx /T (second) /Rect [220 330 380 360] /V ()',
        '/FT /Btn /T (check) /Rect [20 270 50 300] /V /Off /AS /Off',
        '/FT /Ch /T (choice) /Rect [20 210 180 240] /Ff 393216 /Opt [(Red) (Green) (Blue)] /V (Red)',
        '/FT /Tx /T (readonly) /Rect [20 140 180 170] /Ff 1 /V (Fixed)',
        '/FT /Tx /T (multiline) /Rect [20 20 180 120] /Ff 4096 /V (First)',
        '/FT /Ch /T (list) /Rect [220 210 380 300] /Opt [(One) (Two) (Three) (Four) (Five) (Six) (Seven)] /V (One) /I [0]',
        '/Rect [230 145 250 165]',
        '/Rect [290 145 310 165]',
    ]
    refs = []
    for field in fields:
        refs.append(f'{len(objects)+1} 0 R')
        objects.append(f'<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /BS << /W 1 /S /S >> /MK << /BC [0 0 0] /BG [1 1 1] >> {field} >>')
    off = len(objects)+1
    content = '1 g 0 0 30 30 re f 0 G 1 w 0 0 30 30 re S'
    objects.append(f'<< /Type /XObject /Subtype /Form /BBox [0 0 30 30] /Length {len(content)} >>\nstream\n{content}\nendstream')
    on = len(objects)+1
    content = '1 g 0 0 30 30 re f 0 g 7 7 16 16 re f'
    objects.append(f'<< /Type /XObject /Subtype /Form /BBox [0 0 30 30] /Length {len(content)} >>\nstream\n{content}\nendstream')
    parent = len(objects)+1
    objects.append(f'<< /FT /Btn /Ff 32768 /T (radio) /V /A /Kids [{refs[7]} {refs[8]}] >>')
    for index, name in [(12, 'A'), (13, 'B')]:
        state = name if name == 'A' else 'Off'
        objects[index] = objects[index][:-3] + f' /Parent {parent} 0 R /AS /{state} /AP << /N << /Off {off} 0 R /{name} {on} 0 R >> >> >>'
    objects[7] = objects[7][:-3] + f' /AP << /N << /Off {off} 0 R /Yes {on} 0 R >> >> >>'
    annots = ' '.join(refs)
    objects[0] = '<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>'
    objects[1] = '<< /Type /Pages /Kids [3 0 R] /Count 1 >>'
    objects[2] = f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /Helv 5 0 R >> >> /Annots [{annots}] >>'
    fields_refs = ' '.join(refs[:7]+[f'{parent} 0 R'])
    objects[3] = f'<< /Fields [{fields_refs}] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>'
    objects[4] = '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>'
    if large:
        objects[2] = objects[2].replace('/MediaBox [0 0 400 400]','/MediaBox [0 0 1000 1000]')
    if cropped:
        objects[2] = objects[2][:-3] + ' /CropBox [10 10 390 390] /Rotate 90 >>'
    pdf = '%PDF-1.7\n'; offsets = []
    for index, obj in enumerate(objects):
        offsets.append(len(pdf)); pdf += f'{index+1} 0 obj\n{obj}\nendobj\n'
    xref = len(pdf); pdf += f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'
    pdf += ''.join(f'{offset:010} 00000 n \n' for offset in offsets)
    pdf += f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
    path.write_text(pdf)


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/release/pdfoxium-viewer')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    endpoint_path = args.output / 'control.json'
    assert not endpoint_path.exists()
    report = []
    pdf = (args.output / 'forms.pdf').resolve()
    form_pdf(pdf)
    with (args.output / 'app.log').open('w') as log:
        app = subprocess.Popen([str(args.binary), str(pdf), '--control', str(endpoint_path.resolve()), '--control-only-input'], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while not endpoint_path.exists():
                assert app.poll() is None, 'viewer exited; see app.log'
                assert time.monotonic() < deadline, 'viewer did not start'
                time.sleep(0.05)
            endpoint = json.loads(endpoint_path.read_text())

            def call(method, params=None, error=False):
                with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=40) as conn:
                    conn.sendall(json.dumps({'jsonrpc': '2.0', 'id': len(report), 'token': endpoint['token'], 'method': method, 'params': params or {}}).encode() + b'\n')
                    response = json.loads(conn.makefile('rb').readline())
                report.append({'method': method, 'params': params or {}, 'response': response})
                if error:
                    assert 'error' in response, response
                    return response['error']
                assert 'error' not in response, response
                return response['result']

            def wait():
                state = call('ui.wait')
                assert state['settled'] and state['active']['render_complete']
                return state

            initial = wait()
            source_hash = hashlib.sha256(pdf.read_bytes()).digest()
            def command(id, **params):
                call('ui.command', {'id': id, **params})
                return wait()
            def forms():
                return call('ui.forms')
            def field(name):
                return next(f for f in forms()['fields'] if f['name'] == name)
            def click(name):
                x,y,w,h = field(name)['screen_rect']
                call('ui.click', {'position':[x+w/2,y+h/2]})
                return wait()
            def capture(name):
                path = (args.output/f'{name}.png').resolve()
                call('ui.screenshot', {'path':str(path)})
                return Image.open(path).convert('RGB')
            command('view.zoom', zoom=1.5)
            assert forms()['enabled'] and forms()['javascript']
            assert len(forms()['fields']) == 9
            baseline = capture('01-baseline')
            # Hover queues form/cursor input, but must not imply PDF rendering.
            hover_before = call('ui.state')['active']['pipeline']
            for name in ['name','second','readonly','multiline','choice','list']:
                x,y,w,h = field(name)['screen_rect']
                for n in range(6):
                    call('ui.pointer',{'position':[x+w*(0.2+n*0.05),y+h/2]})
                    state=call('ui.state')
                    assert not state['busy_indicator'], state
                    assert not state['active']['rendering'], state
                wait()
            hover_after=call('ui.state')['active']['pipeline']
            assert hover_after['results']==hover_before['results']
            assert hover_after['partial_updates']==hover_before['partial_updates']
            assert not forms()['dirty'] and forms()['focus'] is None
            # Buttons may legitimately repaint once on enter for rollover
            # appearances. Motion within the same button must not keep painting.
            x,y,w,h=field('check')['screen_rect']
            call('ui.pointer',{'position':[x+w/2,y+h/2]});wait()
            hover_before=call('ui.state')['active']['pipeline']
            for n in range(12):
                call('ui.pointer',{'position':[x+w*(0.3+n*0.02),y+h/2]})
                assert not call('ui.state')['busy_indicator']
            wait()
            hover_after=call('ui.state')['active']['pipeline']
            assert hover_after['results']==hover_before['results']
            assert hover_after['partial_updates']==hover_before['partial_updates']

            click('name')
            assert forms()['focus']['name'] == 'name' and forms()['text'] == 'Initial'
            call('ui.key', {'key':'A','modifiers':['command']});wait()
            assert forms()['selected_text'] == 'Initial'
            pipeline_before = call('ui.state')['active']['pipeline']
            call('ui.type', {'text':'Viewer value'});wait()
            pipeline_after = call('ui.state')['active']['pipeline']
            assert pipeline_after['textures'] == pipeline_before['textures']
            assert pipeline_after['partial_updates'] > pipeline_before['partial_updates']
            assert pipeline_after['bytes'] - pipeline_before['bytes'] < 200000
            time.sleep(0.65)
            wait()
            caret_pipeline = call('ui.state')['active']['pipeline']
            assert caret_pipeline['partial_updates'] > pipeline_after['partial_updates']
            assert caret_pipeline['textures'] == pipeline_after['textures']
            assert forms()['text'] == 'Viewer value' and forms()['dirty']
            x,y,w,h = field('name')['screen_rect']
            call('ui.click', {'position':[x+10,y+h/2], 'count':2});wait()
            assert forms()['selected_text'] == 'Viewer', forms()
            call('ui.key', {'key':'End'});wait()
            edited = capture('02-focused-edit')
            state = call('ui.state')
            scale = edited.width/state['window']['width']
            x,y,w,h = field('name')['screen_rect']
            crop = tuple(round(v*scale) for v in [x+2,y+2,x+w-2,y+h-2])
            diff = ImageChops.difference(baseline.crop(crop),edited.crop(crop))
            assert diff.getbbox() is not None
            assert min(v[0] for v in edited.crop(crop).getextrema()) < 100
            call('ui.key', {'key':'Tab'});wait()
            assert forms()['focus']['name'] == 'second'
            assert field('name')['value'] == 'Viewer value'
            call('ui.type', {'text':'Second value'});wait()
            call('ui.key', {'key':'Tab','modifiers':['shift']});wait()
            assert forms()['focus']['name'] == 'name'
            click('check')
            assert field('check')['checked']
            checked = capture('03-checked')
            x,y,w,h = field('check')['screen_rect']
            assert max(checked.getpixel((round((x+w/2)*scale),round((y+h/2)*scale)))) < 100
            click('readonly')
            call('ui.key', {'key':'A','modifiers':['command']});wait()
            call('ui.type', {'text':'Must not change'});wait()
            assert field('readonly')['value'] == 'Fixed'
            click('choice')
            call('ui.key', {'key':'ArrowDown'});wait()
            call('ui.key', {'key':'Enter'});wait()
            command('form.blur')
            assert field('choice')['value'] == 'Green'
            command('view.thumbnails')
            previews = call('ui.thumbnails')
            assert previews['rows'][0]['source'] == 'rendered'
            thumb_before = capture('03-live-preview-before')
            click('check')
            assert not field('check')['checked']
            thumb_after = capture('03-live-preview-after')
            x,y,w,h = call('ui.thumbnails')['rows'][0]['rect']
            crop = tuple(round(v*scale) for v in [x,y,x+w,y+h])
            assert ImageChops.difference(thumb_before.crop(crop), thumb_after.crop(crop)).getbbox() is not None
            click('check')
            assert field('check')['checked']
            command('view.thumbnails')
            click('list')
            command('form.blur')
            # Focus the first row, then let wheel-down select the next row.
            x,y,w,h = field('list')['screen_rect']
            call('ui.click', {'position':[x+20,y+9]});wait()
            before_scroll = call('ui.state')['active']['scroll']
            call('ui.wheel', {'position':[x+20,y+9], 'dy':-40});wait()
            assert forms()['focus']['name'] == 'list'
            command('form.blur')
            assert field('list')['value'] == 'Two', field('list')
            assert call('ui.state')['active']['scroll'] == before_scroll
            radios = [f for f in forms()['fields'] if f['name'] == 'radio']
            x,y,w,h = radios[1]['screen_rect']
            call('ui.click', {'position':[x+w/2,y+h/2]});wait()
            radios = [f for f in forms()['fields'] if f['name'] == 'radio']
            assert not radios[0]['checked'] and radios[1]['checked'], radios
            radio_image = capture('03-radio-group')
            x,y,w,h = radios[1]['screen_rect']
            assert max(radio_image.getpixel((round((x+w/2)*scale),round((y+h/2)*scale)))) < 100
            click('multiline')
            call('ui.key', {'key':'A','modifiers':['command']});wait()
            call('ui.type', {'text':'First line'});wait()
            call('ui.key', {'key':'Enter'});wait()
            call('ui.type', {'text':'Second line'});wait()
            command('form.blur')
            assert field('multiline')['value'].splitlines() == ['First line','Second line']
            multi_image = capture('03-multiline')
            x,y,w,h = field('multiline')['screen_rect']
            crop = multi_image.crop(tuple(round(v*scale) for v in [x+3,y+3,x+w-3,y+h-3]))
            rows = [any(max(crop.getpixel((xx,yy)))<100 for xx in range(crop.width)) for yy in range(crop.height)]
            assert sum(v and (i==0 or not rows[i-1]) for i,v in enumerate(rows)) == 2
            output = (args.output/'filled-copy.pdf').resolve()
            command('form.save', path=str(output))
            assert not forms()['dirty'] and output.exists()
            assert hashlib.sha256(pdf.read_bytes()).digest() == source_hash
            call('ui.open', {'path':str(output)});wait()
            command('view.zoom', zoom=1.5)
            assert field('name')['value'] == 'Viewer value'
            assert field('second')['value'] == 'Second value'
            assert field('check')['checked']
            assert field('choice')['value'] == 'Green'
            capture('04-reopened')
            for rotation in range(1, 4):
                command('view.rotate')
                click('name')
                assert forms()['focus']['name'] == 'name'
                call('ui.key', {'key':'A','modifiers':['command']});wait()
                call('ui.type', {'text':f'Rotation {rotation}'});wait()
                assert forms()['text'] == f'Rotation {rotation}'
                command('form.blur')
                assert field('name')['value'] == f'Rotation {rotation}'
                capture(f'05-rotation-{rotation}')
            # Opening/closing a dirty form must wait for an explicit decision.
            call('ui.open', {'path':str(pdf)});wait()
            assert call('ui.state')['departure']['kind'] == 'open'
            capture('05-unsaved-changes')
            call('ui.key',{'key':'Escape'});wait()
            assert call('ui.state')['departure'] is None
            assert field('name')['value'] == 'Rotation 3'
            call('ui.quit');wait()
            assert call('ui.state')['departure']['kind'] == 'close'
            command('document.cancel')
            # Saving a new copy completes a pending open only after success.
            cropped = (args.output/'cropped.pdf').resolve()
            form_pdf(cropped, cropped=True)
            call('ui.open', {'path':str(cropped)});wait()
            assert call('ui.state')['departure']['kind'] == 'open'
            call('ui.click',{'id':'document.save'});wait()
            capture('05-save-copy-dialog')
            command('form.save', path=str((args.output/'rotation-copy.pdf').resolve()))
            assert call('ui.state')['departure'] is None
            command('view.zoom', zoom=1.5)
            click('name')
            call('ui.key', {'key':'A','modifiers':['command']});wait()
            call('ui.type', {'text':'Cropped rotation'});wait()
            command('form.blur')
            assert field('name')['value'] == 'Cropped rotation'
            capture('06-cropped-intrinsic-rotation')
            large = (args.output/'large.pdf').resolve()
            form_pdf(large, large=True)
            call('ui.open', {'path':str(large)});wait()
            command('document.discard')
            state = call('ui.state')['active']
            monitor = state['render_policy']['monitor_pixels']
            zoom = math.sqrt(5*monitor[0]*monitor[1])/1000/state['render_policy']['pixels_per_point']
            assert 0.1 <= zoom <= 8
            command('view.zoom', zoom=zoom)
            x,y,w,h = field('name')['screen_rect']
            state = call('ui.state')['active']
            call('ui.scroll', {'dx':x-state['viewport'][0]-20,'dy':y-state['viewport'][1]-100});wait()
            x,y,w,h = field('name')['screen_rect']
            call('ui.click', {'position':[x+20,y+h/2]});wait()
            assert call('ui.state')['active']['render_policy']['pages'][0]['mode'] == 'tiled'
            call('ui.key', {'key':'A','modifiers':['command']});wait()
            before = call('ui.state')['active']['pipeline']
            for part in ['Tile',' ','entry']:
                call('ui.type', {'text':part})
            wait()
            after = call('ui.state')['active']['pipeline']
            assert after['textures'] == before['textures']
            assert after['partial_updates'] > before['partial_updates']
            command('form.blur')
            tile_copy = (args.output/'tile-copy.pdf').resolve()
            command('form.save',path=str(tile_copy))
            partial = capture('07-tile-patch')
            state = call('ui.state')['active']
            x,y,w,h = field('name')['screen_rect']
            crop_box = tuple(round(v*scale) for v in [x,y,x+w,y+h])
            scroll = state['scroll']
            call('ui.open',{'path':str(tile_copy)});wait()
            command('view.zoom',zoom=zoom)
            current_scroll = call('ui.state')['active']['scroll']
            call('ui.scroll',{'dx':scroll[0]-current_scroll[0],'dy':scroll[1]-current_scroll[1]});wait()
            fresh = capture('08-tile-fresh-render')
            diff = ImageChops.difference(partial.crop(crop_box),fresh.crop(crop_box))
            maximum = max(v[1] for v in diff.getextrema())
            pixels = diff.tobytes()
            report.append({'comparison':'tile-patch-vs-fresh','maxnorm':maximum,'different_pixels':sum(any(pixels[i:i+3]) for i in range(0,len(pixels),3))})
            assert maximum <= 2, diff.getextrema()
            call('ui.quit')
            assert app.wait(timeout=15) == 0
            assert not endpoint_path.exists()
            print(f'Forms viewer smoke passed: {len(report)} records; screenshots in {args.output}')
        finally:
            (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
            if app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()


if __name__ == '__main__':
    main()
