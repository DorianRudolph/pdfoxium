#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Create an original annotation-frame animation PDF (ReportLab and pypdf)."""
from io import BytesIO
import math
from pathlib import Path
from reportlab.pdfgen import canvas
from pypdf import PdfReader, PdfWriter
from pypdf.generic import (
    ArrayObject, DictionaryObject, DecodedStreamObject, FloatObject,
    NameObject, NumberObject, TextStringObject,
)

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'crates/pdfoxium-viewer/assets/demos/animation-demo.pdf'
buf = BytesIO()
c = canvas.Canvas(buf, pagesize=(612,792), invariant=1)
for page in range(2):
    c.setFillColorRGB(.08,.42,.58); c.rect(0,680,612,112,fill=1,stroke=0)
    c.setFillColorRGB(1,1,1); c.setFont('Helvetica-Bold',26)
    c.drawString(40,735,'A travelling wave' if page==0 else 'Animation pauses offscreen')
    c.setFont('Helvetica',12); c.drawString(40,710,'pdfoxium / JavaScript and annotation-frame animation')
    c.setFillColorRGB(.12,.19,.24); c.setFont('Helvetica',13)
    if page==0:
        c.drawString(56,628,'Click the picture or Play / Pause to start and stop.')
        c.drawString(56,604,'The wave travels; the orange point moves up and down.')
        c.setFillColorRGB(.94,.97,.98); c.roundRect(56,300,500,240,12,fill=1,stroke=0)
        c.setStrokeColorRGB(.65,.73,.77); c.setLineWidth(.6)
        c.line(70,420,542,420); c.line(306,312,306,528)
        c.setFillColorRGB(.12,.19,.24); c.setFont('Helvetica',11)
        c.drawString(56,220,'24 vector frames / 10 frames per second / no embedded movie')
        c.drawString(56,200,'Go to page 2 to stop it. Returning resets the first frame.')
    else:
        c.drawString(56,620,'The page-visibility action cancels the animation timer.')
        c.drawString(56,594,'Return to page 1 and click the picture to play again.')
    c.setFont('Helvetica',10); c.drawString(40,30,f'Original demo content / {page+1} of 2')
    c.showPage()
c.save()
w = PdfWriter(); w.clone_document_from_reader(PdfReader(buf))
def number_array(values): return ArrayObject([FloatObject(v) for v in values])
def js(source): return DictionaryObject({NameObject('/S'):NameObject('/JavaScript'),NameObject('/JS'):TextStringObject(source)})
def appearance(content, width, height):
    stream=DecodedStreamObject(); stream.set_data(content.encode())
    stream.update({NameObject('/Type'):NameObject('/XObject'),NameObject('/Subtype'):NameObject('/Form'),NameObject('/BBox'):number_array([0,0,width,height]),NameObject('/Resources'):DictionaryObject()})
    return w._add_object(stream)
fields=ArrayObject(); annots=ArrayObject()
def widget(name, rect, ap=None, hidden=False, caption=None, actions=None):
    d=DictionaryObject({NameObject('/Type'):NameObject('/Annot'),NameObject('/Subtype'):NameObject('/Widget'),NameObject('/FT'):NameObject('/Btn'),NameObject('/Ff'):NumberObject(65536),NameObject('/T'):TextStringObject(name),NameObject('/Rect'):number_array(rect),NameObject('/F'):NumberObject(6 if hidden else 4),NameObject('/P'):w.pages[0].indirect_reference})
    if ap is not None:d[NameObject('/AP')]=DictionaryObject({NameObject('/N'):ap})
    if caption is not None:
        d[NameObject('/MK')]=DictionaryObject({NameObject('/CA'):TextStringObject(caption),NameObject('/BG'):number_array([.91,.95,.97]),NameObject('/BC'):number_array([.08,.42,.58])})
    if actions:d[NameObject('/AA')]=DictionaryObject({NameObject('/'+key):js(value) for key,value in actions.items()})
    ref=w._add_object(d); fields.append(ref);annots.append(ref)
for frame in range(24):
    phase=2*math.pi*frame/24
    points=[(14+472*i/160,120+76*math.sin(4*math.pi*i/160-phase)) for i in range(161)]
    path=' '.join(f'{x:.4f} {y:.4f} '+('m' if i==0 else 'l') for i,(x,y) in enumerate(points))
    y=120+76*math.sin(2*math.pi-phase);r=6.;k=.5522847498*r
    circle=f'256 {y} m 256 {y+k} {250+k} {y+r} 250 {y+r} c {250-k} {y+r} 244 {y+k} 244 {y} c 244 {y-k} {250-k} {y-r} 250 {y-r} c {250+k} {y-r} 256 {y-k} 256 {y} c f'
    content=f'q 0.08 0.42 0.58 RG 2.5 w 1 J 1 j {path} S 0.96 0.40 0.12 rg {circle} Q'
    widget(f'frame{frame:02}',[56,300,556,540],appearance(content,500,240),hidden=frame!=0,actions={'U':'toggleAnimation();'})
init='''var animationDoc=this; var animationFrame=0; var animationTimer=null;
function showAnimationFrame(n){var dirty=animationDoc.dirty;for(var i=0;i<24;i++){animationDoc.getField('frame'+(i<10?'0':'')+i).display=(i==n)?display.visible:display.hidden;}animationFrame=n;animationDoc.dirty=dirty;}
function advanceAnimation(){showAnimationFrame((animationFrame+1)%24);}
function toggleAnimation(){if(animationTimer){app.clearInterval(animationTimer);animationTimer=null;}else{animationTimer=app.setInterval('advanceAnimation()',100);}}
showAnimationFrame(0);'''
stop="if(animationTimer){app.clearInterval(animationTimer);animationTimer=null;}showAnimationFrame(0);"
widget('animation',[56,300,556,540],appearance('',500,240),actions={'PO':stop,'PI':stop,'PC':stop,'U':'toggleAnimation();'})
widget('toggle',[56,250,216,282],caption='Play / Pause',actions={'U':'toggleAnimation();'})
w.root_object[NameObject('/Names')]=DictionaryObject({NameObject('/JavaScript'):DictionaryObject({NameObject('/Names'):ArrayObject([TextStringObject('animation'),w._add_object(js(init))])})})
w.pages[0][NameObject('/Annots')]=annots
fonts=DictionaryObject(dict(w.pages[0]['/Resources']['/Font']))
fonts[NameObject('/Helv')]=fonts['/F1']
w.root_object[NameObject('/AcroForm')]=DictionaryObject({NameObject('/Fields'):fields,NameObject('/DA'):TextStringObject('/Helv 12 Tf 0 g'),NameObject('/DR'):DictionaryObject({NameObject('/Font'):fonts})})
w.add_metadata({'/Title':'A travelling wave — animated PDF demo'})
with OUT.open('wb') as f:w.write(f)
print(OUT)
