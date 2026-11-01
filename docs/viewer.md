# Viewer architecture

The [viewer README](../crates/pdfoxium-viewer/README.md) covers launch and app
control. The viewer is a demo client of the safe engine; core rendering and PDF
ownership must remain independent of egui, wgpu and the control protocol.

## Threads and invalidation

Static documents use independent render workers over shared immutable bytes,
plus a reader worker. Forms/XFA instead keep input, JavaScript, timers, layout,
editing and rendering on one live document owner. Separate rendering copies
cannot reproduce that owner's unsaved form state.

Work carries source/view generations. Ignore late results after source, geometry,
zoom or rotation changes; cancel obsolete groups and remove their in-flight tile
entries before replanning. Previous zoom textures may remain as backdrops while
current renders arrive. Navigation must update layout/scroll and backdrop
placement together to avoid a frame painting old textures at target coordinates.

Use actual form invalidation rectangles for patches. Avoid repainting for hover
without visible damage or refreshing all visited field snapshots on every key.
Process input and its dirty paint in the same owner turn when possible. Ordinary
UI acknowledgement plus a second render request adds an avoidable frame.
Animation thumbnail refreshes keep the old texture until a replacement is ready.
Unsaved-departure dialogs suppress animated background churn.

## Rendering and textures

The policy uses physical **monitor** area, with window area only as a startup
fallback: enter tiled mode above 4× area when less than 75% of the page is visible;
leave below 3× area. Retain the mode between thresholds. GPU texture dimension
limits override it. Tile grids and request merging stay in the safe library.

Current-resolution page origins snap to physical pixels after scrolling, and
textures use their exact integer bitmap dimensions. Selection, annotation and
form geometry must use that same displayed transform. During zoom gestures,
old textures can stretch until exact-resolution replacements arrive. Accumulate
long-document layout heights in double precision and preserve a page-local zoom
anchor, rather than a fraction of total document height.

Render results are straight-alpha RGBA. Opaque rows copy directly into the final
egui pixel allocation; transparent rows use egui's premultiplication semantics.
Whole-page results can use a contiguous copy. A byte allocation cannot safely
become an owned `Vec<Color32>` because allocation alignment differs. Do not add
an atlas or custom uploader without measurements separating render time, CPU
preparation, upload staging and presentation.

## Text and annotations

Search begins at the current page, publishes local matches early, then scans
with wraparound. Typing updates highlights without navigating; explicit next/
previous commands choose a destination in physical document order.

Selection display uses loose font-metric bounds and merges adjacent runs into
line bands. Copy text retains extracted Unicode/order. Markup annotations likewise
need line/run grouping: per-glyph ink boxes create disjoint underline/strikeout
segments. Preserve separate lines/columns, rotation and partial selections.

Edits use live page/object/annotation handles and document revisions. Preserve
appearance resources and regenerate the actual appearance, rather than painting
an overlay that disappears on save. FreeText requires a usable font/default
appearance; pages with unrelated form resources must not accidentally select
missing fonts. Shape/text drag handles and sidebar hit regions must not compete
with the page scrollbar for layout or pointer ownership.

## Direct app control

Control commands and toolbar/keyboard actions share the same command handler.
The socket thread only authenticates, bounds and queues requests; GPU/application
state stays on the UI thread. Tests inject ordinary egui input and capture the
real framebuffer. Control-only input prevents unrelated desktop events from
changing tests. See the reusable `scripts/smoke-viewer*.py` tests and the browser
smoke bridge; screenshots still need visual inspection.
