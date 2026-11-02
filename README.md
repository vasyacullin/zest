# Zest 🍋

Zest is an early-stage Rust GUI engine project. The intended architecture combines
an immediate user API, a persistent internal ECS scene and direct GPU rendering
through `wgpu-hal`, targeting desktop and mobile applications and tools.

**Current implementation includes a headless scene and an experimental HAL quad demo.**
The scene provides scoped widget identities,
validation of scene/subtree declarations before mutation, persistent ECS nodes
and interaction flags, plus typed coordinate spaces and DPI conversion.
The separate GPU slice opens a window and renders one colored quad. Scene widgets
are not yet connected to rendering; layout, text and texture sampling are still
planned. See [plan.md](plan.md) for the full roadmap and acceptance criteria.

## Run

Install Rust 1.95 or newer, then run from this repository root:

```sh
cargo run -p zest-headless
cargo run -p zest-quad
cargo run -p zest-quad -- --smoke
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

The demo reorders keyed buttons, then partially updates their container while
retaining a button's focus/hover and a status sibling. Explicit keys are required
in this initial API; automatic call-site keys are planned.

## Experimental GPU slice

`zest-quad` uses `winit` and direct `wgpu-hal`: Vulkan on Windows/Linux, Metal on
Apple targets. The default `zest` facade stays headless; enable its `gpu` feature
to use `zest::gpu::{Renderer, Quad}`. HAL, Naga and wgpu-types are pinned together
at 30.0.1; the high-level `wgpu` crate is not used.

The demo renders a quad at a fixed logical-pixel rectangle and changes its color
on hover. Resize and DPI events update the physical surface/geometry. Normal
operation uses an event-driven `Wait` loop, without continuous redraw requests.
Close the window or press Esc to exit. Linux needs a Vulkan driver/loader and
X11 or Wayland; CI installs the native development libraries listed in its
workflow. Windows needs a working Vulkan driver. There is no DX12 fallback yet.

The renderer validates WGSL through Naga and checks adapter limits, sRGB format,
blending, copy support and presentation capabilities. It owns two frame slots
with reusable staging/vertex buffers, explicit usage barriers, noncoherent memory
flushes and a shared monotonically increasing submission fence. Command buffers
and target views are reclaimed only after completion. Resize and shutdown wait
for rendering and presentation before releasing resources. Zero-size surfaces
remain unconfigured and submit no work. The renderer stays on its creation
thread, owns the window through `Arc`, and exposes no native GPU handles.

`--smoke` presents 20 frames, requests a window resize, tests zero-size
suspend/resume, then reads two offscreen renders back from the GPU. Eighteen
pixel samples check fractional DPI, bounds/orientation, padded row copies, sRGB
encoding and opaque/premultiplied-alpha blending (two byte tolerance). To save
the opaque offscreen image as PPM:

```sh
cargo run -p zest-quad -- --smoke --capture target/quad-smoke.ppm
```

`Renderer::capture` provides synchronous packed sRGB RGBA8 readback for tests and
screenshots. Ordinary rendering never reads back pixels. Smoke fails on renderer
or validation errors; Vulkan loader diagnostics are printed and counted
separately because stale system layer manifests can produce loader errors even
when presentation succeeds. Validation messages depend on installed platform
validation layers; passing smoke does not prove device-loss recovery or complete
HAL correctness.

All unsafe operations are confined to `zest-gpu/src/backend.rs`, with documented
invariants and Clippy checks for safety comments. Core, facade and demos retain
their unsafe-code prohibition. If GPU completion cannot be established during
shutdown, the owner retains native resources until process exit rather than
free storage that might still be in use. Terminal HAL failures require recreating
the renderer. General resource handles, an upload ring, texture sampling, backend
fallback and device recovery remain future work; this is an initial HAL slice.

## Scene updates

`Scene::update` declares the complete scene. `Scene::update_children` declares all
children of an existing container, recursively, without rebuilding its ancestors
or sibling branches:

```rust
use zest::Scene;

let mut scene = Scene::default();
let mut panel = None;
scene.update(|ui| {
    panel = Some(ui.container("panel", |ui| {
        ui.button("save", "Save");
    }));
    ui.button("status", "Ready");
}).expect("unique widget keys");

