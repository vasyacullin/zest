# Zest 🍋

Zest is an early-stage Rust GUI engine project. The intended architecture combines
an immediate user API, a persistent internal ECS scene and direct GPU rendering
through `wgpu-hal`, targeting desktop and mobile applications and tools.

**Current implementation includes a headless scene and an experimental HAL quad demo.**
The scene provides scoped widget identities,
validation of scene/subtree declarations before mutation, persistent ECS nodes
and interaction flags, plus typed coordinate spaces and DPI conversion.
The separate GPU slice opens a window and renders ordered, clipped quad batches.
An optional text model provides Unicode editing, transient IME composition and
bounded undo/redo. Optional paragraph layout/rasterization renders Latin,
Cyrillic and Arabic in the GPU demo. Scene widgets are not yet connected to
rendering; widget layout is still planned. The window demo now binds the text
model to keyboard/IME events and paints caret, selection and preedit geometry.
See [plan.md](plan.md) for the full roadmap and acceptance criteria.

## Run

Install Rust 1.95 or newer, then run from this repository root:

```sh
cargo run -p zest-headless
cargo run -p zest-text-demo
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

The demo renders overlapping quads, a checker texture and text with rectangular clips and changes the
left quad's color on hover. Resize and DPI events update surface/geometry. Normal
operation uses an event-driven `Wait` loop. A timer runs only while an active
text drag can scroll beyond a viewport edge; other idle states request no frames.
Click the text field to edit, or outside it to release text focus. Use Shift with
arrows for selection, Home/End for displayed-line start/end, Enter for a newline,
Backspace/Delete for grapheme deletion, Ctrl/Cmd+A to select all, Ctrl/Cmd+Z to
undo, and Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y to redo. Ctrl/Cmd+Home/End move to document
start/end. Left/Right follow visual order in LTR/RTL runs; Up/Down preserve the
desired horizontal position through short or wrapped lines. Home/End use logical
text start/end within the displayed line. On macOS, Cmd+Left/Right also use line
boundaries and Cmd+Up/Down use document boundaries. Word and page navigation are
still pending. Click places
the caret using layout hit testing; Shift+click extends selection. Hold the left
button and drag to extend from a fixed anchor, including across graphemes and
wrapped/RTL runs. Dragging beyond the text viewport scrolls toward that edge
until release or the document boundary. Releasing the button, leaving the window,
losing focus, a DPI/lifecycle change or starting composition ends the drag.
The wheel scrolls the field under the pointer without moving the selection or
taking text focus. Vertical-only Shift+wheel scrolls horizontally. Line deltas
use 36 logical pixels; touchpad pixel deltas are converted from physical pixels
using window DPI. Offsets are clamped to content extents. Wheel scrolling during
a drag extends its head at the stationary pointer while retaining its anchor.
Manual scroll position persists across redraws; subsequent typing, navigation or
composition changes reveal the caret again. The caret is steady so idle
rendering stays asleep. Escape cancels an active composition; otherwise it exits.
Ctrl/Cmd+C, X and V copy, cut and paste plain text while the field is focused
and no IME composition is active. Empty copy/cut selections and empty pastes do
nothing. Clipboard failures are reported to stderr and keep the editor open.
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

Text checks shape Latin/Cyrillic/Arabic with explicit fallback fonts, upload
white coverage textures, and compare every readback pixel to the CPU coverage
blended on black in linear space (two byte tolerance after sRGB encoding).
They cover DPI 1/1.25/1.5, clipping and texture retirement. This verifies the
CPU-to-GPU path; it does not replace independent reference shaping goldens.
Editor smoke replays the same binding used for real keyboard/IME events, checks
selection/caret/preedit pixels, verifies that selection changes do not upload
text again, and exercises IME clear/commit/undo, focus cancellation, caret/drag
scrolling, wheel normalization at fractional DPI, clipped drag highlight pixels,
candidate-area bounds and texture retirement. Pointer tests use controlled elapsed
time for stationary-edge autoscroll, limit its step after a stall, and verify it
stops at document boundaries or capture cancellation. Native IME candidate
windows and actual keyboard layouts still require manual platform testing;
event replay does not establish their behavior.
Navigation checks cover vertical goal columns, visual movement in mixed bidi
text, Shift extension, ligature/grapheme boundaries, empty documents and soft-wrap
affinity. GPU checks verify the caret moves between visual lines even when its
byte offset is unchanged and navigation reuses the same bitmap/image.

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
transforms, dedicated glyph primitives and automatic extraction from the ECS scene remain
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

## Unicode editing model

Enable the facade's `text` feature to use `zest::text`, or depend directly on
`zest-text`. Its default editing model has no window, font, ECS or GPU dependencies. The deterministic
console demo (`cargo run -p zest-text-demo`) checks editing and composition;
it does not open a text editor window.

```rust
use zest::text::{Selection, TextBuffer};

