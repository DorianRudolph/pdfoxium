# Safe API ownership and reentry

Public examples are in the [crate README](../crates/pdfoxium/README.md). The
wrapper retains core identity and callback ordering without moving the engine
to a global arena or copying page trees to obtain exclusive access.

## Pages and identities

A page retains its document and shared logical page state. The document's page
table retains that state without a strong backlink, avoiding a cycle. Logical
state and the optional parsed core page are separate: clones alias the same
state, and `unload()` releases the parsed page/image cache for every alias while
keeping its identity. Dropping one API handle does not evict the document cache;
font/document resources have independent lifetimes.

Page IDs include a non-reused document identity and monotonic slot, independent
of page order. Moving/importing pages updates indices without replacing surviving
states; deletion invalidates them. IDs survive unload but do not identify a
reopened copy of the document. Iterators capture a revision and reject edits as
`Stale`, preventing silently mixed document versions.

Text snapshots own immutable character/text data and small font metadata, so
unload or document drop does not invalidate their contents. They remain
thread-affine. Plain read models and rendered bitmap data can be independent
transferable snapshots. Search geometry is explicit work, not eagerly computed
for callers that only need matching text/ranges.

## Edits and failure

Object slots identify live core boxes during page residency. Reordering preserves
identity; removal, unload/flatten/deletion, ancestor removal and cross-page moves
invalidate old object handles. Moves return a destination identity. Annotation
IDs retain dictionary identity and validate membership in the current array.
Live handles retain their parents; they do not cache unobserved core pointers.

Nested form edits detach each ancestor's serialized storage once per loaded
instance, preserving live child identities without modifying sibling instances
of a shared form. Shared content-mark changes must dirty all owning objects and
ancestors. Annotation appearance handles share one parsed context, while the
page keeps only weak registrations; edits outside that context conservatively
invalidate it.

Regenerate dirty content before save, unload or import. Generation failure keeps
the loaded state so work is not silently discarded. No fallible save/generation
belongs in `Drop`. The lazy input must remain readable; writing over its source
file can corrupt later reads.

There is no general transaction/undo guarantee. A core mutation or page import
can leave partial changes on failure. Update identity/index bookkeeping from
actual changes and conservatively advance revision/dirty state. Do not turn an
error into a claim that nothing changed. Cross-document and tagged-object moves
need resource/structure remapping and are rejected by the current wrapper.

## Leases, callbacks and threads

A non-clonable page lease reserves loading, rendering, editing or unloading;
conflicts return `Busy`. It owns a handle, not a long-lived `RefMut`. A job drops
its core context/readers before releasing that lease. Synchronous rendering uses
the same job with no pause requests.

A document call guard rejects reentry while core work is active. Unwinding
through core marks the document `Poisoned`; later engine operations reject it.
Progressive steps release document/buffer borrows and the call guard before the
pause callback. That callback can use another idle page, while the rendered page
remains leased. Callback panics outside core do not themselves poison core state.

Documents, pages, form sessions and jobs remain `!Send + !Sync`. Moving bytes or
owned pixels across threads does not make their live engine owners transferable.
Use [independent render workers](render-pool.md) for static concurrency and a
[single live owner](rust-api-forms.md) for interactive forms.
