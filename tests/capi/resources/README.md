# C API test font

`hebrew_unicode.ttf` is a test-only derivative of the embedded Noto Sans Hebrew
subset in PDFium's pinned `testing/resources/hebrew_truetype_font.fragment`.
The original font is Copyright 2015 Google Inc. and licensed under the SIL Open
Font License 1.1; see [OFL.txt](OFL.txt). Its copyright, authorship and license
metadata remain in the derived font. The modified family is named
`Pdfoxium Hebrew Test` / `PdfoxiumHebrewTest-Regular`.

The original subset has only a MacRoman cmap using codes 1–5. Its separate PDF
`hebrew_cmap.fragment` maps those codes to Hebrew. PDFium's XFA font lookup uses
the font's own Unicode cmap, so embedding the unchanged subset does not provide
its five Hebrew glyphs to XFA. The derivative adds a Windows Unicode BMP cmap
using exactly the existing PDF ToUnicode mapping:

| Unicode | Original glyph ID | Advance /1000 em |
| --- | ---: | ---: |
| U+05DF |1|294|
| U+05D9 |2|235|
| U+05DE |3|645|
| U+05E0 |4|397|
| U+05D1 |5|542|

All six glyph IDs, including `.notdef`, their compiled outlines/instructions,
and their advances are verified unchanged. Original MacRoman cmap entries are
retained. Only Unicode addressability, family/unique/PostScript names and
necessary font checksums change. No synthetic glyphs or character assignments
are introduced.

Regenerate from the repository root with developer-only FontTools 4.61.1:

```sh
python -m pip install fonttools==4.61.1
python tests/capi/resources/generate_hebrew_unicode.py
python tests/capi/resources/generate_hebrew_unicode.py --check
```

Neither Python nor FontTools is a production or C++ test-runner dependency.
The script fixes timestamps by retaining the pinned input's values and validates
the resulting Unicode-to-glyph mapping and original glyph content.

- Decoded pinned source SHA256:
  `30965aa1b56395591372f0d173d32d5c3e5a7fb78a6bb0f85df3929e017691c6`
- Derived fixture SHA256:
  `7a16b22b901094f59fb4fea19588ebe71f74697d7aeffdf67dfcf7f860f83a46`
- Derived size: 6840 bytes.
