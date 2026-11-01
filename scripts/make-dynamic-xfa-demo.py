#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Create an original repeating-row XFA demo using the existing embedded font."""
from pathlib import Path
from pypdf import PdfReader, PdfWriter
from pypdf.generic import DecodedStreamObject, NameObject

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "crates/pdfoxium-viewer/assets/demos"
template = '''<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
<subform name="demo" layout="tb" locale="en_US">
<pageSet><pageArea name="page"><occur min="1" max="-1"/>
<medium short="612pt" long="792pt"/><contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
</pageArea></pageSet>
<subform name="controls" layout="position" w="540pt" h="120pt">
<draw x="0pt" y="0pt" w="540pt" h="36pt"><font typeface="Helvetica" size="24pt"/><value><text>Dynamic XFA: repeating rows</text></value></draw>
<draw x="0pt" y="40pt" w="540pt" h="22pt"><font typeface="Helvetica" size="11pt"/><value><text>Add two rows to create another page. Remove rows to shrink again.</text></value></draw>
<field name="add" x="0pt" y="74pt" w="150pt" h="30pt"><ui><button/></ui><caption><font typeface="Helvetica" size="12pt"/><value><text>Add row</text></value></caption><border><edge/></border>
<event activity="click"><script contentType="application/x-javascript">if (xfa.form.demo._item.count &lt; 20) { xfa.form.demo._item.addInstance(true); xfa.layout.relayout(); }</script></event></field>
<field name="remove" x="175pt" y="74pt" w="170pt" h="30pt"><ui><button/></ui><caption><font typeface="Helvetica" size="12pt"/><value><text>Remove last row</text></value></caption><border><edge/></border>
<event activity="click"><script contentType="application/x-javascript">if (xfa.form.demo._item.count &gt; 1) { xfa.form.demo._item.removeInstance(xfa.form.demo._item.count - 1); xfa.layout.relayout(); }</script></event></field>
</subform>
<subform name="item" layout="position" w="540pt" h="180pt">
<occur min="1" initial="2" max="20"/><keep intact="contentArea"/>
<field name="description" x="0pt" y="5pt" w="500pt" h="45pt"><font typeface="Helvetica" size="12pt"/><ui><textEdit/></ui><caption placement="top" reserve="18pt"><font typeface="Helvetica" size="10pt"/><value><text>Item description</text></value></caption><border><edge/></border><value><text>New item</text></value></field>
<field name="notes" x="0pt" y="65pt" w="500pt" h="90pt"><font typeface="Helvetica" size="12pt"/><ui><textEdit multiLine="1"/></ui><caption placement="top" reserve="18pt"><font typeface="Helvetica" size="10pt"/><value><text>Notes (multiline)</text></value></caption><border><edge/></border><value><text>Enter notes for this row.</text></value></field>
</subform></subform></template>'''
datasets = '''<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><demo><item><description>First item</description><notes>Edit me, then add more rows.</notes></item><item><description>Second item</description><notes>Rows flow automatically onto new pages.</notes></item></demo></xfa:data></xfa:datasets>'''
writer = PdfWriter()
writer.clone_document_from_reader(PdfReader(OUT / "xfa-demo.pdf"))
packets = writer.root_object["/AcroForm"]["/XFA"]
for index in range(0, len(packets), 2):
    name = str(packets[index])
    if name in ("template", "datasets"):
        stream = DecodedStreamObject()
        stream.set_data((template if name == "template" else datasets).encode())
        packets[index + 1] = writer._add_object(stream)
writer.add_metadata({"/Title": "Dynamic XFA repeating-row demo"})
(OUT / "dynamic-xfa-template.xml").write_text(template + "\n")
with (OUT / "dynamic-xfa-demo.pdf").open("wb") as output:
    writer.write(output)
print(OUT / "dynamic-xfa-demo.pdf")
