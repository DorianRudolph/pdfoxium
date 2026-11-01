# XFA ownership, layout and input

XFA uses the same PDF engine, Boa runtime and AGG renderer as ordinary documents.
Its portable XML/CSS, locale/picture, FGAS/FDE text, FWL widgets and barcode paths
are Rust implementations. Native page models are adapted to safe owners, not a
second rendering engine.

## Node representation

A generic factory carries the exact constructor/schema metadata for concrete
XML-creatable classes: packet masks, object types, child properties and typed
attributes/defaults. Many native classes are constructor-only wrappers; generic
Rust nodes represent them without inventing empty class modules. Method-bearing
classes and script inheritance still require their actual specialized behavior.
Generated tables and dispatch live beside `xfa_basic_data` and parser sources.

The document owns a shared allocation store and XML owners. Tree links,
back-pointers and public node references are weak. Lists/locales retain store
tokens, and cross-document snapshots retain each member's store. This preserves
identity and detached-node/XML lifetime without document cycles. It can retain
an entire allocation store longer than tracing would retain individual nodes;
it is an ownership tradeoff, not an implicit cache or a cloned tree.

Tree/attribute changes release internal borrows before notifications. Pin the
allocation store across callbacks, then recheck the committed relationship/value
before follow-up XML changes. Invalid parentage, foreign anchors and cycles reject
before detaching nodes. A matching prototype child retains identity and gets a
template link; cloning must preserve explicit-versus-inherited values and XML
mapping, rather than materializing every inherited attribute.

Data binding, repeated instances, remerge and layout are identity-sensitive.
Retiring a fragment must detach prior-pass child links and widget resources;
reusing a layout item cannot leave stale generated overflow paint or binding
edges. Saved unbound form values may intentionally return to template defaults;
live UI state is not automatically a serialization contract.

## Layout and painting

FGAS/FDE retain PDFium's text classification, Arabic forms, bidi ordering, plain/
rich text breaking and baseline policy. Coordinate and numeric operation order
matter; ordinary PDF text must not be reshaped through this XFA layout path.
Text edits commit replacements and history atomically after synchronous change/
validation callbacks, including rewritten ranges and cancellation.

Layout boxes are not paint clips. In particular, clipping every text run to its
nominal line box cuts descenders and synthetic style extents. Apply the caller's
actual clip to painting; use line metrics for layout/hit testing. This distinction
was confirmed by the calendar descender regression and remains tested in the
public API suite. Larger-resolution screenshots can expose clipping hidden at
72 DPI, but font-coverage differences still need independent diagnosis.

Styles, measurements and picture formats retain native unit conversion and f32
rounding. Locale selection distinguishes missing, unqueried and explicitly empty
configuration. Host clocks/timezones must remain explicit; formatted display
text and underlying field data are separate values.

## Widgets and callbacks

The FWL app/note driver owns the widget tree and dispatches synchronous messages
and events. Rust observers/weak edges replace C++ unowned pointers; listeners and
owner links must be removed during logical teardown. End borrows before focus,
selection, validation, layout and theme callbacks. A closing environment may be
invalidated during those callbacks even while an active owner keeps memory alive.

Composite controls share their actual edit/list/caret owners. Focus changes must
blur the previous edit and remove its timer, not merely change the outer widget's
focus ID. Pointer capture and release-outside behavior matter for dropdowns,
radio buttons and text selection. Layout fragments share the field node's image,
physical DPI and model state; they must not create independent form values.

The viewer uses a single live XFA owner for input, layout, timers and rendering.
After dynamic pagination, refresh page geometry and route through current XFA
page identities. Static PDF navigation/editing tools cannot be blindly applied
to dynamic layout pages. Native and browser builds use the same worker boundary.

See [forms](rust-api-forms.md), [runtime reentry](javascript-runtime.md),
[font policy](fonts.md), and [native-oracle differences](capi-known-native-differences.md)
for deliberate compatibility departures and independent regression assertions.
