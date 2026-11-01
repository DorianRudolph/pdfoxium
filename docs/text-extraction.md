# Text extraction and query boundary

`fpdftext::CpdfTextPage` follows the pinned page-text algorithm: object ordering,
font Unicode mapping, geometry, spacing and line breaks, ActualText, duplicate
suppression, bidi segments, normalization and character/text index conversion.
Construction borrows a parsed page. Results retain small object metadata and font
handles rather than cloning page objects or keeping Rust references into them.
The orientation projection uses merged intervals instead of allocating a bitmap
proportional to an untrusted page dimension.

Text searches preserve the source token/whitespace and overlap strategy. Whole
word boundaries recognize ASCII letters, digits and ligatures; punctuation is
not accidentally classified as a letter by the source's open ASCII ranges.
Backward searches use text indices consistently. Unicode lowercase uses existing
Rust character support and keeps one scalar per source index.

Search preparation borrows the original full-text slice and fuses surrogate-pair
decoding with case folding into one owned search buffer. Ordinary text keeps an
implicit identity index map; a source-span vector is allocated only when a
surrogate pair actually collapses two source entries. Start lookup is constant
time for identity maps and uses sorted partition points for explicit maps.
This preserves starts inside pairs, source character ranges and forward/backward
semantics without caching normalized page text or changing snapshot ownership.

Web-link detection retains the native URL/email tables and first-match rules.
A linear offset map keeps source character ranges correct when controls, CRLF,
wrapper prefixes or a hyphen marker are removed while assembling the URL.

The C ABI owns text snapshots and keeps searches/links alive independently of
their text-page handle. Text-object aliases resolve through the live page's
existing identity cache; a closed page cannot supply an object alias. Buffer
capacities and UTF-16/byte units follow each public function's contract. Saving,
rendering and JavaScript callers use the same parsed page/font machinery.

The legacy `FPDFText_GetText` C function specifically returns UCS-2: non-BMP
scalars are omitted, existing surrogate units pass through, and a terminator
fits within `count + 1` slots without dropping following text. This does not
restrict the safe Rust API's Unicode snapshots or bounded UTF-16 extraction.
Native search can miss supplementary characters stored as paired units even
when raw extraction agrees. Preserve the safe API's complete Unicode and source
ranges; do not confuse the bulk UCS-2 omission with that search defect. See
[benchmark guidance](../benchmarks/README.md).
