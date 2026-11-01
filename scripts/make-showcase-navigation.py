#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Make the original navigation/search PDF used by the recorded walkthrough (ReportLab)."""
from pathlib import Path
from reportlab.pdfgen import canvas
from reportlab.lib.colors import HexColor, white

root = Path(__file__).resolve().parent.parent
path = root/'crates/pdfoxium-viewer/assets/demos/showcase-navigation.pdf'
ink, accent, soft = map(HexColor, ['#283932', '#256958', '#d7eadb'])
c = canvas.Canvas(str(path), pagesize=(612, 792), invariant=1)
c.setTitle('Reading and navigation - pdfoxium')
c.setAuthor('pdfoxium contributors')

def header(kicker, title, subtitle, page):
    c.setFillColor(accent); c.rect(0, 642, 612, 150, fill=1, stroke=0)
    c.setFillColor(soft); c.setFont('Helvetica-Bold', 11); c.drawString(44, 750, kicker)
    c.setFillColor(white); c.setFont('Helvetica-Bold', 28); c.drawString(44, 705, title)
    c.setFont('Helvetica', 13); c.drawString(44, 675, subtitle)
    c.setFillColor(ink); c.setFont('Helvetica', 10); c.drawString(44, 28, 'pdfoxium / original showcase document')
    c.drawRightString(568, 28, page)

def text(lines, y, size=15):
    c.setFillColor(ink); c.setFont('Helvetica', size)
    for line in lines: c.drawString(44, y, line); y -= 25

def section(title, target, y, subtitle):
    c.setFillColor(soft); c.roundRect(44, y-64, 524, 82, 8, fill=1, stroke=0)
    c.setFillColor(accent); c.setFont('Helvetica-Bold', 18); c.drawString(62, y-9, title)
    c.setFillColor(ink); c.setFont('Helvetica', 12); c.drawString(62, y-37, subtitle)
    c.linkRect('', target, (44,y-64,568,y+18), relative=0, thickness=0)

c.bookmarkPage('overview'); c.addOutlineEntry('Start here', 'overview')
c.addPageLabel(0, style='ROMAN_LOWER', start=1)
header('READ / EXPLORE / EDIT', 'A document, connected', 'Bookmarks, links and page previews keep your place.', 'i')
text(['Open the outline or page previews in the sidebar.', 'Follow a link below, then return with location history.'], 596)
section('1   Reading and search', 'reading', 475, 'Find text, move between matches, and copy a selection.')
section('2   Vector detail', 'graphics', 365, 'Zoom into crisp paths, pan, and rotate the view.')
text(['Everything in this document is original demo content.', 'The viewer runs on the safe Rust pdfoxium API.'], 205, 13)
c.showPage()
c.bookmarkPage('reading'); c.addOutlineEntry('Reading and search', 'reading')
c.addPageLabel(1, style='ARABIC', start=1)
header('READING', 'Find your place', 'Search from the current page without losing context.', '1')
text(['Search starts on the current page.', 'Typing highlights matches without moving the document.', 'Choose the next match when you are ready to jump.', '', 'A selection follows each line, including font changes.', 'Drag across text to select it, then copy with Command-C.'], 594)
c.bookmarkHorizontalAbsolute('selection', 445); c.addOutlineEntry('Text selection', 'selection', level=1)
c.setFillColor(soft); c.roundRect(44, 220, 524, 138, 8, fill=1, stroke=0)
text(['Search for "document" to see several matches.', 'A document can contain text, paths, images and forms.', 'The original document stays intact when you save a copy.'], 324, 14)
c.setFillColor(accent); c.setFont('Helvetica-Bold', 13); c.drawString(44, 150, 'Return to the overview')
c.linkRect('', 'overview', (44,142,230,164), relative=0, thickness=0)
c.showPage()
c.bookmarkPage('graphics'); c.addOutlineEntry('Vector detail', 'graphics')
header('VECTOR GRAPHICS', 'Look a little closer', 'Smooth zoom, sharp rendering, and tiles for large views.', '2')
text(['This diagram is made from PDF paths and text.', 'Zoom in to inspect the geometry, then return to fit width.'], 594, 14)
for row in range(5):
    for col in range(8):
        x, y = 64+col*65, 470-row*47
        c.setStrokeColor(accent); c.setLineWidth(.6)
        if col<7: c.line(x,y,x+65,y)
        if row<4: c.line(x,y,x,y-47)
        c.setFillColor(soft if (row+col)%2 else white)
        c.circle(x,y,12,fill=1,stroke=1)
        c.setFillColor(ink); c.setFont('Helvetica',6); c.drawCentredString(x,y-2,f'{row+1}.{col+1}')
text(['The document also supports page labels and bookmarks.', 'Return to reading to find the next document match.'], 190, 13)
c.setFillColor(accent); c.setFont('Helvetica-Bold',13); c.drawString(44,115,'Back to reading')
c.linkRect('', 'reading',(44,107,200,130),relative=0,thickness=0)
c.save()
print(path)