let mut text = TextBuffer::new("Привет 👩🏽‍💻");
text.backspace().unwrap(); // the whole emoji is one deletion
assert_eq!(text.text(), "Привет ");
text.set_preedit("にほん", Some(Selection::caret("にほん".len()))).unwrap();
assert_eq!(text.text(), "Привет "); // committed document is unchanged
assert_eq!(text.display_text(), "Привет にほん");
text.commit_preedit("日本");
text.undo().unwrap(); // one transaction for the entire commit
assert_eq!(text.text(), "Привет ");
```

`Selection` stores anchor/head as UTF-8 byte offsets and preserves direction.
Document selections must lie on extended grapheme boundaries. Movement and
deletion use [unicode-segmentation](https://docs.rs/unicode-segmentation/1.13.3/unicode_segmentation/)
to keep combining marks, emoji sequences and CRLF together. New carets snap
forward to a grapheme boundary after an edit merges adjacent clusters. Movement
is in logical string order. The optional paragraph layer provides visual bidi
movement, line navigation and layout hit testing for interactive editors. Text and line
endings are preserved verbatim.

Preedit replaces the selected range only in `display_text()`. Its cursor is
relative to the preedit string and may use any UTF-8 scalar boundary. Invalid
document/preedit offsets return an error before mutation. During composition,
document edits, navigation and undo/redo return `CompositionActive`; the host
must commit or cancel first. An empty preedit cancels it. A commit may differ
from the last preedit or arrive without one. `display_selection()` and
`preedit_range()` project document-relative state into display-string byte offsets;
a hidden IME cursor returns None. `TextBuffer::clone()` makes an independent
copy including composition and bounded history, supporting staged edits.

Each actual replacement is one undo step, restoring text and directional
selection. Cursor-only changes and identical replacements retain redo; a new
text edit discards the redo branch. `HistoryLimits` bounds combined undo/redo by
edit count and UTF-8 delta payload bytes (defaults: 128 edits / 1 MiB). It excludes
allocation overhead and document/preedit storage. Oldest edits are evicted;
an edit larger than the budget succeeds but clears history so stale deltas cannot
be replayed. Zero in either limit disables history. `history_stats()` reports
both stacks and their combined payload size.

This initial `String` model scans grapheme boundaries and shifts text during
edits. It is intended for short UI fields; it has no large-document performance
claim. Winit routing, clipboard integration, pointer selection and scrolling
currently live in the demo rather than a public platform/widget API.

The demo routes [winit IME events](https://docs.rs/winit/0.30.13/winit/event/enum.Ime.html),
enables IME only for text focus, and updates the candidate area from the visible
caret in logical window coordinates. It handles the empty preedit preceding a
commit as one text edit. Focus loss, IME disablement and Escape cancel pending
preedit; late commits are ignored until IME is enabled again. Key releases,
synthetic focus key events, shortcut payloads and control text are not inserted.
Ctrl+Alt text remains available for AltGr; Option text remains available on macOS.
Command shortcuts use physical A/Z/Y/C/X/V positions so switching to a Cyrillic
layout does not disable selection, history or clipboard commands; ordinary typing
uses the event's text payload.
The binding stages the model/history and paragraph layout before accepting an
edit. If a raster budget is exceeded, it keeps the last accepted document and
reports the rejection to stderr; an active native composition is reset.

The demo opens [arboard 3.7.0](https://docs.rs/arboard/3.7.0/arboard/struct.Clipboard.html)
lazily on the first clipboard command and retains it until the window closes,
including for Linux clipboard ownership. Image clipboard support is disabled;
the Linux build enables Wayland data control as well as X11 support. Clipboard
availability depends on the desktop session. The core text model and default
facade do not depend on an OS clipboard backend.

Copy preserves selection, navigation, history and the text image. Cut stages the
deletion and paragraph layout, writes the selected text, then accepts the edit;
a failed write or layout check keeps the document and history intact. Paste is
one undo step and converts CRLF and bare CR to LF, preserving tabs and Unicode
sequences. Other control characters are rejected; copying embedded NUL is also
rejected to avoid truncation on native backends. Incoming plain text and the
resulting pasted document are limited to 64 KiB of UTF-8, with paragraph raster
budgets still applying. Clipboard rejections leave the accepted document intact.
Unit tests and the GPU smoke replay use an in-memory provider, including injected
read/write failures, and do not access the host clipboard. Native clipboard
interaction still needs manual verification on each target desktop.

## Paragraph shaping and rasterization

Enable `zest`'s `text-raster` feature (or `zest-text/raster`) for
`zest::text::raster::{Rasterizer, RasterOptions}`. Parley/Fontique 0.11.1 perform
font selection, Unicode shaping, bidi ordering and line wrapping; Swash 0.2.10
rasterizes monochrome outlines. These dependencies are optional; `text` alone
retains the small editing-only dependency set. No system fonts are discovered.

```rust
use zest::text::raster::{RasterOptions, Rasterizer};

