# Interactive form ownership

A live form session aliases one environment per document. It keeps the document
alive; the document holds only a weak session reference. Focus, selection,
widgets, scripts and timers belong to that environment, not independent page
snapshots. Form initialization, input and teardown can synchronously reenter;
release internal borrows before host callbacks.

The static render pool is unsuitable for unsaved interactive state: each worker
has a different document. The form worker owns one actual document, environment,
JavaScript context and all loaded form pages on one thread. Input, layout,
visibility actions, timers, rendering and saving queue to that owner. Detached
snapshots/bitmap results can cross the boundary; live form handles cannot.

## Painting and input

Form overlays paint after page content with the same full-page viewport/matrix.
Dirty rectangles come from real invalidation callbacks. A widget hover is not
itself a request to redraw the page. Preserve caret/selection damage, close popup
areas and old/new widget bounds; use fresh rendered pixels to validate patches.

Keep metadata refresh separate from paint requests. Refreshing every visited
page's field snapshot on every key adds work unrelated to the changed widget.
Likewise, requiring an input acknowledgement and a second queued paint request
adds a UI frame. Input and its resulting damage can be processed in one owner
turn while retaining ordinary asynchronous UI delivery.

Pointer/key/text input must reach the actual SDK/FWL control path, including
capture, focus, selection, list opening and release outside controls. Logical
focus changes must blur old edits and kill their caret timers. Static field-value
editing alone does not implement interactive form behavior.

## JavaScript and host services

Enable execution explicitly with `FormOptions.javascript`; the viewer opts in.
Alerts/prompts use a synchronous reply bridge to UI modals. No runtime/document
borrow may span that callback. Shutdown cancels pending replies before joining
the owner, avoiding deadlock on a dialog whose UI is going away.

The owner dispatches document initialization, page/annotation visibility actions,
field events and timers in one realm. Observe script change generations as well
as change callbacks so dirty state reflects scripted edits and explicit resets.
Action execution order matters for formatting/calculation (for example currency
display) and annotations controlling animations.

Submission, URL, linked-file and network requests are embedder services; keep
policy in the host adapter and preserve cancellation/errors. The interpreter
cannot silently substitute a native runtime or a second script realm. See
[runtime reentry](javascript-runtime.md) for the context lease.

## XFA

XFA uses this same owner for model changes, dynamic pagination, widgets and
rendering. Page counts and layouts can change after input; refresh geometry and
resolve current XFA pages rather than treating them as fixed PDF page indices.
Static PDF editing/outline tools need appropriate XFA routing or an explicit
unsupported result. See [the XFA boundary](xfa.md).
