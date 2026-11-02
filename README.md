# Zest 🍋

Zest is an early-stage Rust GUI engine project. The intended architecture combines
an immediate user API, a persistent internal ECS scene and direct GPU rendering
through `wgpu-hal`, targeting desktop and mobile applications and tools.

**Current implementation includes a headless scene and an experimental HAL quad demo.**
The scene provides scoped widget identities,
validation of scene/subtree declarations before mutation, persistent ECS nodes
and interaction flags, plus typed coordinate spaces and DPI conversion.
The separate GPU slice opens a window and renders ordered, clipped quad batches. Scene widgets
are not yet connected to rendering; layout and text are still
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

`zest-quad` uses `winit` and direct `wgpu-hal`: DX12/Vulkan on Windows, Vulkan on Linux, Metal on
Apple targets. The default `zest` facade stays headless; enable its `gpu` feature
to use `zest::gpu::{Renderer, Quad}`. HAL, Naga and wgpu-types are pinned together
at 30.0.1; the high-level `wgpu` crate is not used.

The demo renders overlapping quads and a checker texture with rectangular clips and changes the
left quad's color on hover. Resize and DPI events update surface/geometry. Normal
operation uses an event-driven `Wait` loop, without continuous redraw requests.
Close the window or press Esc to exit. Linux needs a Vulkan driver/loader and
X11 or Wayland; CI installs the native development libraries listed in its
workflow. Windows needs a working DX12 or Vulkan driver.

`RendererOptions::default()` uses `Backend::Auto`: DX12 then Vulkan on Windows,
Vulkan on Linux, Metal on Apple. Auto tries the next API if initialization fails;
`renderer.backend()` reports the chosen API and `renderer.backend_failures()`
retains earlier failures with their initialization stage. Explicit backend choices
return an error if unavailable or unsupported on the target, without falling back.
Selection happens once at creation; it does not recover a device lost during rendering.
The demo accepts the same choices through `--backend`:

```sh
cargo run -p zest-quad -- --backend dx12 --smoke
cargo run -p zest-quad -- --backend vulkan --smoke
```