let mut raster = Rasterizer::new();
let families = raster.register_font(std::fs::read("assets/fonts/NotoSans.ttf").unwrap()).unwrap();
let bitmap = raster.rasterize("Привет, office!", &[families[0].as_str()],
    RasterOptions { font_size: 24.0, width: 400.0, scale: 1.5 }).unwrap();
// With the gpu feature: renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)
```

Register application-owned font bytes and supply an ordered list of registered
families for fallback. Missing characters produce tofu glyphs and increment
`metrics.missing_glyphs`. The demo/test fonts cover Latin/Cyrillic and Arabic,
not general CJK or emoji. Their source revision, SHA-256 hashes, copyrights and
SIL OFL 1.1 licenses are in [assets/fonts](assets/fonts/README.md). Libraries do
not embed these assets; demo binaries embed them and distributions must retain
their font licenses.

`RasterizedText` contains a white straight-alpha RGBA8 bitmap, physical bitmap
origin and dimensions, scale and logical typographic metrics. Ink bounds include
negative overhangs and accents beyond those metrics. To place a bitmap at logical
paragraph origin `(x, y)`, use image origin `(x + origin_x/scale, y + origin_y/scale)`
and size `(width/scale, height/scale)`; image tint supplies linear text color.
Coverage is alpha, so it is not sRGB encoded before upload. Normal GPU rendering
requires no readback.

Reuse one rasterizer per coarse UI context and retain the resulting bitmap/GPU
image until text, fonts, wrapping width or DPI changes. The demo reuses its image
across redraws and replaces/retires it when text or DPI changes. This first implementation
rebuilds paragraph layout and rasterizes a whole paragraph per call; it has no
persistent shaping cache or shared GPU glyph atlas. It renders grayscale outlines
only, without color emoji or rich text decorations.

`Rasterizer::paragraph` returns an immutable `Paragraph` retaining that same
layout plus its bitmap. `caret`, `selection_rects` and `hit_test` use logical
coordinates relative to the paragraph origin. UTF-8 offsets are validated;
scalar offsets inside a shaped cluster resolve to cluster geometry. Hit testing
snaps forward to a grapheme boundary accepted by `TextBuffer`. These byte-only
methods canonicalize bidi/soft-wrap affinity. Interactive editing uses
`Paragraph::layout_selection` or `selection_at` to create a `LayoutSelection`,
then `move_selection` with `Navigation` and an extension flag. The state retains
both endpoint affinities and the horizontal goal for repeated Up/Down moves.
`selection_caret` and `selection_geometry` use that retained visual state;
`extend_selection` preserves the pointer anchor's affinity. `selection()` gives
grapheme-aligned model byte offsets. `same_position()` compares visual endpoints,
not the saved vertical goal column.

Layout selections belong to one immutable snapshot. Using them with another
paragraph returns `ForeignSelection`; rebuild them from model offsets after text,
font, wrapping width or DPI changes. The demo clears cached navigation on a layout
rebuild or model edit, and blocks navigation while IME preedit is active.
The demo projects the editing model's display selection into these snapshots,
paints selection behind text and preedit underlines/caret above it, and clips
everything to the field viewport. Selection-only changes reuse the same bitmap
and GPU image.
`Paragraph::content_size()` includes trailing whitespace, bitmap ink and the
line-end carets in the positive scroll extents from the paragraph origin, allowing
the viewport to reach whitespace and a final empty line. Negative ink overhangs
remain available through the bitmap's origin/dimensions.

Each call accepts at most 64 KiB of text. Registered input font bytes total at
most 16 MiB. Output is limited to 4096 pixels per dimension and 4 Mi pixels;
retained intermediate glyph masks have a separate 4 MiB limit. These limits do
not bound the libraries' internal caches or all temporary allocations. Font size
must be positive and at most 256 logical units, physical font size 1–512 pixels,
scale positive and at most 8, and physical wrapping width 1–4096 pixels. Invalid
options, unknown fonts and budget failures return errors. Use trusted font assets.

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

CI is configured for formatting, Clippy, tests, the headless/text demos and benchmark
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
- `crates/zest-text`: Unicode editing, transient preedit, bounded history and optional paragraph rasterization.
- `demos/headless`: runnable reconciliation example.
- `demos/quad`: window, clipped quad/image/text batches, hover, resize and GPU readback checks.
- `demos/text`: deterministic console smoke for the Unicode editing model.
- `examples`: external reference checkouts, excluded from the workspace.

The default scene API forbids unsafe code. The HAL crate has its own narrowly
scoped unsafe policy and documented invariants; see the
[HAL contracts](https://docs.rs/wgpu-hal/30.0.1/wgpu_hal/).

## License

Source code is licensed under Apache-2.0; see [LICENSE](LICENSE).
Bundled demo/test fonts retain their SIL OFL 1.1 licenses in
[assets/fonts](assets/fonts/README.md).
