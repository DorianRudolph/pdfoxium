# JavaScript runtime and callback ownership

Boa is the production JavaScript engine; V8 is only part of the native test
oracle. PDFium's SDK, field events, calculations, timers and XFA bindings adapt to
one actual Boa context per document runtime. See [compile features](compile-features.md)
for disabled execution and [forms](rust-api-forms.md) for host integration.

## Synchronous reentry

PDF scripts call the embedder, and an embedder callback can synchronously call
back into the same document's script engine. Queueing those calls or evaluating
in a second realm would change validation/calculation order and object identity.
Do not hold a `RefCell` borrow over an operation that can execute user script.

`CfxJsEngine::with_context` takes the context out of its available slot and
releases that slot's borrow. RAII restores it on return/error. A Boa native
function already receives `&mut Context`; before entering PDF-core/embedder code
that can reenter, `with_host_callback` temporarily moves that actual context,
including suspended VM frames, into the available slot and puts an inert context
in the suspended borrow. Nested calls lease the same actual context. Returning
restores it before Boa resumes. Inert placeholders are pooled by callback depth
and never execute PDF scripts.

This is safe ownership transfer, not a raw context pointer or a cloned object
graph. The same boundary covers script-origin field setters and actual public
C callbacks. Event/document/widget borrows must end before entering it. RAII
restores ownership during unwinding, but does not promise recovery from arbitrary
interpreter panics.

## Values and lifetimes

- Keep Boa values/objects rooted across coercion, evaluation and nested host
  calls. Native object data must trace actual GC edges. Use safe function-pointer
  callbacks or Copy-only dispatch captures, never unsafe closures hiding roots.
- Use Boa's coercion/property operations; values cross the boundary as UTF-16,
  including NULs and lone surrogates. A Rust `String` is not a lossless substitute.
- Retain opaque thrown values. Convert an exception to an SDK error while the
  actual context/event scope is active. Event changes made before a throw still
  write back; guards restore nested event and duplicate-field state.
- Bindings resolve weak environment links and observed field/node identities.
  Retaining a script object does not keep a logically closed page/widget alive.
  Coercion may run script, so recheck logical lifetime afterward.
- Detachment removes the environment's runtime owner and shuts down App timers
  before the embedder Release callback. Active calls retain enough ownership to
  unwind; later host access rejects the detached environment. GC finalizers must
  not call PDF embedder services.

## Events, delayed mutations and timers

The document owns typed pending field mutations. Extract a queue before applying
it, preserving native order without a queue borrow spanning formatting,
invalidation or callbacks. Script-global objects keep real Boa property identity,
getters, descriptors and cycles; weak persistence delegates cannot create runtime
ownership cycles. Cached intrinsic operations must not depend on script-modified
`Reflect` functions.

Timers retain their originating runtime/event data. Kill/teardown removes their
ownership before invoking reentrant host services. Timer callbacks use the same
context lease and event guards as ordinary actions.

The safe form layer explicitly enables JavaScript with `FormOptions.javascript`;
the viewer enables it. C ABI hosts retain PDFium's platform opt-in. An embedder
can configure Boa's loop limit; the viewer's limit is not a wall-clock deadline
or general cancellation mechanism. Script errors use the normal error channel.

Page visibility dispatch includes annotation `/PO`, `/PV`, `/PI` and `/PC`, which
are needed by hidden animation controllers. The pinned public PDFium page-action
entry point only invokes page dictionary `/O` and `/C`; the safe wrapper connects
the existing annotation event kinds as an intentional extension.

## FormCalc and XFA

FormCalc is translated by its lexer/parser/AST/emitter and executed in the same
Boa boundary. Locale/picture, date/time, SOM, node-expression and host-service
operations must preserve native coercions, errors and event scope. Network,
submission and linked-resource requests go through explicit owner services;
there is no hidden native-runtime fallback.

XFA variables keep scoped contexts and their real owner model. Node bindings
must retain store identity and dynamic-property semantics while resolving weak
native owners; do not create a second disconnected XFA object graph. See
[XFA ownership and layout](xfa.md).

Document JavaScript word extraction differs from the full text-page API: it
visits active top-level text objects in content order, using PDFium's word
classification rather than geometric page-text normalization. Preserve this
boundary when sharing helpers.
