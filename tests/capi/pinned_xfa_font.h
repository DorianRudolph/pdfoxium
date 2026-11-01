// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

#pragma once

#include "test_support.h"

#include <gtest/gtest.h>

#include <array>
#include <fstream>
#include <iterator>
#include <string>

namespace pdfoxium::integration::forms {

// Complete, unmodified pinned Arimo-Regular.ttf embedded as PDF objects
// 11 (TrueType font), 12 (descriptor), 13 (FontFile2). The caller may alias
// object 11 as /Helv, /Helvetica and /Arial in AcroForm/DR/Font so XFA's
// requested field/calendar families resolve through embedded-font lookup.
//
// Metadata is derived from that file (SHA-256 eafef8c99e94d10f17506c125e24d98
// a84256e0e665e6c659498eca96b19e148), using only its SFNT tables:
//   head: unitsPerEm=2048, bbox=[-1114 -621 2666 2007].
//   hhea: ascender=1854, descender=-434; OS/2: sCapHeight=1409.
//   cmap Windows Unicode BMP format 4 maps Unicode to glyph IDs;
//   hmtx supplies each glyph's advance width (last long metric repeated).
// The 224 widths below cover WinAnsi character codes 32..255, using pinned
// cpdf_fontencoding.cpp::kAdobeWinAnsiEncoding for their Unicode mapping.
// Widths use fx_font.cpp::NormalizeFontMetric: truncate(value*1000/upem+.5).
// Descriptor dimensions preserve the exact 1000/upem-scaled source values.
// StemV=70 is the native fpdf_edittext.cpp regular-font embedding heuristic;
// it is advisory PDF metadata rather than a measured Arimo stem width.
// Font provenance and the original license are in resources/test_fonts/README.md.
inline std::array<std::string, 3> PinnedXfaArimoFontObjects() {
  std::ifstream file(TestRoot() / "tests/capi/resources/test_fonts/Arimo-Regular.ttf",
                     std::ios::binary);
  EXPECT_TRUE(file.good());
  const std::string bytes((std::istreambuf_iterator<char>(file)),
                          std::istreambuf_iterator<char>());
  EXPECT_EQ(bytes.size(), 436180u);
  constexpr char kWidths[] =
      "278 278 355 556 556 889 667 191 333 333 389 584 278 333 278 278 "
      "556 556 556 556 556 556 556 556 556 556 278 278 584 584 584 556 "
      "1015 667 667 722 722 667 611 778 722 278 500 667 556 833 722 778 "
      "667 778 722 667 611 722 667 944 667 667 611 278 278 278 469 556 "
      "333 556 556 500 556 556 278 556 556 222 222 500 222 833 556 556 "
      "556 556 333 500 278 556 500 722 500 500 500 334 260 334 584 350 "
      "556 350 222 556 333 1000 556 556 333 1000 667 333 1000 350 611 350 "
      "350 222 222 333 333 350 556 1000 333 1000 500 333 944 350 500 667 "
      "278 333 556 556 556 556 260 556 333 737 370 556 584 333 737 552 "
      "400 549 333 333 333 576 537 333 333 333 365 556 834 834 834 611 "
      "667 667 667 667 667 667 1000 722 667 667 667 667 278 278 278 278 "
      "722 722 778 778 778 778 778 584 778 722 722 722 722 667 667 611 "
      "556 556 556 556 556 556 889 500 556 556 556 556 278 278 278 278 "
      "556 556 556 556 556 556 556 549 611 556 556 556 556 500 556 500 ";
  return {
      std::string("<< /Type /Font /Subtype /TrueType /BaseFont /Arimo "
                  "/Encoding /WinAnsiEncoding /FirstChar 32 /LastChar 255 "
                  "/Widths [") + kWidths + "] /FontDescriptor 12 0 R >>",
      "<< /Type /FontDescriptor /FontName /Arimo /Flags 32 "
      "/FontBBox [-543.9453125 -303.22265625 1301.7578125 979.98046875] "
      "/ItalicAngle 0 /Ascent 905.2734375 /Descent -211.9140625 "
      "/CapHeight 687.98828125 /StemV 70 /MissingWidth 750 "
      "/FontFile2 13 0 R >>",
      PdfStream("/Length1 " + std::to_string(bytes.size()), bytes)};
}

}  // namespace pdfoxium::integration::forms
