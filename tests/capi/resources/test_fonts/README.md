# Pinned PDFium test fonts

These unmodified test-only fonts reproduce the mapping in PDFium's
`testing/test_fonts.cpp`: Times maps to Tinos, Arial/Helvetica/Calibri to Arimo,
and Courier to Cousine. Each family has regular, bold, italic and bold-italic
faces. They are not production dependencies or fallbacks.
The scoped public C API provider restores each library's owned default provider
after the fixture documents have been destroyed. Font bytes are loaded lazily
and shared between both providers and later scenarios; enumeration alone does
not read the twelve font files.

PDFium's pinned `DEPS` selects Chromium `third_party/test_fonts` revision
`7f51783942943e965cd56facf786544ccfc07713`. Its `test_fonts.tar.gz.sha1` is
`cd96fc55dc243f6c6f4cb63ad117cad6cd48dceb`; the source archive is
<https://storage.googleapis.com/chromium-fonts/cd96fc55dc243f6c6f4cb63ad117cad6cd48dceb>.
The complete archive is not included. Only the three Latin families and the
Arabic fixture below are retained, without renaming or modifying tables.

Arimo, Tinos and Cousine are Copyright 2010-2012 Google Corporation and licensed under Apache
License 2.0. Their original embedded notices remain intact.
`LICENSE.chromium` is the complete license file from that exact source revision;
its Apache section explicitly lists all twelve Latin files. The other fonts/licenses it
mentions are not redistributed here, apart from DejaVu Sans documented below.
No extra font decoder or Python package is needed by the C++ test runner.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| Arimo-Bold.ttf | 438,516 | `89a9a8f88a1da4cc2a96238a19239ef85e12205ea06122e226dd36932cbd482e` |
| Arimo-BoldItalic.ttf | 435,028 | `35c0e8abf09b2f8855bb8e90ffbd5bee362c19f12a159298053ecb493332fe9a` |
| Arimo-Italic.ttf | 440,740 | `edcc229e781249b3f623061f5ac3f94b5fa6b880c85876f73b09b99b23ab6223` |
| Arimo-Regular.ttf | 436,180 | `eafef8c99e94d10f17506c125e24d98a84256e0e665e6c659498eca96b19e148` |
| Cousine-Bold.ttf | 296,624 | `42d14baaf1802f8edf7659346a8574b41c4c78e264eb81cdb0891ca6686354ce` |
| Cousine-BoldItalic.ttf | 272,700 | `ef65b8624e7993977feb9f9ba18f514a1fa9adc4e1c4737c8839574685c188ae` |
| Cousine-Italic.ttf | 269,840 | `9b2b98e5d389fe74968af2abb440ac4b6fcd24e2a1aba5ed895b0337b6b0b5e6` |
| Cousine-Regular.ttf | 308,360 | `69e1ea59eb770014204e5174f805750f9a793db4a2531e6516b30b7460d470b3` |
| Tinos-Bold.ttf | 451,516 | `6a0afe87068ba5ddeb6a9bb7b9d0dc41666966c044767df69de3efb7e33f5af8` |
| Tinos-BoldItalic.ttf | 456,592 | `52c4d87a988eb1f1c25e1fcb97970d456a2fb252e0fa2116e2b4bb33e7824bda` |
| Tinos-Italic.ttf | 455,400 | `751c979043c9641dad389ab9c680c2c9ffb0a0c7352153f48b9ebd958e8963d8` |
| Tinos-Regular.ttf | 469,968 | `ef438a1c6449f1e38e2fb50768440d1c3e489fd9db3862ff479180291fec5b29` |

DejaVu Sans is embedded directly by the Arabic XFA tests. It comes from the same
verified archive, supports basic Arabic, contextual presentation forms, all four
Lam/Alef pairs, joining controls and ordinary combining marks, and is never used
as a production fallback. The font has no FC5E–FC62 combined-shadda Unicode
entries; tests do not pretend that missing-glyph rectangles cover those forms.
Its original Bitstream Vera/DejaVu notices are embedded in the font and included
in `LICENSE.chromium`. Outlines, glyph IDs, metrics and naming are unmodified.

| Arabic fixture | Bytes | SHA-256 |
| --- | ---: | --- |
| DejaVuSans.ttf | 757,076 | `7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954` |

Archive SHA-256: `ec973eccd669d417fbf2d0a4dac730744a3174268ff062db5451b55c82bc3492`.

The XFA date-picker, initialization/validation and fixed calendar/time-picture
scenarios also embed the unchanged Arimo file. `pinned_xfa_font.h` supplies its
source-derived WinAnsi widths and descriptor, exposed under Helvetica/Arial
AcroForm resource aliases. An enumerating version-1 scoped provider supplies
theme aliases. See [fixture design and validation](../../README.md).
