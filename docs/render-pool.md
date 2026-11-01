# Render workers and tile planning

The engine remains single-threaded. Each static render worker opens its own
`Document` and owns its pages, parser, fonts, caches and render contexts. Only
immutable `Arc<[u8]>` input, requests and owned bitmap results cross threads.
Interactive forms instead need their [live document owner](rust-api-forms.md).
See [the safe API README](../crates/pdfoxium/README.md#static-render-pool) for use.

## Scheduling and cancellation

Page-index modulo worker count gives fixed affinity. There is no work stealing:
sparse selections may leave workers idle, but repeated requests retain page/cache
locality. Priority ordering is Visible, Prefetch, Thumbnail, Background, with
FIFO selection within a priority. Queues are bounded.

Preemption happens at existing core pause points. A paused job for another page
can retain progress. For the same page, drop its old context/lease before starting
a new one; requeue the old request to restart later unless cancelled/superseded.
Never construct another context while the page remains leased.

A newer queued request can supersede an older request only for the same page,
identical render settings, sufficient priority and containing region. Cancel
predecessors only after the covering render job is successfully created; an
invalid clip must not discard valid work. Equal-priority arrival alone does not
supersede an active job.

Cancellation is checked even after the last render step so completion cannot
leak a cancelled result. Release queue/completion locks before invoking wakers.
Worker panic completes its tickets with `Poisoned` and closes its queue. Pool
drop cancels and joins; a waker dropping its own pool must not self-join.
Reload/source generations prevent old completed work from becoming current.

## Tile planning

Plan on the full rotated/scaled device canvas. A clipped request keeps that same
scale and translates the device origin; never fit the page to the tile dimensions.
The grid merges missing horizontal runs and matching consecutive rows without
including cached/in-flight tiles. This is a simple rectangle cover, not a global
minimum-rectangle solver.

Tile IDs contain row/column only. Namespace availability by source identity/
generation, page, complete render configuration and grid settings. Dimensions
alone do not distinguish rotation, background, annotations, pixel format or edits.
Old zoom backdrops must not satisfy availability for a new configuration.

Mark tiles in flight on submission, clear them on cancellation/failure and mark
cached only after successful upload. Remove old in-flight entries before
replanning. Merged results expose source subrectangles; preserve row stride when
uploading them without allocating a temporary buffer for every tile. Cache
budgets, overscan and gesture policy belong to the viewer, not the planner.