In Rust, pass `RendererOptions { backend: Backend::Vulkan, ..Default::default() }`
to `Renderer::with_options`. DX12 uses HAL's default automatic shader compiler
selection, including the system FXC fallback when DXC is unavailable
([HAL compiler options](https://docs.rs/wgpu-types/30.0.1/wgpu_types/enum.Dx12Compiler.html)).

The renderer validates WGSL through Naga and checks adapter limits, sRGB format,
blending, copy support and presentation capabilities. It owns two frame slots
with reusable staging/vertex buffers, explicit usage barriers, noncoherent memory
flushes and a shared monotonically increasing submission fence. Command buffers
and target views are reclaimed only after completion. Resize and shutdown wait
for rendering and presentation before releasing resources. Zero-size surfaces
remain unconfigured and submit no work. The renderer stays on its creation
thread, owns the window through `Arc`, and exposes no native GPU handles.

`Renderer::render_list` expands the list to ordered triangles and merges adjacent
commands using the same texture. Solid-only lists use one draw; interleaved
quad/image/quad commands retain their order and use three draws. Each frame slot grows its buffers to a
power-of-two capacity (bounded by adapter limits), retaining capacity for reuse.
Growth waits for that slot's fence and resets resource usage tracking. Empty
lists clear the target without allocating/uploading vertices or issuing a draw.
`last_frame_stats()` reports rectangle primitives (including images), encoded draw calls, uploaded vertex bytes and the
selected slot's vertex capacity; these are CPU counters, not GPU timestamps.

`--smoke` presents 20 frames, requests a window resize, tests zero-size
suspend/resume, then reads offscreen renders back from the GPU. Eighteen
pixel samples check fractional DPI, bounds/orientation, padded row copies, sRGB
encoding and opaque/premultiplied-alpha blending (two byte tolerance). Additional
checks render 129 quads in one draw, test nested clips and reversed painter order,
grow/reuse buffers, and check every pixel in a clear-only capture. The first two
onscreen frames test empty slots before their first vertex buffer allocation.
Image checks cover padded RGBA upload, UV crop/orientation, clipping, tint/alpha,
material order, adjacent batching, stale/foreign handles and fence retirement.
Smoke uses a 4 KiB texture upload ring, uploads 64 retained images through repeated
wraps, verifies every image in a later grid, and checks oversized dedicated
staging followed by another ring upload. To save
the demo display list rendered offscreen as PPM:

```sh
cargo run -p zest-quad -- --smoke --capture target/quad-smoke.ppm
```

`Renderer::capture` and `capture_list` provide synchronous packed sRGB RGBA8 readback for tests and
screenshots. Ordinary rendering never reads back pixels. Smoke fails on renderer
or validation errors; Vulkan loader diagnostics are printed and counted
separately because stale system layer manifests can produce loader errors even
when presentation succeeds. Validation messages depend on installed platform
validation layers; passing smoke does not prove device-loss recovery or complete
HAL correctness.

All unsafe operations are confined to `zest-gpu`'s backend module tree, with documented
invariants and Clippy checks for safety comments. Core, facade and demos retain
their unsafe-code prohibition. If GPU completion cannot be established during
shutdown, the owner retains native resources until process exit rather than
free storage that might still be in use. Terminal HAL failures require recreating
the renderer. Texture atlases and device recovery remain future work;
this is an initial HAL slice.

## Display lists and rectangular clipping

`zest::paint` is available without the GPU feature. `DisplayList` is an immutable
paint snapshot; `Painter` validates geometry/colors/UVs, preserves declaration order,
intersects nested clips and omits fully clipped or transparent primitives:

```rust
use zest::{geometry::Rect, paint::{DisplayList, Quad}};

let list = DisplayList::build(|p| {
    p.quad(Quad {
        bounds: Rect::from_xywh(0.0, 0.0, 100.0, 80.0),
        color: [1.0, 0.0, 0.0, 1.0],
    })?;
    p.with_clip(Rect::from_xywh(20.0, 20.0, 40.0, 40.0), |p| {
        p.quad(Quad {
            bounds: Rect::from_xywh(0.0, 0.0, 100.0, 80.0),
            color: [0.0, 0.0, 1.0, 0.5],
        })
    })
}).expect("valid paint data");
assert_eq!(list.len(), 2);
```

Pass the snapshot to `Renderer::render_list` or `capture_list`. Single-quad
`render`/`capture` calls and the `zest::gpu::Quad` import remain available.
Nonfinite clips are errors; empty/negative clip extents hide their child scope.
Invalid quad input is rejected even when clipped out. A returned clip-scope error
rolls back its commands, and a failed list builder exposes no partial snapshot.
Child painters preserve the parent's clip without a mutable global clip stack.
Clipping trims axis-aligned rectangles and adjusts image UVs on the CPU; rounded clips,
transforms, text and automatic extraction from the ECS scene remain
future work.

## Image resources

With the `gpu` feature, `Renderer::upload_rgba8(width, height, bytes)` accepts
top-to-bottom straight-alpha sRGB RGBA8 data and returns an `ImageId`. Use
`Painter::image(Image::new(id, bounds))` to include it in a display list.
`Image::uv` is a normalized rectangle within [0, 1]; `tint` is straight-alpha
linear RGBA. Sampling currently uses nearest/clamp-to-edge with one mip level.
The shader decodes sRGB through the texture format and premultiplies after
sampling. Bilinear filtering, image-file decoding, atlases and async loading
are not implemented yet.

Upload row pitch and source offsets honor adapter alignment; padding is
initialized, noncoherent memory is flushed, and texture usage transitions from
UNINITIALIZED through COPY_DST to RESOURCE. Uploads and rendering share a
queue/fence, so a draw observes earlier uploads without a whole-device idle wait
per image.

Texture staging uses one bounded, reusable upload ring (4 MiB by default).
`Renderer::with_options` accepts `RendererOptions { upload_ring_capacity, ..Default::default() }` for
a different power-of-two capacity, at least 256 bytes and within adapter limits.
Only successfully submitted ranges consume ring space. Completed fence values
reclaim ranges; if the ring is full, allocation waits for the oldest submitted
range. Wraps never overwrite pending bytes. A single upload larger than the ring
uses a dedicated staging buffer retained by its frame slot until completion and
recycling. The ring itself remains alive through shutdown after GPU idle.

On coherent memory, CPU writes and pending GPU reads use disjoint protected
ranges. On noncoherent memory, writes conservatively wait for all previous ring
reads before mapping/flushing, because HAL does not expose the memory atom size.
Mapping guards unmap on normal return and error/unwind. Vertex staging continues
to reuse the separate per-frame buffers.

`upload_stats()` reports cumulative ring allocations/uploads, staged bytes
(including row padding), wraps, fence-wait calls, dedicated allocations and
coherence. Pending bytes are CPU-tracked submitted ranges, which may have already
completed on hardware; `wait_idle()` reclaims them. Counters include the internal
white texture upload. Ring capacity bounds shared texture staging, not total
renderer memory or the oversized fallback allocations.

`remove_image(id)` invalidates the handle immediately and queues native resources
for retirement after their last upload/draw submission completes. Later rendering,
captures and uploads collect completed entries; `wait_idle()` drains retirement
after waiting, and shutdown releases all owned resources. `image_stats()` counts
live user images and retired entries, excluding the internal white texture.
Stale handles, handles from another arena/renderer, and wrong resource kinds
cannot alias an image after slot reuse. Display lists with stale/foreign handles
fail before GPU submission and do not make the renderer terminal. Recreating a
renderer requires reuploading images; old IDs belong to its old resource arena.

The underlying `zest::resource::{ResourceArena, ResourceId}` is also available in
headless builds. It provides typed, arena-local generational identities; its
owner controls actual destruction. Resource marker types prevent mixing kinds
at compile time; exhausted generations retire slots instead of wrapping.

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
Rust 1.95. Local GPU verification covers Windows/DX12 and Windows/Vulkan, including
the default Auto path and identical offscreen demo pixels between the two APIs. Linux/Vulkan
and macOS/Metal still need runtime verification on those platforms. CI configuration is
not a report of a successful remote run.

## Structure

- Root package `zest`: public facade.
- `crates/zest-core`: private ECS implementation and public scene API.
- `crates/zest-paint`: validated quad snapshots and nested rectangular clips.
- `crates/zest-gpu`: experimental safe wrapper around direct HAL rendering.
- `demos/headless`: runnable reconciliation example.
- `demos/quad`: window, clipped quad batches, hover, resize and GPU readback checks.
- `examples`: external reference checkouts, excluded from the workspace.

The default scene API forbids unsafe code. The HAL crate has its own narrowly
scoped unsafe policy and documented invariants; see the
[HAL contracts](https://docs.rs/wgpu-hal/30.0.1/wgpu_hal/).

## License

Licensed under Apache-2.0; see [LICENSE](LICENSE).
