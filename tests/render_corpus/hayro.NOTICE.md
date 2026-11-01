# Hayro corpus catalog provenance

`hayro.json` is a normalized copy of the five rendering manifests in
[LaurenzV/hayro](https://github.com/LaurenzV/hayro/tree/73d59bc6f39d094564c65510a090bc14d497edbe/hayro-tests),
revision `73d59bc6f39d094564c65510a090bc14d497edbe` (2026-10-21).
The manifests are used under Hayro's MIT license; see
[HAYRO-MIT.txt](licenses/HAYRO-MIT.txt).

| Source manifest | Entries | PDFs hosted in Git | PDFs hosted on hayro-assets.dev |
|---|---:|---:|---:|
| manifest_custom.json | 311 | 286 | 25 |
| manifest_pdfjs.json | 679 | 0 | 679 |
| manifest_pdfbox.json | 454 | 0 | 454 |
| manifest_pdfium.json | 127 | 0 | 127 |
| manifest_corpus.json | 41 | 0 | 41 |
| Total | 1612 | 286 | 1326 |

Identifiers are prefixed by `hayro/<group>/`. `file` entries become pinned raw
GitHub download URLs; linked entries use Hayro's asset URLs. Source comments
(including the `comments`/`command` spelling variants) become `notes`; upstream
`ignore` becomes informational `upstream_ignored`. The source's string-valued
`link: "true"` is normalized as a linked PDF. Passwords and zero-based inclusive
first/last-page bounds are preserved. No PDF content is converted or modified.

The repository-hosted rendering fixtures total 22,746,010 bytes. Hayro's Git
PDF tree also contains load, text and other fixtures that are outside these
rendering manifests. They are not automatically imported as rendering cases;
load tests can intentionally accept parse failures and exercise malformed data.

The catalog's source revision pins the manifests and Git-hosted PDF URLs.
Hayro asset URLs are unversioned; existing local downloads are reused until an
explicit `fetch --refresh`. No content-hash infrastructure is added. Updating
this catalog means reviewing the five manifests at a new revision, preserving
metadata, recording that revision, and refreshing affected reference images.

The PDFs originate from Hayro fixtures, public issue trackers, pdf.js, PDFBox,
PDFium and a selection of the PDF Association's large-scale corpus. Individual
PDFs retain their original rights and credits; neither the Hayro license nor
this project's license relicenses their content. Downloaded PDFs and rendered
artifacts remain local and Git-ignored. This repository distributes the catalog
and source attribution, not the external PDFs.
