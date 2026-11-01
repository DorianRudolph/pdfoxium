# JPEG codec snapshot

Source: <https://github.com/developer0hye/libjpeg-turbo-rs/tree/b614c77936e56e6409c06af87fb104a5e75e64ee>.
Original package: libjpeg-turbo-rs 0.8.0, MIT OR Apache-2.0.
Packaged here as `pdfoxium-jpeg` 0.1.0 to retain the evaluated fixes when
building from Git or a registry without a workspace patch or Git dependency.

The production `src/` tree retains the upstream implementation. A local
`unused_mut` guard follows both the x86_64 and SIMD conditions of the encoder
fast path, keeping scalar x86_64 builds warning-free without changing runtime
behavior. The manifest selects the same
runtime dependencies/features; upstream tooling, C reference implementations
and integration campaigns are omitted. Three small fixtures referenced by
in-source unit tests are retained. The libjpeg test image is from
<https://github.com/libjpeg-turbo/libjpeg-turbo> (`testimages/testimgari.jpg`);
the derived progressive image is its upstream re-encoding. See the IJG and
libjpeg-turbo notices in `LICENSE-REFERENCE.md`.
