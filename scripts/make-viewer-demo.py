#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Rebuild original viewer demo PDFs and a transparent PNG insert asset.
Requires reportlab, pypdf and Pillow (available in the bundled Codex Python).
"""
import argparse
import subprocess
from io import BytesIO
from pathlib import Path
from reportlab.pdfgen import canvas
from reportlab.lib.colors import HexColor,white
from pypdf import PdfReader,PdfWriter
from pypdf.generic import NameObject,DictionaryObject,ArrayObject,DecodedStreamObject,TextStringObject,BooleanObject,NumberObject,FloatObject
from PIL import Image,ImageDraw
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--editing-only',action='store_true')
args=parser.parse_args()
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'crates/pdfoxium-viewer/assets/demos';OUT.mkdir(parents=True,exist_ok=True)
INK=HexColor('#20313e');BLUE=HexColor('#146c94')
def header(c,title,subtitle):
    c.setFillColor(BLUE);c.rect(0,680,612,112,fill=1,stroke=0)
    c.setFillColor(white);c.setFont('Helvetica-Bold',26);c.drawString(40,738,title)
    c.setFont('Helvetica',12);c.drawString(40,713,subtitle)
def footer(c,text):
    c.setFillColor(INK);c.setFont('Helvetica',10);c.drawString(40,30,text)
def lines(c,text,x=40,y=630,leading=22,size=13):
    c.setFillColor(INK);c.setFont('Helvetica',size)
    for line in text:c.drawString(x,y,line);y-=leading
c=canvas.Canvas(str(OUT/'editing-demo.pdf'),pagesize=(612,792),invariant=1)
for index,(title,color) in enumerate([('Edit this document','#e6f4fa'),('Delete this page','#fff0df'),('Keep this page','#e6f4e9')]):
    header(c,'Annotations you can edit' if index==0 else title,'pdfoxium / PDF annotations, page content and assembly')
    if index==0:
        lines(c,['This sentence already has a yellow highlight.','Underline marks important text.','Strike out text that should be removed.','Click an annotation to edit its comment.'],leading=30)
    else:
        lines(c,['Select and highlight this sentence.','Then create a text box, rectangle or ellipse.','','Use Edit pages to rotate the actual page,','insert an image, delete a page or import another PDF.'])
    c.setFillColor(HexColor(color));c.roundRect(40,190,532,260,12,fill=1,stroke=0)
    c.setFillColor(INK)
    if index==0:
        c.setFont('Helvetica-Bold',16);c.drawString(65,425,'Shapes and editable text')
        c.setFont('Helvetica',11);c.drawString(345,405,'Rectangle');c.drawString(380,310,'Ellipse')
    else:
        c.setFont('Helvetica-Bold',22);c.drawString(65,390,f'PAGE {index+1}')
        c.setFont('Helvetica',14);c.drawString(65,350,['','Delete this page during the walkthrough.','This page should survive the assembly edits.'][index])
    footer(c,f'Original demo content / Page {index+1} of 3 / Save copy preserves the source')
    c.showPage()
c.save()
subprocess.run(['cargo','run','--quiet','--release','-p','pdfoxium','--example','annotate_demo','--',str(OUT/'editing-demo.pdf')],cwd=ROOT,check=True)
if args.editing_only:
    print(OUT/'editing-demo.pdf')
    raise SystemExit(0)
c=canvas.Canvas(str(OUT/'import-page.pdf'),pagesize=(420,260),invariant=1)
c.setFillColor(BLUE);c.rect(0,0,420,260,fill=1,stroke=0);c.setFillColor(white);c.setFont('Helvetica-Bold',30);c.drawString(30,175,'Imported page');c.setFont('Helvetica',13);c.drawString(30,135,'A different page size makes insertion easy to see.');c.drawString(30,45,'pdfoxium / assembly demo');c.showPage();c.save()
im=Image.new('RGBA',(400,220),(0,0,0,0));d=ImageDraw.Draw(im)
d.rounded_rectangle((4,4,396,216),radius=24,fill=(20,108,148,255))
for x,y,h in [(55,150,40),(125,120,70),(195,90,100),(265,55,135)]:d.rounded_rectangle((x,y,x+42,y+h),radius=7,fill=(255,255,255,255))
im.save(OUT/'insert-chart.png')
# ReportLab maintains one canonical field/widget tree; keep it interactive.
buf=BytesIO();c=canvas.Canvas(buf,pagesize=(612,792),invariant=1);header(c,'Interactive AcroForm','Text, choices, checkbox and JavaScript formatting')
for name,label,y,value in [('name','Name',610,'Ada Lovelace'),('quantity','Quantity',535,'2'),('amount','Amount (formats as currency)',460,'1234.5')]:
    lines(c,[label],y=y+28,size=12);c.acroForm.textfield(name=name,x=40,y=y,width=350,height=25,value=value,borderWidth=1,forceBorder=True,fontSize=12)
lines(c,['Delivery'],y=412,size=12);c.acroForm.choice(name='delivery',x=40,y=373,width=240,height=25,options=['Standard','Express'],value='Standard',forceBorder=True)
c.acroForm.checkbox(name='approved',x=40,y=315,buttonStyle='check',checked=False,forceBorder=True);lines(c,['Approve this request'],x=70,y=321,size=12)
footer(c,'Type in a field, press Tab, toggle approval, then Save copy.');c.showPage();c.save()
w=PdfWriter();w.clone_document_from_reader(PdfReader(buf))
for f in w.get_fields():
    if f=='amount':
        target=next(f.get_object() for f in w._root_object['/AcroForm']['/Fields'] if f.get_object().get('/T')=='amount')
        target[NameObject('/AA')]=DictionaryObject({NameObject('/F'):DictionaryObject({NameObject('/S'):NameObject('/JavaScript'),NameObject('/JS'):TextStringObject("AFNumber_Format(2,0,0,0,'$',true);")})})
with (OUT/'acroform-demo.pdf').open('wb') as f:w.write(f)
# Full XFA uses real template/dataset packets; the fallback is only for readers
# that do not load XFA. It is not an AcroForm imitation of the XFA fields.
buf=BytesIO();c=canvas.Canvas(buf,pagesize=(612,792),invariant=1);header(c,'Interactive XFA form','Open in pdfoxium with JavaScript enabled')
lines(c,['This PDF contains an XFA template and datasets.','A reader without XFA displays this fallback page.','The pdfoxium demo loads the real XFA controls.']);c.showPage();c.save()
template='''<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="demo" layout="position"><pageSet><pageArea name="page"><occur min="1" max="1"/><medium short="612pt" long="792pt"/><contentArea x="36pt" y="36pt" w="540pt" h="720pt"/></pageArea></pageSet>
<draw x="0pt" y="0pt" w="500pt" h="40pt"><font typeface="Helvetica" size="24pt"/><value><text>Interactive XFA form</text></value></draw>
<draw x="0pt" y="42pt" w="500pt" h="22pt"><font typeface="Helvetica" size="11pt"/><value><text>Real XFA controls, layout, data binding and JavaScript.</text></value></draw>
'''
def field(name,label,x,y,width,height,ui,value='',extra=''):
    return f'''<field name="{name}" x="{x}pt" y="{y}pt" w="{width}pt" h="{height}pt"><font typeface="Helvetica" size="12pt"/><ui>{ui}</ui><caption placement="top" reserve="18pt"><font typeface="Helvetica" size="10pt"/><value><text>{label}</text></value></caption><border><edge thickness="0.75pt"/></border><bind match="dataRef" ref="$record.{name}"/><value><text>{value}</text></value>{extra}</field>'''
template+=field('customer','Customer name',0,85,360,50,'<textEdit/>','Ada Lovelace')
template+=field('email','Email',0,155,360,50,'<textEdit/>','ada@demo.test')
template+=field('quantity','Quantity',0,225,150,50,'<numericEdit/>','2')
template+=field('delivery','Delivery',190,225,220,50,'<choiceList open="userControl"/>','Standard','<items><text>Standard</text><text>Express</text></items>')
template+=field('notes','Notes (multiline)',0,305,480,110,'<textEdit multiLine="1"/>','Try editing this XFA field.')
template+='''<field name="approved" x="0pt" y="445pt" w="300pt" h="26pt"><ui><checkButton size="14pt"/></ui><caption placement="right"><font typeface="Helvetica" size="12pt"/><value><text>Approve this request</text></value></caption><items><integer>1</integer><integer>0</integer></items><bind match="dataRef" ref="$record.approved"/><value><integer>0</integer></value></field>
<field name="confirm" x="0pt" y="500pt" w="170pt" h="32pt"><ui><button/></ui><caption><font typeface="Helvetica" size="12pt"/><value><text>Confirm details</text></value></caption><border><edge/></border><event activity="click"><script contentType="application/x-javascript">xfa.form.demo.status.rawValue = "Confirmed";</script></event></field>
<field name="status" access="readOnly" x="200pt" y="500pt" w="260pt" h="32pt"><font typeface="Helvetica" size="12pt"/><ui><textEdit/></ui><value><text>Ready</text></value></field>
</subform></template>'''
(OUT/'xfa-template.xml').write_text(template)
packets=[('preamble','<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">'),('config','<config xmlns="http://www.xfa.org/schema/xci/3.0/"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>'),('template',template),('datasets','<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><demo><customer>Ada Lovelace</customer><email>ada@demo.test</email><quantity>2</quantity><delivery>Standard</delivery><notes>Try editing this XFA field.</notes><approved>0</approved></demo></xfa:data></xfa:datasets>'),('postamble','</xdp:xdp>')]
w=PdfWriter();w.clone_document_from_reader(PdfReader(buf));array=ArrayObject()
for name,xml in packets:
    stream=DecodedStreamObject();stream.set_data(xml.encode());array.extend([TextStringObject(name),w._add_object(stream)])
w._root_object[NameObject('/AcroForm')]=DictionaryObject({NameObject('/Fields'):ArrayObject(),NameObject('/XFA'):array})
# Embed the existing licensed Arimo fixture so native and Rust XFA readers use
# the same font instead of depending on the host's font catalogue.
from reportlab.pdfbase.ttfonts import TTFont
fontpath=ROOT/'tests/capi/resources/test_fonts/Arimo-Regular.ttf'
font=TTFont('DemoArimo',str(fontpath)).face
widths=[]
for code in range(32,256):
    try:char=bytes([code]).decode('cp1252')
    except UnicodeDecodeError:char=' '
    widths.append(NumberObject(int(font.charWidths.get(ord(char),font.defaultWidth)+.5)))
stream=DecodedStreamObject();stream.set_data(fontpath.read_bytes());stream[NameObject('/Length1')]=NumberObject(len(fontpath.read_bytes()))
descriptor=DictionaryObject({NameObject('/Type'):NameObject('/FontDescriptor'),NameObject('/FontName'):NameObject('/Arimo'),NameObject('/Flags'):NumberObject(32),NameObject('/FontBBox'):ArrayObject([FloatObject(value) for value in font.bbox]),NameObject('/ItalicAngle'):NumberObject(0),NameObject('/Ascent'):FloatObject(905.2734375),NameObject('/Descent'):FloatObject(-211.9140625),NameObject('/CapHeight'):FloatObject(font.capHeight),NameObject('/StemV'):NumberObject(70),NameObject('/FontFile2'):w._add_object(stream)})
resource=w._add_object(DictionaryObject({NameObject('/Type'):NameObject('/Font'),NameObject('/Subtype'):NameObject('/TrueType'),NameObject('/BaseFont'):NameObject('/Arimo'),NameObject('/Encoding'):NameObject('/WinAnsiEncoding'),NameObject('/FirstChar'):NumberObject(32),NameObject('/LastChar'):NumberObject(255),NameObject('/Widths'):ArrayObject(widths),NameObject('/FontDescriptor'):w._add_object(descriptor)}))
w._root_object['/AcroForm'][NameObject('/DR')]=DictionaryObject({NameObject('/Font'):DictionaryObject({NameObject('/Helvetica'):resource,NameObject('/Arial'):resource,NameObject('/Helv'):resource})})
w._root_object[NameObject('/NeedsRendering')]=BooleanObject(True)
with (OUT/'xfa-demo.pdf').open('wb') as f:w.write(f)
for pdf in OUT.glob('*.pdf'):
    reader=PdfReader(pdf);print(pdf.relative_to(ROOT),len(reader.pages),'pages',list((reader.get_fields() or {}).keys()))
