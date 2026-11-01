#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Verify loose, continuous teal selection bands through viewer control (Pillow)."""
import argparse
from collections import Counter
import ctypes as C
import json
from pathlib import Path
import socket
import subprocess
import time
from PIL import Image
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,default=root/'target/release/pdfoxium-viewer')
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--native-library',type=Path)
args=parser.parse_args()
args.output.mkdir(parents=True,exist_ok=True)
pdf=(args.output/'selection.pdf').resolve()
content='BT /F1 12 Tf 20 160 Td (Hello be) Tj /F2 12 Tf (go) Tj /F1 12 Tf (ne BANANA) Tj 0 -25 Td (Second line) Tj ET BT /F1 12 Tf 200 160 Td (Column) Tj ET'
objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 220] /CropBox [10 20 290 210] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>','<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>',
    f'<< /Length {len(content)} >>\nstream\n{content}\nendstream']
source='%PDF-1.7\n';offsets=[]
for i,obj in enumerate(objects):
    offsets.append(len(source));source+=f'{i+1} 0 obj\n{obj}\nendobj\n'
xref=len(source)
source+=f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'+''.join(f'{offset:010} 00000 n \n' for offset in offsets)
source+=f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'
pdf.write_text(source)
endpoint_path=args.output/'control.json';assert not endpoint_path.exists()
report=[]
with (args.output/'app.log').open('w') as log:
    app=subprocess.Popen([str(args.binary),str(pdf),'--control',str(endpoint_path.resolve()),'--control-only-input'],stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+30
        while not endpoint_path.exists():
            assert app.poll() is None and time.monotonic()<deadline
            time.sleep(.02)
        endpoint=json.loads(endpoint_path.read_text())
        def call(method,params=None):
            with socket.create_connection(('127.0.0.1',endpoint['port']),timeout=40) as connection:
                connection.sendall((json.dumps({'jsonrpc':'2.0','id':len(report),'token':endpoint['token'],'method':method,'params':params or {}})+'\n').encode())
                response=json.loads(connection.makefile('rb').readline())
            report.append({'method':method,'params':params,'response':response})
            assert 'error' not in response,response
            return response['result']
        def wait():
            state=call('ui.wait');assert state['settled'],state
            return state
        def command(id,**params):
            call('ui.command',{'id':id,**params});return wait()
        def capture(name):
            path=(args.output/f'{name}.png').resolve();call('ui.screenshot',{'path':str(path)})
            return Image.open(path).convert('RGB')
        wait();command('view.zoom',zoom=2.0)
        command('selection.set',page=1,start=6,end=12)
        selection=call('ui.selection')
        capture('selected-mixed-font')
        assert selection['text']=='begone',selection
        assert len(selection['pages'][0]['rects'])==1,selection
        normalized=selection['pages'][0]['rects'][0]
        # Optional native PDFium oracle: independently union loose character
        # bounds for the six selected characters, then apply this crop transform.
        native_image=None
        if args.native_library:
            class Rect(C.Structure):
                _fields_=[(name,C.c_float) for name in ['left','top','right','bottom']]
            native=C.CDLL(str(args.native_library.resolve()))
            for name,params,result in [
                ('FPDF_LoadDocument',[C.c_char_p,C.c_char_p],C.c_void_p),('FPDF_LoadPage',[C.c_void_p,C.c_int],C.c_void_p),
                ('FPDFText_LoadPage',[C.c_void_p],C.c_void_p),('FPDFText_GetLooseCharBox',[C.c_void_p,C.c_int,C.POINTER(Rect)],C.c_int),
                ('FPDFText_ClosePage',[C.c_void_p],None),('FPDF_ClosePage',[C.c_void_p],None),('FPDF_CloseDocument',[C.c_void_p],None)]:
                fn=getattr(native,name);fn.argtypes=params;fn.restype=result
            native.FPDF_InitLibrary()
            doc=native.FPDF_LoadDocument(str(pdf).encode(),None);page=native.FPDF_LoadPage(doc,0);text=native.FPDFText_LoadPage(page)
            bounds=[]
            for i in range(6,12):
                rect=Rect();assert native.FPDFText_GetLooseCharBox(text,i,C.byref(rect));bounds.append(rect)
            left=min(r.left for r in bounds);right=max(r.right for r in bounds);bottom=min(r.bottom for r in bounds);top=max(r.top for r in bounds)
            expected=[(left-10)/280,(210-top)/190,(right-left)/280,(top-bottom)/190]
            assert max(abs(a-b) for a,b in zip(expected,normalized))<1e-5,(expected,normalized)
            (args.output/'native-loose-bounds.json').write_text(json.dumps({'expected':expected,'actual':normalized},indent=2)+'\n')
            for name,params,result in [
                ('FPDFBitmap_CreateEx',[C.c_int,C.c_int,C.c_int,C.c_void_p,C.c_int],C.c_void_p),
                ('FPDFBitmap_FillRect',[C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_int,C.c_ulong],None),
                ('FPDF_RenderPageBitmap',[C.c_void_p,C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_int,C.c_int,C.c_int],None),
                ('FPDFBitmap_Destroy',[C.c_void_p],None)]:
                fn=getattr(native,name);fn.argtypes=params;fn.restype=result
            state=call('ui.state');screenshot=capture('native-comparison-selection')
            pixel_scale=screenshot.width/state['window']['width']
            px,py,pw,ph=state['active']['pages_on_screen'][0]['rect']
            width,height=round(pw*pixel_scale),round(ph*pixel_scale)
            buffer=C.create_string_buffer(width*height*4)
            bitmap=native.FPDFBitmap_CreateEx(width,height,4,buffer,width*4);assert bitmap
            native.FPDFBitmap_FillRect(bitmap,0,0,width,height,0xffffffff)
            native.FPDF_RenderPageBitmap(bitmap,page,0,0,width,height,0,4)  # FPDF_NO_NATIVETEXT: portable text renderer.
            native_image=Image.frombytes('RGBA',(width,height),buffer.raw,'raw','BGRA').convert('RGB')
            native_image.save(args.output/'native-page.png')
            native.FPDFBitmap_Destroy(bitmap)
            native.FPDFText_ClosePage(text);native.FPDF_ClosePage(page);native.FPDF_CloseDocument(doc);native.FPDF_DestroyLibrary()
        # One rectangle must remain one uniformly colored band at all view angles.
        for rotation in range(4):
            if rotation: command('view.rotate')
            selection=call('ui.selection');state=call('ui.state')
            assert selection['text']=='begone' and len(selection['pages'][0]['rects'])==1
            image=capture(f'selection-rotation-{rotation}')
            x,y,w,h=selection['pages'][0]['screen_rects'][0];scale=image.width/state['window']['width']
            box=(round((x+1)*scale),round((y+1)*scale),round((x+w-1)*scale),round((y+h-1)*scale))
            pixels=list(image.crop(box).get_flattened_data());counts=Counter(pixels)
            teal=[(count,color) for color,count in counts.items() if color[1]>color[0]+15 and color[2]>color[0]+10 and color[1]>color[2]]
            assert teal,(rotation,counts.most_common(5))
            dominant,color=max(teal)
            assert dominant/len(pixels)>.4,(rotation,dominant,len(pixels))
            # Blank padding and spaces should have a single alpha application;
            # text antialiasing can darken tint only where the plain glyph ink is.
            command('selection.clear');plain=capture(f'plain-rotation-{rotation}')
            background=list(plain.crop(box).get_flattened_data())
            if rotation==0 and native_image is not None:
                # Compare the actual selected screenshot against independently
                # rendered native pixels with the known selection color. Any
                # excess error must fit the unselected glyph-AA difference.
                px,py,pw,ph=state['active']['pages_on_screen'][0]['rect']
                native_box=(box[0]-round(px*scale),box[1]-round(py*scale),box[2]-round(px*scale),box[3]-round(py*scale))
                native_pixels=list(native_image.crop(native_box).get_flattened_data())
                excess=[];same_base=0;max_error=0
                for actual,base,oracle in zip(pixels,background,native_pixels):
                    expected=tuple(round(c*80/255+b*175/255) for c,b in zip((37,105,88),oracle))
                    error=max(abs(a-b) for a,b in zip(actual,expected));max_error=max(max_error,error)
                    baseline_error=max(abs(a-b) for a,b in zip(base,oracle))
                    same_base+=base==oracle
                    if error>baseline_error+1:excess.append((actual,base,oracle,expected,error))
                assert same_base>100 and not excess,(same_base,excess[:5])
                (args.output/'native-render-comparison.json').write_text(json.dumps({'compared_pixels':len(pixels),'identical_base_pixels':same_base,'max_selected_channel_error':max_error,'new_errors_beyond_baseline':len(excess)},indent=2)+'\n')
            blank=[a for a,b in zip(pixels,background) if b==(255,255,255)]
            assert blank and sum(max(abs(a[i]-color[i]) for i in range(3))<=1 for a in blank)/len(blank)>.98,(rotation,Counter(blank).most_common(5))
            command('selection.set',page=1,start=6,end=12)
        command('view.rotate')
        ends=[]
        for word in ['line','Column']:
            command('search.find',query=word)
            ends.extend(hit['end'] for hit in call('ui.search')['hits'])
        command('search.clear')
        command('selection.set',page=1,start=0,end=max(ends))
        selection=call('ui.selection')
        assert len(selection['pages'][0]['rects'])==3,selection
        capture('selection-lines-and-columns')
        assert 'Second line' in selection['text'] and 'Column' in selection['text']
        call('ui.quit');assert app.wait(timeout=15)==0
        print(f'Selection smoke passed: {len(report)} requests; screenshots in {args.output}')
    finally:
        (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if app.poll() is None:app.terminate();app.wait(timeout=10)