scene.update_children(&panel.unwrap(), |ui| {
    ui.button("save", "Save changes");
}).expect("valid panel and unique widget keys");
assert_eq!(scene.stats().nodes, 3); // status survives
```

The partial builder starts inside the target container's identity path. Repeat
only scopes declared inside it; do not redeclare the target. Omitted descendants
are removed, so an empty builder clears the container. Retained widgets keep
their ECS entities and interaction state; removing the focused widget clears
focus. Unknown targets, buttons used as targets, duplicate IDs and declarations
outside the target subtree are rejected before scene mutation.

`roots()` and `children(&id)` expose declaration order without exposing ECS types.
Child lookup returns `None` for an unknown ID and an empty slice for an existing
empty node. The retained hierarchy lets partial reconciliation walk only the
old and new descendants of the chosen container. Updates remain synchronous
and explicit; there is no automatic dependency tracking or layout engine yet.

## Coordinates

`zest::geometry` separates logical window pixels, physical device pixels and
canvas document coordinates at compile time:

```rust
use zest::geometry::{Logical, Point, Rect, ScaleFactor};

let scale = ScaleFactor::new(1.25).expect("valid DPI scale");
let bounds = Rect::<Logical>::from_xywh(0.0, 0.0, 100.0, 40.0);
assert!(bounds.contains(Point::<Logical>::new(10.0, 5.0)));
let physical = bounds.to_physical(scale);
assert_eq!(physical.size.width, 125.0);
```

Coordinates use `f64`. DPI conversion preserves fractional pixels; platform
adapters will choose the rounding policy for integer surface sizes. Scale factors
must be finite and positive. Canvas pan/zoom will use a separate transform.
Rectangle containment includes minimum edges and excludes maximum edges;
intersection returns `None` for empty, touching or disjoint bounds. Bounds with
nonfinite coordinates, sizes or maximum edges are treated as empty.

## Reconciliation baseline

```sh
cargo bench -p zest-core --bench reconciliation --locked
cargo bench -p zest-core --bench reconciliation --locked -- --smoke
```

The full benchmark runs 1,000 and 10,000 keyed buttons under one container. It
measures unchanged declarations, keyed reversal, one changed label and alternating
insertion/removal. Each scenario warms up for 10 updates, then reports nearest-rank
p50/p95/p99 over 100 samples. Timing includes the immediate builder, string and ID
allocation, duplicate validation and ECS reconciliation; key preparation and
correctness checks run outside the timed section. This measures complete scene
updates even when one label changes, rather than partial rebuild performance.

Every iteration checks mutation counts, retained focus/hover, node count and stable
archetype count. The smoke mode uses 100 buttons, two warmup updates and four
samples to exercise these checks in CI; its percentiles are not performance data.
The harness uses the standard library and adds no dependencies. Record the CPU,
OS, Rust version and power/build settings alongside results when comparing runs.
`FullSidebar` and `PartialSidebar` also compare the same one-button panel beside
the large list: both change its label on every update, using a complete scene
declaration or `update_children`, respectively. The partial path retains the
large list and validates the panel's identity, label and hierarchy after each
sample. Partial updates still rebuild the entire selected subtree; changing one
button in a flat list requires declaring that list's other children.

These timings cover CPU reconciliation only; frame time, GPU work, allocation
counts and idle wakeups still need separate measurement. CI has no timing gate.

CI is configured for formatting, Clippy, tests, the headless demo and benchmark
smoke checks on Windows, Linux and macOS. Linux additionally runs the quad/pixel
smoke under Xvfb with Mesa software Vulkan. A separate Linux job tests minimum
Rust 1.95. Local GPU verification currently covers Windows/Vulkan; Linux/Vulkan
and macOS/Metal still need runtime verification on those platforms. CI configuration is
not a report of a successful remote run.

## Structure

- Root package `zest`: public facade.
- `crates/zest-core`: private ECS implementation and public scene API.
- `crates/zest-gpu`: experimental safe wrapper around direct HAL rendering.
- `demos/headless`: runnable reconciliation example.
- `demos/quad`: window, quad, hover, resize and GPU readback smoke example.
- `examples`: external reference checkouts, excluded from the workspace.

The default scene API forbids unsafe code. The HAL crate has its own narrowly
scoped unsafe policy and documented invariants; see the
[HAL contracts](https://docs.rs/wgpu-hal/30.0.1/wgpu_hal/).

## License

Licensed under Apache-2.0; see [LICENSE](LICENSE).
