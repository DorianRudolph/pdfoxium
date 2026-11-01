# The Right to Privacy - first two pages

Samuel D. Warren and Louis D. Brandeis, *The Right to Privacy*, Harvard Law
Review 4(5), December 15, 1890, pp. 193-220
([bibliographic record](https://www.jstor.org/stable/1321160)). This fixture
contains only the opening two pages, journal pages 193-194. The 1890 US
publication is public domain in the United States; see the
[US Copyright Office's explanation](https://www.copyright.gov/what-is-copyright/).
The article is not relicensed under this project's software license.

The DjVu source is [Right to Privacy.djvu on Wikimedia Commons](https://commons.wikimedia.org/wiki/File:Right_to_Privacy.djvu),
which identifies the authors, original journal and scan provenance.
The user supplied `Right_to_Privacy.pdf`, a 28-page DjVu conversion produced
with [djvu2pdf2smol](https://github.com/dorianrudolph/djvu2pdf2smol)
(`djvu2pdf 0.1.0`). It preserves searchable OCR text and uses layered JBIG2
images. The two-page excerpt was extracted with pypdf without rasterizing or
reencoding the images: page boxes, content streams, OCR text, encoded JBIG2
streams and Poppler-rendered pixels are unchanged. The full supplied PDF is not added.

Source SHA-256: `246c6db238834b01a63671d8b98321f0b22e9b56f6456404c5220fe532ab2970`.
This compact real-world fixture exercises DjVu-derived, segmented JBIG2 scan
rendering and OCR extraction.

Rendering regression: pinned native PDFium renders this scan very pale because
its optimized JBIG2 refinement decoder resets a valid vertical reference offset,
causing the dark masks to fail decoding. pdfoxium preserves that offset and now
renders the original streams correctly. The [focused investigation](../../../docs/capi-known-native-differences.md#valid-jbig2-refinement-displacements)
records independent pixel checks, the decoder fix and the converter workaround.
The fixture keeps its original encoded streams to expose the native defect.
