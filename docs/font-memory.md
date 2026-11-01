# Font retention limits

Each library instance shares two budgets across all fonts, documents and pages:

| Budget | Default | Retained contents |
|---|---:|---|
| Glyph cache | 64 MiB | CfxGlyphCache bitmaps, outline paths, widths, including failed lookups |
| Glyph scratch | 4 MiB | Idle geometry, rasterizer, coverage and LCD row buffer capacities |

Zero disables retention for that budget. `usize::MAX` in Rust or `UINT64_MAX`
in C means unlimited. Rendering still works when retention is disabled or an
individual result/workset is larger than its budget; it is simply not retained.
Limits are library configuration, independent of render flags.

Controls are available through the safe API's `Config`, core font-memory
controller and additive `PDFOXIUM_*` C extensions in
[the public header](../crates/pdfoxium-capi/include/pdfoxium.h). They target the
initialized calling thread's library instance. Initialization preserves existing
limits; destruction/reinitialization restores defaults. Separate instances have
independent controllers. The image realization threshold is a separate policy.

## Eviction, accounting and exceptions

Cache pressure evicts the least recently used **font cache as a whole**, including
its bitmap, path and width maps. Cache hits update recency. An oversized entry is
returned uncached without evicting useful entries to accommodate it. Whole-font
eviction avoids a second per-glyph indexing structure but can make small budgets
less efficient. Negative entries consume budget too.

Cache accounting charges owned bitmap/palette and path **Vec capacities**, object
and `Rc` metadata, and a conservative full BTreeMap-node allowance per entry
(`11 * size_of::<(K, V)>() + 16 * size_of::<usize>()`). This covers sparse nodes
without depending on private standard-library layouts, and intentionally charges
more than densely filled maps usually allocate. Usage is a policy ledger, not a
heap-profiler estimate. Cached results are treated as immutable; caller-retained
or caller-mutated results are outside the retention contract.

Scratch accounting sums actual Vec capacities for the outline pen, kurbo
expansion input, AGG vertices, curve subdivision points, the rasterizer's
cells/sort indices/rows, scanline coverage/spans and LCD row. Kurbo's returned
expanded outline is an active working buffer, converted into the reusable pen
and dropped before the scratch workset returns; it adds no retained cache.
Checkout removes the workset's idle charge; return
checks its current capacity against the current shared budget. Idle worksets are
evicted in least recently used order. A workset too large to retain is dropped
on return, including failed/early-return glyph renders. Ordinary worksets retain
their capacities for reuse; no buffer copying is needed to check them out.

Lowering limits reclaims idle retention immediately. Raising limits permits
future reuse. Explicit trim discards idle caches and scratch without changing
limits; any scratch already checked out at trim is discarded on return. Results
already held by callers remain valid after eviction or trim. Dropping a font or
cache removes its charge. The controller keeps only weak resource registrations.

**These limits bound accounted retained font memory, not peak memory or RSS.**
Active glyph rasterization and returned results may temporarily allocate more
than the budgets. Outstanding `Rc` results can outlive eviction. Fixed per-face,
controller and weak-index metadata, allocator headers/rounding/fragmentation,
font files, Fontations font/hinting metadata, parsed PDF objects/streams, image
caches, page bitmaps and viewer tiles are outside these budgets. Allocators may
keep freed pages mapped; trim does not call `malloc_trim` or promise an RSS drop.
A viewer must separately bound page/tile/image retention and close documents.
The CID Type 1 adapter borrows binary font data (hex input is decoded once) and
retains decoded local subroutines as font metadata. Encrypted glyph charstrings
are decrypted only during an active draw and then freed; they are not cached
separately. Its glyph paths, bitmaps and rasterization scratch use the same
accounted retention limits as other fonts.
Type 3 character procedures and source images are document resources. The
transformed Type 3 bitmap cache is held only during active text rendering in
the current pipeline (the document index holds weak references), so it does not
add idle zoom retention to these budgets.

## Diagnostics

`pdfoxium-diff memory` reports accounted budgets separately from Linux RSS and
allocator observations. `--trim-fonts` releases idle font retention after a pass;
`--trim` is a diagnostic glibc trim after teardown. Neither is an RSS guarantee.
The differential harness alone accepts `PDFOXIUM_TEST_CACHE_BYTES` and
`PDFOXIUM_TEST_SCRATCH_BYTES`; the production renderer does not read these
variables. See [benchmark methodology](../benchmarks/README.md).
