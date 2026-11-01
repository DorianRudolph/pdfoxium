# Optional content (PDF layers)

The implementation follows PDF Reference 1.7 §4.10, using the pinned
`CPDF_OCContext`, render-status, annotation and SDK action callers as the
implementation guide. The reference is available from
[Adobe](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf).
This extends PDFium's immutable render context with transient viewing state;
[known native differences](capi-known-native-differences.md#optional-content-defaults-usage-and-single-group-policies)
record the spec corrections.

## Ordinary use

Existing rendering calls are unchanged. Opening a PDF evaluates `/OCProperties/D`;
selecting a configuration or toggling a group changes the current view shared by
cloned `Document` handles. These operations do **not** edit the catalog, increment
the document's edit revision, mark it dirty, or change what save/reopen restores.
They require the same owner-thread access as other document operations and return
`Busy` while a progressive render holds a page lease.

```rust,no_run
# use pdfoxium::{Document, RenderConfig, Result};
# fn example(document: &Document) -> Result<()> {
let layers = document.optional_content()?;
for group in layers.groups()? {
    if group.name == "Roads" && !group.locked {
        layers.set_visible(group.id, false)?;
    }
}
let bitmap = document.page(0)?.render(&RenderConfig::dpi(144.))?;
# Ok(())
# }
```

No-OCG documents expose an empty group list and lazily cache that absence.
Names are decoded PDF text strings; identity is an opaque document-scoped ID,
not a display name. IDs from another open document are rejected.

## Independent views and workers

`OptionalContentState` is an independent, `Send` viewing context. Its immutable
`OptionalContentSnapshot` is `Send + Sync`; states and snapshots contain no
`Rc`, parser handles, SDK widgets or browser objects. Their group/configuration
IDs retain their original namespace.

```rust,no_run
# use pdfoxium::{Document, RenderConfig, OptionalContentEvent, Result};
# fn example(document: &Document) -> Result<()> {
let mut view = document.optional_content_state()?;
if let Some(group) = view.groups().first() {
    view.set_visible(group.id, false)?;
}
let config = RenderConfig::dpi(144.).optional_content(view.snapshot());
let bitmap = document.page(0)?.render(&config)?;
let print_config = RenderConfig::dpi(144.)
    .print(true)
    .optional_content(view.snapshot_for(OptionalContentEvent::Print));
# Ok(())
# }
```

Snapshots freeze the chosen event and environment. A print render without an
explicit snapshot evaluates Print usage automatically; when supplying a frozen
snapshot, choose `snapshot_for(Print)` explicitly. Export works the same way.
Neither event changes the current View state.

Worker documents may consume a snapshot only when opened from the **same
`Arc<[u8]>` allocation**, with the same optional-content catalog generation.
Unrelated byte allocations, stale catalog generations and foreign document IDs
return errors. Ordinary form edits do not revoke source identity. A catalog edit
requires a fresh context; a worker pool must be reloaded from newly saved bytes
before consuming the edited catalog.

`RenderPool::optional_content_state()` supplies a compatible context.
`set_optional_content(snapshot)` changes the default for **future** submissions;
existing tickets keep their captured snapshot. The caller cancels obsolete
requests and invalidates pixel caches. The viewer does this for full pages, tiled
regions, pending form paints and thumbnails, including hidden/offscreen pages.
It keeps previous page and thumbnail textures visible until replacements arrive,
including during repeated toggles. Replaced tiles are released individually so
the transition does not temporarily double the page texture cache.
Layer changes retain worker documents, parsed pages, fonts and decoded images.
Embedded `/Thumb` images are bypassed for layered documents because they cannot
represent the selected state. Reader hit-query results carry an epoch so an old
visibility result cannot repopulate a new view's link/annotation cache.

The form-owner thread publishes `DocEvent::OptionalContent` after PDF actions or
context updates. Its render generation changes independently of the saved PDF
revision. The UI consumes that state without echoing a command back to the owner.

## Configurations and rendering rules

- `/D` is the opening configuration; `/Configs` are explicit alternatives.
  BaseState, ON, OFF and Unchanged are evaluated in order. Alternate Order and
  RBGroups inherit from `/D` when absent.
- Intent matching is independent of View/Print/Export events. Nonmatching groups
  do not hide content. `/Usage` metadata affects state only through a matching
  `/AS` application. Categories and matching applications combine by AND.
- Automatic View/Print/Export, zoom intervals `[min,max)`, language matching
  (exact before preferred partial), and user conditions use `UsageEnvironment`.
  The viewer supplies its logical zoom and host/browser language. Embedders can
  supply user context or disable automatic usage evaluation.
- Interactive toggles respect Locked and radio groups, including locked peers.
  They override subsequent automatic View changes until reset/configuration
  selection. Explicit programmatic changes and PDF `SetOCGState` actions bypass
  Locked; PreserveRB controls radio handling. Action ON/OFF/Toggle order is kept.
- `/Order` labels/nesting control presentation, not inherited visibility.
  Unlisted groups are not added to the panel. `/ListMode /VisiblePages` filters
  presentation using groups actually referenced by visible pages' parsed marks,
  form/image XObjects and annotations, including OCMD policies and VE operands.
- Nested marked content combines visibility; OCMD AnyOn/AllOn/AnyOff/AllOff and
  nested And/Or/Not expressions are evaluated with bounded recursion. Hidden
  painting still applies clipping, graphics state and text positioning.
- Form/image XObjects and annotation `/OC` conditions use the same context.
  Annotation flags continue to apply. Hidden SDK annotations are excluded from
  hit testing, drawing, keyboard input and tab traversal. Form field metadata
  reports the effective layer visibility.

Malformed/unregistered groups retain the existing PDFium-compatible fallback;
invalid configurations, foreign IDs and invalid zoom environments return errors.
Order/expression traversal is bounded against malformed or cyclic structures.
The Acrobat JavaScript `getOCGs` reader-UI no-op remains the pinned upstream
behavior; the implemented control surface is the Rust API and PDF actions.

## Explicit PDF edits

`layers.create_group(name)` registers an indirect OCG, makes it initially visible
and appends it to the default Order. `Page::set_object_layer` assigns/removes a
single OCG on a **top-level** object; non-OC marks remain intact. Its named
Properties entry preserves indirect group identity when content is regenerated.
`Page::set_annotation_layer` assigns/removes annotation `/OC` membership.
Existing nested marks, OCMDs and XObject memberships are preserved on ordinary
save/reopen. These edit APIs use the normal edit/lease/form-session exclusions.

`layers.save_default(&state)` explicitly persists an opening configuration and
marks the PDF dirty. It retains presentation, intent, locks, radio groups and
Print/Export applications, and removes View automatic applications so reopening
restores the saved visibility. It does not run during ordinary toggles.

Page import, N-up and page-to-XObject import copy optional-content catalog data
through the same reference map as resources. An empty destination retains the
full source catalog. Import into a layered destination adds registered groups,
default state and presentation metadata; imported alternate configurations use
Unchanged for existing destination groups. Same-named groups remain distinct.
Self-contained imported Form XObjects can retain OC properties during insertion;
structure-tagged content still requires the established tag-rebinding path.

## Viewer and validation

Native and WASM share the same toolbar: **Layers** appears only when groups are
present, with checkboxes, locked states, named configurations and Reset layers.
The embedded **A map with layers** sample has two pages, four groups, a locked
guide, a second configuration and a clickable SetOCGState action. Reproduce it
with `scripts/make-layers-demo.py` using ReportLab and pypdf.

Validation includes core usage/radio/language/event tests; safe API raster,
identity, snapshot, save/reopen, authoring, membership, clipping, imports and
form-owner action tests; and native/Rust GoogleTest graphics-state comparisons
with all four writer/reader save combinations. The sample is rendered through
both engines. Native and WASM walkthroughs exercise actual dropdown input,
configuration/reset, multiple pages and editor-worker transitions; the WASM
walkthrough additionally asserts changed/restored map pixels and covers all six
samples, file upload, drops, downloads, search and real browser-window resizing.
See `scripts/smoke-viewer-layers.py` and `scripts/smoke-viewer-wasm.js`.
