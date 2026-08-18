# Phase 0 performance baseline

Recorded on 2026-08-10 using the optimized, ad-hoc-signed application bundle.

| Property | Value |
| --- | --- |
| macOS | 26.6.1 (25G76) |
| Architecture | Apple Silicon (`arm64`) |
| UI toolkit | Iced 0.14.0 |
| Renderer | `iced_wgpu` → wgpu 27 → Metal |
| Bundle executable | 6.4 MiB, stripped |
| Idle RSS samples | 91,760 KiB; 92,192 KiB; 91,840 KiB |
| Median idle RSS | 91,840 KiB (89.7 MiB) |
| Idle CPU samples | 0.0%; 0.8%; 0.0% |
| Median idle CPU | 0.0% |

## Build and renderer evidence

The release build uses default-disabled Iced features with `wgpu`, `basic-shaping`, `thread-pool`,
and the codec-free `image-without-codecs` path enabled. `cargo tree -p despatch -e features`
resolves the rendering path through `iced_wgpu`, wgpu's Metal feature, and the `metal` crate. It
does not enable Iced's `tiny-skia`, `webgl`, `unconditional-rendering`, or PNG/JPEG/GIF/TIFF image
codecs.

`otool -L target/release/Despatch.app/Contents/MacOS/Despatch` lists
`Metal.framework`, `QuartzCore.framework`, and `CoreVideo.framework`. The release window must render
successfully before the measurement is accepted; renderer initialization failure exits the app
with an error.

## Measurement method

For each sample:

1. Launch `target/release/Despatch.app` with `/usr/bin/open`.
2. Wait five seconds for window and renderer initialization to settle.
3. Read RSS and CPU with `ps -o rss=,%cpu= -p <pid>`.
4. Send the app a normal quit event with AppleScript using bundle identifier
   `com.despatch.app`.
5. Confirm the process exits within five seconds before starting the next sample.

The runtime smoke test also queried `CGWindowListCopyWindowInfo` and found an on-screen layer-zero
window owned by `Despatch`. Core Graphics reported 1860 × 1049 physical pixels on the scaled
display, corresponding to the configured 1120 × 720 logical window.

This is a scaffold baseline, not a permanent budget. Later phases compare the same release-build
measurement and explain material regressions. Debug builds are not comparable.

## Phase 4 comparison

Measured after the application shell, presenters, adaptive tile grid, platform wiring, and native
accessibility bridge were added:

| Property | Phase 4 value | Change from Phase 0 |
| --- | --- | --- |
| Bundle executable | 6.9 MiB, stripped | +0.5 MiB |
| Idle RSS samples | 93,712 KiB; 93,616 KiB; 88,144 KiB | — |
| Median idle RSS | 93,616 KiB (91.4 MiB) | +1,776 KiB (+1.9%) |
| Median idle CPU | 0.0% | unchanged |

The release bundle rendered and remained idle during all samples. The small increase covers the
domain/application/config graph plus AppKit accessibility metadata; it is not a material regression
against the Phase 0 baseline. Iced remains event-driven with no frame subscription or unconditional
render loop.

## Glass and native-icon comparison

Measured after enabling the system backdrop blur, translucent structural chrome, and three cached
SF Symbol textures:

| Property | Current value | Change from Phase 4 |
| --- | --- | --- |
| Bundle executable | 7.2 MiB, stripped | +0.3 MiB |
| Idle RSS samples | 94,944 KiB; 90,048 KiB; 94,368 KiB | — |
| Median idle RSS | 94,368 KiB (92.2 MiB) | +752 KiB (+0.8%) |
| Median idle CPU | 0.0% | unchanged |

The three decoded 48 × 48 RGBA symbol buffers total 27 KiB. The measured memory change is small,
and the UI remains idle at 0% CPU because blur is supplied by the macOS compositor and icons are
uploaded once rather than redrawn by a subscription.
