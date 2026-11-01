# Incremental PDF loading

Pinned source `core/fpdfapi/parser/cpdf_data_avail.{cpp,h}` maps to
`cpdf_data_avail.rs`; `cpdf_hint_tables.{cpp,h}` maps to
`cpdf_hint_tables.rs`. The existing read validator, cross-reference scanner,
object/page-object scanners, syntax parser, linearized header and document
loader remain the actual dependencies.

The state machine checks the PDF header and linearization metadata, validates
and loads cross references, then visits the root/info dictionaries, page tree,
requested page objects, inherited resources and AcroForm. Page and shared-object
hint bit tables supply actual file ranges, lengths and object numbers. Missing
ranges reach the embedder's download callback. Full-file requests occur only
in the native error/rebuild fallback paths; they do not replace incremental
availability. Owned queues and visited sets move between polls without cloning
whole cross-reference maps or page graphs.

`pdfoxium-capi/src/dataavail.rs` implements all eight native public entry points:
Create/Destroy, IsDocAvail, GetDocument, GetFirstPageNum, IsPageAvail, IsFormAvail
and IsLinearized. Callback layouts/status values match the public header. The
operation moves out of its handle slot before callbacks; its retained owner
survives synchronous destruction, while nested operations reject the occupied
slot. Parse errors and unsupported-feature notifications use the shared SDK
paths. Parser readers snapshot their safe owners before I/O so the outer
document can be queried during input callbacks.

The document link is weak. Per-document object/resource/form scans clear on
expired-owner access, including an immediate reparse after document destruction.
Cached resource keys retain the corresponding object to prevent allocation-ID
reuse. Page traversal uses explicit bounded frames and cycle detection instead
of host recursion.

The port corrects three native edge cases: nested page selection counts actual
leaf pages rather than comparing a global page index to local child counts;
a missing page Count transitions immediately into tree scanning instead of
reporting a wait without a download request; truncated shared-hint signature
flags/counts reject safely before indexing. These preserve the intended
availability operations and meaningful malformed-input failures.

All three native hint-table declarations retain complete fixture tables and
assertions. Additional tests cover real incremental document/page/form/resource
requests, nested/missing-count page trees, owner expiry/reparse, callbacks and
C ABI operation teardown. Integrated runtime checks have passed, including
the nested/missing-count page-tree correction and expired-owner reparse cleanup.
The [checkpoint validation record](../TODO.md) tracks the complete workspace
runs and affected-test reruns supporting the current result.
