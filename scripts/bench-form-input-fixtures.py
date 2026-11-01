#!/usr/bin/env python3
# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Create form input benchmark fixtures from real corpus pages (requires pypdf and Pillow).
Only test files are written; original PDFs are never modified.
"""
import argparse
import importlib.util
import json
import hashlib
import sys
sys.dont_write_bytecode = True
from pathlib import Path
from pypdf import PdfReader, PdfWriter
from pypdf.generic import NameObject, ArrayObject

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
args.output.mkdir(parents=True, exist_ok=True)
spec = importlib.util.spec_from_file_location('forms_smoke', root/'scripts/smoke-viewer-forms.py')
forms = importlib.util.module_from_spec(spec)
spec.loader.exec_module(forms)
template = args.output/'template.pdf'
forms.form_pdf(template)
manifest = []
for book, page_number in [('obp.0526',332),('hefferon-toc',100),('openstax-biology-2e',170)]:
    source = root/f'tests/render_corpus/books/{book}.pdf'
    reader, field_reader = PdfReader(source), PdfReader(template)
    writer = PdfWriter()
    page = writer.add_page(reader.pages[page_number-1])
    acroform = field_reader.trailer['/Root']['/AcroForm'].clone(writer)
    writer._root_object[NameObject('/AcroForm')] = acroform
    annots = ArrayObject([a.clone(writer) for a in field_reader.pages[0]['/Annots']])
    page[NameObject('/Annots')] = annots
    for annot in annots:
        annot.get_object()[NameObject('/P')] = page.indirect_reference
    target = args.output/f'{book}-{page_number}.pdf'
    writer.write(target)
    manifest.append({'book':book,'page':page_number,'fixture':str(target),'source':str(source),'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'fixture_sha256':hashlib.sha256(target.read_bytes()).hexdigest()})
(args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(args.output/'manifest.json')
