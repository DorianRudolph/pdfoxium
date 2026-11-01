# Progressive rendering

A render job retains its page, target bitmap and core context across steps.
The target is initialized once. Continuing advances retained renderer/codec state;
it must not replay already committed rows, restart the page or blend the same
object twice. The synchronous API drives this same job to completion.

Core pause adapters are queried at source-defined boundaries. A request to pause
is cooperative, not interruption of arbitrary code or JavaScript. Never keep
interior borrows or a queue lock across an embedder pause callback. Public jobs
retain their page lease across pauses while releasing temporary call guards.

Cancellation/close drops the render context before its readers and page lease.
A fresh job can then render the same page. It must not inherit a partially
composited bitmap or a poisoned pending codec result. Cache entries retain
successful source identity, not raw addresses of freed streams; progressive
image/global dictionaries and converted planes have separate owners.

Owned and borrowed output buffers share checked width/height/stride validation;
padding must remain untouched. A borrowed target's lifetime belongs to the job,
whereas the page/document retention is owned. A completed bitmap is independent
of its rendering owners.

Public C API scenarios exercise actual continuation, cancellation/restart,
reentrant callbacks, cached and cold image paths. Different native/Rust pause
cadences are valid. Compare final output and state, not an implementation-specific
number of callbacks. The pinned native C wrapper requires a live non-pausing
callback for uninterrupted continuation; the Rust API also accepts its documented
null pause path. See [known native differences](capi-known-native-differences.md).
