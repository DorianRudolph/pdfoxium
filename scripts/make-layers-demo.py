#!/usr/bin/env python3
# Copyright 2026 The pdfoxium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Create the original, embedded two-page optional-content example (ReportLab/pypdf)."""
from io import BytesIO
from pathlib import Path
from reportlab.pdfgen import canvas
from reportlab.lib.colors import HexColor
from pypdf import PdfReader, PdfWriter
from pypdf.generic import ArrayObject, DictionaryObject, NameObject, TextStringObject, NumberObject, FloatObject

root = Path(__file__).resolve().parents[1]
buffer = BytesIO()
c = canvas.Canvas(buffer, pagesize=(600, 640), pageCompression=1, invariant=1)
c.setTitle('A map with layers — pdfoxium')
for page in range(2):
    c.setFillColor(HexColor('#f5f3e9')); c.rect(0, 0, 600, 640, fill=1, stroke=0)
    c.setFillColor(HexColor('#256958')); c.rect(0, 505, 600, 135, fill=1, stroke=0)
    c.setFillColorRGB(1, 1, 1); c.setFont('Helvetica-Bold', 27)
    c.drawString(32, 585, 'A map with layers' if page == 0 else 'One layer, multiple pages')
    c.setFont('Helvetica', 12); c.drawString(32, 554, 'Open Layers in the toolbar to change what you see.')
    c.drawString(32, 531, 'Visibility changes affect every page. The PDF stays unchanged.')
    c.setFillColor(HexColor('#e0e6d8')); c.rect(32, 138, 536, 337, fill=1, stroke=0)
    c.addLiteral('/OC /Roads BDC')
    c.setStrokeColor(HexColor('#d08433')); c.setLineWidth(18)
    c.line(42, 190 + page*20, 555, 380); c.line(90, 465, 450, 148)
    c.addLiteral('EMC')
    c.addLiteral('/OC /Buildings BDC')
    c.setFillColor(HexColor('#5b8697'))
    for x,y,w,h in [(85,270,100,70),(260,370,105,65),(360,220,110,75)]:
        c.roundRect(x+page*8,y,w,h,7,fill=1,stroke=0)
    c.addLiteral('EMC')
    c.addLiteral('/OC /Labels BDC')
    c.setFillColor(HexColor('#243d34')); c.setFont('Helvetica-Bold', 14)
    for x,y,label in [(96,300,'Library'),(275,397,'Museum'),(374,250,'Town hall')]:
        c.drawString(x+page*8,y,label)
    c.addLiteral('EMC')
    c.addLiteral('/OC /Guide BDC')
    c.setFillColor(HexColor('#243d34')); c.setFont('Helvetica', 12)
    c.drawString(32, 106, 'Roads: orange   /   Buildings: blue   /   Labels: names')
    c.drawString(32, 84, 'Try the Buildings only configuration, then Reset layers.')
    c.setFont('Helvetica', 10); c.drawString(32, 42, f'pdfoxium  /  PDF optional content groups  /  {page+1} of 2')
    c.addLiteral('EMC')
    c.setFillColor(HexColor('#256958')); c.roundRect(416, 58, 152, 21, 4, fill=1, stroke=0)
    c.setFillColorRGB(1,1,1); c.setFont('Helvetica', 11); c.drawString(430,65,'Toggle roads (PDF action)')
    c.showPage()
c.save()
w = PdfWriter(); w.clone_document_from_reader(PdfReader(buffer))
groups = {}
for key,title in [('Roads','Roads'),('Buildings','Buildings'),('Labels','Place names'),('Guide','Guide (locked)')]:
    groups[key]=w._add_object(DictionaryObject({NameObject('/Type'):NameObject('/OCG'),NameObject('/Name'):TextStringObject(title)}))
all_groups=ArrayObject(list(groups.values()))
default=DictionaryObject({NameObject('/Name'):TextStringObject('Map'),NameObject('/BaseState'):NameObject('/ON'),NameObject('/Order'):all_groups,NameObject('/Locked'):ArrayObject([groups['Guide']])})
alternate=DictionaryObject({NameObject('/Name'):TextStringObject('Buildings only'),NameObject('/BaseState'):NameObject('/OFF'),NameObject('/ON'):ArrayObject([groups['Buildings'],groups['Guide']]),NameObject('/Order'):all_groups,NameObject('/Locked'):ArrayObject([groups['Guide']])})
w.root_object[NameObject('/OCProperties')]=DictionaryObject({NameObject('/OCGs'):all_groups,NameObject('/D'):default,NameObject('/Configs'):ArrayObject([alternate])})
for page in w.pages:
    action = DictionaryObject({NameObject('/S'):NameObject('/SetOCGState'),NameObject('/State'):ArrayObject([NameObject('/Toggle'),groups['Roads']])})
    link = DictionaryObject({NameObject('/Type'):NameObject('/Annot'),NameObject('/Subtype'):NameObject('/Link'),NameObject('/Rect'):ArrayObject([FloatObject(n) for n in [416,58,568,79]]),NameObject('/Border'):ArrayObject([NumberObject(0)]*3),NameObject('/A'):action})
    page[NameObject('/Annots')]=ArrayObject([w._add_object(link)])
    page['/Resources'][NameObject('/Properties')]=DictionaryObject({NameObject('/'+key):value for key,value in groups.items()})
w.pdf_header=b'%PDF-1.7'
output=root/'crates/pdfoxium-viewer/assets/demos/layers-demo.pdf'; output.parent.mkdir(parents=True,exist_ok=True)
with output.open('wb') as file:w.write(file)
print(output)
