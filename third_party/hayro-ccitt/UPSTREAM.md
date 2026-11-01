# hayro-ccitt local extension

Upstream: https://github.com/LaurenzV/hayro, crate hayro-ccitt 0.4.0.
Upstream revision: ea9c81dc70cde40c64205986fdcfcd481fe934d8 (hayro-ccitt).
Original crate checksum:
`2866f0d976e76497480b853d5e258dab6e4b89ecfe991124e08f15dfa8fd03ad`.

Sources are copied from the locked crates.io release. Original MIT and Apache
licenses and source notices are retained. PDFium-derived extensions retain the
BSD notices and LICENSE-PDFIUM.

The pdfium module translates core/fxcodec/fax/faxmodule.cpp's selected packed-row
control flow, keeping the planned safe Rust codec's CCITT run-code state tables.
The default upstream API remains available. Windows fax encoding is deferred.

Native extensions retain partial rows and exact bit offsets, EOL scans, reference
row updates before output inversion, mutable byte alignment and its rewind state.
All 208 white/black run codes match the pinned native instruction tables. Invalid
prefixes consume through the native maximum length, without opposite-color
fallback. The G4 helper supports native JBIG2 image widths above 65535. Input size
and signed run overflows fail safely where native CHECK/undefined behavior applies.

G4Decoder exposes the same native rows and ending bit position to JBIG2's packed
word storage using two row buffers, avoiding an image-sized conversion copy.

FaxDecoder can retain the accessor's exact Rc-backed source allocation when it
outlives the initial stream borrow. Changing ownership moves row/reference buffers
and preserves packet position and alignment state without copying source bytes.

G4 vertical mode computes only the reference b1 endpoint; pass mode still
computes b1/b2. Edge-byte filling combines the native subtraction loop into
wrapping mask subtraction, preserving interior-byte zero filling. FindBit
retains the native nonnegative-start invariant. Focused tests cover reference
transitions, partial-byte padding, clipping and already modified destination bytes.

Packaged as `pdfoxium-ccitt` 0.1.0. The distinct
package name preserves the upstream identity above while making the maintained
PDFium extensions available to downstream consumers without workspace patches.
