# Zest 🍋

Zest is an early-stage Rust GUI engine project. The intended architecture combines
an immediate user API, a persistent internal ECS scene and direct GPU rendering
through `wgpu-hal`, targeting desktop and mobile applications and tools.

**Current implementation includes a headless scene, optional Flexbox layout and an experimental HAL quad demo.**
The scene provides scoped widget identities,
validation of scene/subtree declarations before mutation, persistent ECS nodes
and interaction flags, typed event routing, semantic inspection, plus typed coordinate spaces and DPI conversion.
The separate GPU slice opens a window and renders ordered, clipped quad batches.
An optional text model provides Unicode editing, transient IME composition and
bounded undo/redo. Optional paragraph layout/rasterization renders Latin,
Cyrillic and Arabic in the GPU demo. Scene widgets are not yet automatically connected to
rendering. An optional retained Flexbox layer computes scene bounds and supplies
immutable logical snapshots. The window demo binds the text
model to keyboard/IME events and paints caret, selection and preedit geometry.
See [plan.md](plan.md) for the full roadmap and acceptance criteria.

## Run

Install Rust 1.95 or newer, then run from this repository root:

```sh
cargo run -p zest-headless
cargo run -p zest-text-demo
cargo run -p zest-layout-demo
cargo run -p zest-layout-text-demo
cargo run -p zest-quad -- --layout
cargo run -p zest-quad
cargo run -p zest-quad -- --smoke
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

The demo reorders keyed buttons, then partially updates their container while
retaining a button's focus/hover and a status sibling, then routes an event to the
focused button through its container. Explicit keys are required
in this initial API; automatic call-site keys are planned.

## Retained Flexbox layout

Enable `zest`'s `layout` feature to use `zest::layout`, or depend directly on
`zest-layout`. It keeps a private [Taffy 0.15.1](https://docs.rs/taffy/0.15.1/taffy/)
tree keyed by the scene's `WidgetId`; Taffy/ECS node IDs stay private. The default
facade does not include this dependency. Layout needs no fonts, window or GPU.

```rust
use zest::{Scene, geometry::Size, layout::{LayoutTree, Length, Style}};

let mut scene = Scene::default();
let mut save = None;
scene.update(|ui| save = Some(ui.button("save", "Save"))).unwrap();
let mut layout = LayoutTree::default();
layout.sync(&scene, |_| Style {
    width: Length::Px(100.0),
    height: Length::Px(24.0),
    ..Default::default()
}).unwrap();
let frame = layout.compute(Size::new(400.0, 200.0)).unwrap();
let bounds = frame.snapshot.bounds(&save.unwrap()).unwrap();
assert_eq!(bounds.size.width, 100.0);
```

The initial style supports no-wrap rows/columns, auto/pixel/percentage dimensions
(0.5 means 50%), minimum sizes, padding, gap, flex grow/shrink and cross/main-axis
alignment. Minimum sizes default to zero. Leaves use the caller's
`intrinsic_size` for auto content dimensions; nonempty containers measure their
children. Supply new intrinsic metrics when text, fonts or other content change.
The optional text adapter below adds font-based, width-dependent measurement.
Scene roots sit inside a private viewport-sized, stretched vertical column.

`sync` walks the complete scene and evaluates the style callback before changing
any layout node. Invalid styles and callback panics leave the accepted tree and
snapshot intact. It updates only changed styles and child lists, retains nodes
through keyed reorders, and removes omitted nodes. Partial `Scene::update_children`
updates are reflected by the next complete layout sync. Hover/focus and label
changes do not invalidate layout unless the supplied style/metrics also change.

`compute` returns absolute, unrounded `Rect<Logical>` bounds in parent-before-child
declaration order. Taffy's calculations use f32 logical pixels; DPI conversion
belongs to painting/platform code. Zero-sized viewports are supported. Invalid
dimensions are rejected, and nonfinite coordinates or negative computed sizes are never
published. Existing snapshot clones remain immutable after edits or failures.
`snapshot()` returns None while accepted layout inputs await computation.
Unchanged inputs reuse the same snapshot without calling Taffy; other updates
reuse Taffy's retained caches where constraints permit. `SyncStats` counts changed
nodes/child lists, and `ComputeStats` reports engine execution and intrinsic
measurement calls, rather than total node visits.

`new.changes_since(&old)` compares computed snapshots without changing the scene,
layout engine or either snapshot. `LayoutChanges` lists `added` and `changed` IDs
in new declaration order, and `removed` IDs in old order. Retained-node comparison
includes projected/unscrolled bounds, effective clips, scroll metrics, parent and
clip-children policy, including invisible nodes. `order_changed` compares the full
declaration sequence, so additions/removals also set it; `viewport_changed` can be
true even for an empty layout. `is_empty()` checks all lists and flags.

Shared snapshots compare empty immediately. Otherwise the comparison walks both
snapshots using existing identity indexes; independently computed equal geometry
also compares empty. Style inputs, text/appearance and entity generations are
outside this comparison. A recreated key with identical layout can compare equal;
combine layout changes with scene paint invalidation to cover that case. Old/new
bounds remain available on their respective snapshots for repaint decisions.
This API does not calculate damage rectangles, run layout, mark scene flags or
schedule frames. The layout preview marks added/changed nodes for paint, and its
root on viewport/sequence/removal changes, then redraws the whole frame. Geometry
changes therefore invalidate paint without a scene declaration update; unchanged
geometry keeps paint clean. Text/image caches stay reusable for clip/scroll changes.

The headless `zest-layout-demo` checks reorder identity, focus/hover, fractional
bounds, snapshot/cache reuse and explicit conversion of layout bounds into quads.
The CI workflow includes this demo. Grid, wrapping flex rows, absolute/manual layout,
automatic scene paint extraction and a complete widget/platform event binding remain future
work; this is the first layout slice, not a complete widget system.

`LayoutSnapshot::hit_test(point, |id| accepts(id))` finds the last accepted box in
snapshot declaration order. Paint in that order to use the same layering. It
clips hits to the viewport, uses logical coordinates with half-open rectangle
edges, ignores empty boxes and rejects nonfinite/out-of-viewport points. The
predicate lets the caller exclude containers, disabled or noninteractive nodes.
Use freshly computed geometry before positional input; old snapshots remain
immutable and cannot detect scene edits on their own. This initial path tests
rectangular bounds with effective ancestor clips; transforms are not supported.

`Style::clip_children` (default false) intersects descendants' clips with that
node's border bounds, in logical coordinates. It affects both axes and propagates
through nonclipping descendants. A nonclipping parent permits overflow; an empty
clipping parent hides its descendants. Every `LayoutBox::clip` includes the
viewport and all clipping ancestors. It does not clip the node's own paint to its
own bounds. Clipping alone leaves `bounds` unchanged, while
`LayoutSnapshot::visible_bounds(id)` (also on `LayoutBox`) returns their visible
intersection, or None for empty/fully clipped/unknown nodes.

Paint each node through the same effective clip used by hit testing:

```rust
use zest::paint::Quad;

for node in snapshot.boxes() {
    painter.with_clip(node.clip, |painter| {
        painter.quad(Quad { bounds: node.bounds, color: [0.1, 0.4, 0.8, 1.0] })
    })?;
}
```

Clipping/scrolling alone creates a new immutable snapshot without rerunning Taffy or text
measurement (`ComputeStats::performed == false`). Unchanged settings reuse
the same snapshot. Geometry/style changes still use the normal layout caches.
Old snapshots retain their original clips/positions. Rounded masks are not
supported; applications still filter interaction eligibility themselves.

`Style::scroll_offset: Point<Logical>` requests a nonnegative, finite content
offset for a clipping container. Its descendants move by the negative offset;
the container's own border and clip stay fixed. Both axes are supported. Offsets
are inert when `clip_children` is false. `LayoutSnapshot::scroll(id)` (also the
`LayoutBox::scroll` field) returns `ScrollMetrics { offset, max_offset }` with the
applied, clamped position and maximum allowed position. Oversized finite requests
are safe; invalid requests reject the whole sync before mutation.

Ranges include positive descendant overflow and trailing padding. Overflow flows
through nonclipping descendants but stops at a clipping child's border; nested
scrollers calculate independent ranges from unscrolled layout. Resizing/content
changes recompute those ranges and reclamp requested positions. Empty containers
have no range. This initial implementation does not expose negative overflow
before the origin or touch gestures.
Wrap text in a container to scroll it; a leaf's own bitmap is not scroll content.

`LayoutBox::layout_bounds` retains absolute unscrolled geometry; `bounds` contains
the projected position after all ancestor scrolling. Paint, clips, visible bounds
and hit testing use that same projection. Sizes/text measurement stay unchanged.
The core scene remains geometry-independent: update eligibility/cancel gestures
from current snapshots when scrolling or restoring focus.

`LayoutSnapshot::reveal_plan(id)` computes a read-only `RevealPlan` with absolute
`ScrollAdjustment { widget, offset }` values for changed ancestors, ordered from
innermost to outermost. Apply them to those nodes' styles and sync/compute again.
`visible_bounds` predicts the visible portion after all changes, including the
viewport. Small targets use the nearest edge that fits them; oversized targets
fill their scrollport and stay still when they already span it. An inner clip
passes only its visible portion outward, avoiding unnecessary outer movement.
Visible targets can return an empty list. Unknown, empty or completely
unreachable targets return None. Planning does not mutate the scene/snapshot,
run Taffy or measure text. Use fresh snapshots and check scene eligibility;
plans carry no entity-generation or input/modal authorization.

`LayoutSnapshot::scroll_plan(target, delta, boundary)` routes a signed logical
offset delta through a target and its clipping ancestors. Positive components
increase offsets; negate normalized wheel content movement before calling it.
Each scrollport consumes only the available movement, passing the remainder
outward independently on each axis. `ScrollPlan::adjustments` contains absolute
`ScrollAdjustment` offsets, inner-first; apply them to styles and sync/compute.
`remaining` holds movement unused by the chain. Nonclipping nodes pass it through;
starting at a container includes that container, and sibling branches are skipped.

The optional boundary is an inclusive ancestor/target where routing stops, such
as the active modal root. Unknown/unrelated boundaries and nonfinite deltas reject
the plan without mutation. Zero requests and exhausted chains return an empty
adjustment list. Planning uses a fresh immutable snapshot, does no layout/text
measurement, and carries no scene eligibility or entity-generation checks.
Choose the start with current hit testing and scene permissions before applying
it. Applications may discard remaining movement or forward it to another owner.
Discard the remainder when the boundary is intended to contain input, such as a modal.

`ScrollbarGeometry::new(track, Axis::{Row, Column}, viewport_extent, metrics,
min_thumb)` provides shared logical track/thumb geometry for custom painting and
input. Row is horizontal; Column is vertical. It hides empty/invalid tracks and
zero overflow, clamps finite oversized offsets and bounds the minimum thumb to
the track. Proportional size and drag mapping avoid overflowing large finite
metrics. `hit_test` uses half-open edges; `grab_offset` records where the thumb was
pressed so dragging does not jump. `drag_offset` clamps captured pointers outside
the track; `page_offset` moves one viewport before/after the thumb. A tiny track
with no thumb travel retains its offset without division. The helper owns no
scene widget or capture state; callers cancel drags when geometry/ranges change.

Enable the facade's `layout-text` feature (or `zest-layout/text`) to connect
shaped text to Flexbox leaves. Create `TextContent::new(raster.shape(text,
families, ShapeOptions { font_size, scale })?)`, then call
`layout.sync_with_text(&scene, |id| (style, optional_text_content))`. Nonempty
containers cannot contain measured text. `Style::intrinsic_size` remains the
fallback for leaves without text; ordinary `sync` clears measured text content.

Text measurement receives Taffy's known content dimensions and available width,
after padding. It uses real line breaking for min/max-content requests and
reflows for definite widths, including zero. Known dimensions remain fixed.
The same shaping is reused as a viewport narrows or expands, so paragraph height
and following siblings adapt without rasterizing glyphs. Cloning `TextContent`
preserves its identity and retained measurement caches. Shape a replacement
after text, font/fallback, font size or DPI changes; its new identity invalidates
the node and dependent sizes. Color changes need no new shape. A measurement
failure publishes no snapshot and clears placeholder measurement caches before
retry. `SyncStats::content_changed` counts replacements separately from styles.

`zest-layout-text-demo` embeds the existing licensed demo fonts and checks
Unicode/RTL wrapping, padding, sibling movement, content replacement and cache
reuse. The CI workflow includes it. The plain `layout` feature stays free of
font/shaping dependencies. This adapter measures typography; automatic text
painting, glyph atlases and width-keyed measurement caches beyond Taffy's own
cache remain future work.

`cargo run -p zest-quad -- --layout` opens a responsive text preview. Resizing
reflows its labels and moves the following marker using scene layout bounds.
It renders from the same shaped text used for measurement, retaining one
paragraph/image per label. Unchanged widths, tint and position changes reuse
those images; width/content/DPI changes rebuild them and retire old textures.
GPU restoration can reupload the retained CPU paragraph without reshaping.
The preview clips to the viewport, limits the readable column width, omits text
when its content width is below one physical pixel and handles empty viewports.
Hover highlights a label/marker; pressing the primary mouse button focuses it.
Releasing over the same node toggles selection (green underline/marker). Moving
away disarms the press without transferring capture; moving back re-arms it.
Blank-space presses clear focus. Tab/Shift+Tab cycles eligible interactive nodes
in declaration order, revealing hidden targets through the column's scrollport.
Disabled/modal-blocked nodes and unreachable clips are skipped. Enter/Space toggles selection on release; repeats and
synthetic keys cannot activate nodes. Unsupported modifiers cancel a pending press.
F2 toggles the body label's enabled state. Disabled labels are dimmed, reject
pointer/keyboard activation and are skipped by Tab; selection stays retained.
Re-enabling re-hits a stationary pointer without restoring the old focus/press.
F3 restricts input to the body label and marks its top edge in amber. Other nodes
are dimmed and reject clicks; Tab/Shift+Tab stays in the scope. Esc or F3 closes
it and restores the previous visible, available focus. F2 disabling the scoped
label also closes its scope. These shortcuts ignore synthetic/repeated events.
F4 clips the column to its fixed 96.25-logical-pixel border height, or allows its
overflow again. Text and the marker keep the same layout bounds and textures.
Paint, pointer hits, Tab and restored focus use the same effective ancestor clip;
visible slivers narrower/shorter than one physical pixel are ineligible. Changing
clips cancels pending gestures and re-hits a stationary pointer before more input.
Keyboard traversal applies a reveal plan before checking the new focused node's
visibility. It works in both directions and when a modal retains the same focused
ID but needs a scroll adjustment. Repeated revelation of a spanning/visible
target leaves the snapshot unchanged. Mouse input still requires currently
visible bounds, including the minimum one-physical-pixel extent.
While F4 clipping is enabled, the wheel scrolls the column under the pointer.
Line deltas use 36 logical pixels; touchpad pixels are divided by window DPI.
Shift redirects vertical-only deltas horizontally; unsupported modifiers and
nonfinite deltas are ignored. Positions clamp at the ends; boundary no-ops reuse
the snapshot. Wheel routing uses `scroll_plan` from the topmost input-eligible
box, including container padding or the scrollbar track. Modal scopes stop the
chain at their root, so a non-scrollable leaf modal cannot scroll an outside owner.
A disabled/unfocused column rejects it. Zero/nonfinite deltas are ignored without
cancelling an active scrollbar drag. Scrolling re-hits a stationary pointer and
cancels gestures; fully hidden focus is cleared. Resizing reclamps the position;
turning F4 off resets it. Text paragraphs and GPU images stay reusable.
The clipped column also paints a vertical scrollbar in its right padding. Drag
the thumb to move content, or click the track to page once (no held-button repeat).
Thumb/track input takes precedence over scene hits and does not activate or focus
an underlying node. Capture continues outside the track until release; leaving
the window still depends on the platform delivering captured events. Resize,
DPI, reconciliation, blur, occlusion, root disabling, modal entry, scrolling by
wheel or keyboard commands cancel a drag. An unchanged redraw keeps it active.
Esc cancels a scrollbar drag before closing a scope/window. The overlay hides
without overflow, on disabled/tiny columns and while a modal scope is open.
This preview overlay is a geometry/input demonstration, not a scene scrollbar
widget or accessibility implementation. It changes quads without uploading text.
Targets that cannot reveal a one-physical-pixel visible area are skipped. Geometry is refreshed before
pointer/keyboard/wheel input, and resize re-hits a stationary pointer. DPI changes discard
stale pointer coordinates; focus remains on the keyed node while it is visible.
Focus loss clears hover/focus/modifiers and resets pending gestures. Geometry,
DPI, occlusion and Tab changes cancel a pending gesture without activating it.
Keyed reorder retains focus/selection; removal clears them. These changes update
scene flags and paint without invalidating layout, paragraphs or text images.
Repeated movement within one target requests no redraw.
F6 lets the column intercept child activations and marks its top edge orange.
Clicks and Enter/Space still show press/focus feedback, but their selection action
is prevented. F6 restores normal activation. An F3 modal scope excludes the outer
column from its event route, so activation inside that scope still works.
F7 lets the column move focus to the footer after an allowed child activation,
marking its bottom edge cyan. Selection still applies to the originally activated
node before focus changes. Prevented activations and modal routes that exclude the
column do not schedule this focus change. F7 restores normal focus behavior.
Text/IME editing remains in the default demo. Esc closes the layout window when
neither a modal scope nor a scrollbar drag is active.
The raster bitmap budgets still apply. Layout smoke uses controlled input replay
for state/highlight pixels and ignores live pointer/Tab input for deterministic
captures; native keyboard/window focus and outside-window release delivery still
need platform testing.

`zest::input::ActivationState` provides the window-independent gesture controller
used by this preview. Keep one per scene/primary pointer, supply eligible hit
targets or focused nodes, and handle the returned `InputResult::activated` once.
`WidgetState::pressed()` shares the existing flags component and marks paint dirty
without changing ECS archetypes. Captures record scene identity and entity
generation, so a removed/recreated key cannot finish its predecessor's gesture.
Call `reconcile` after scene/focus changes, `cancel` after geometry/eligibility
changes, and `reset` when the window loses input ownership. Cancellation retains
held-input latches until release; reset forgets them. Reset with the owning scene
before dropping/switching controllers. Capture is logical: the window backend
must deliver releases outside its bounds; the controller does not grab the cursor.

`zest::event` adds typed, synchronous routing with no window, layout or GPU dependency.
`Scene::dispatch_event(target, &mut payload, handler)` delivers `EventContext` and
the mutable payload to the handler at each node: capture from the outermost
ancestor to the parent, target once, then bubble from parent to outermost ancestor.
`dispatch_focused_event` chooses the current focused node. Identity-only `push_id`
scopes are skipped. The active modal root is an inclusive boundary; handlers outside
it are not called. Unknown, disabled or modal-blocked targets return `None` without
calling handlers or changing the payload.

Return `EventControl::Continue`, `StopPropagation`, `PreventDefault` or `Consume`
(stop and prevent). Stopping propagation alone leaves the default action allowed,
even if capture stopped before target delivery. Prevention persists through later
handlers; `EventContext::default_prevented` reports earlier prevention.
`DispatchResult` reports visits, target delivery, propagation stopping and default
prevention. Apply the application/widget default only for `Some(result)` with
`!result.default_prevented`. The layout preview uses this path for both pointer
and keyboard selection actions.

Handlers may mutate application state and the typed payload while borrowing the
scene immutably. Record reconciliation/focus changes as intents and apply them
after dispatch. Nested dispatches have independent propagation/default state.
`EventContext::scene()` exposes the routed scene for read-only queries and command binding.
Each dispatch reads the current hierarchy; a recreated key addresses its new node.
Validate queued/stale input separately: `ActivationState` checks gesture generations,
and positional targeting requires current geometry. Routing does not automatically
activate, focus, capture input or invalidate paint.

`EventHandlers<E, S>::new(&scene)` stores typed callbacks for components, with one
callback per widget/phase. `set(&scene, id, phase, callback)` installs or replaces
that binding; callbacks receive `EventContext`, `&mut E` and `&mut S` for application
state. They can own captured data, require no Send/Sync bound, and must have a
`'static` capture lifetime. Pass transient application state through `S`.
`dispatch(&scene, target, &mut event, &mut state)` and `dispatch_focused` use the
same scene route and controls. Nodes without a callback continue; `visits` counts
route nodes/phases rather than callback invocations. The preview registers its
parent interceptor through this API; the headless demo registers a button action
that updates application state and prevents the default.

A registry belongs to its original scene. Bindings retain the exact entity
generation through keyed reorder/label changes; deleted/recreated nodes never
inherit old callbacks. Explicitly call `set` to bind the new node. Disabled and
modal-blocked nodes keep their bindings but reject delivery. `prune(&scene)` drops
stale callbacks after reconciliation and returns the count removed; delivery skips
them even without pruning. `len` includes stale entries until removal.
`remove(id, phase)`, replacement and `clear` release owned captures. Foreign scenes
cannot bind or dispatch; pruning a foreign scene leaves the registry unchanged.
The registry is mutably borrowed during dispatch, so change bindings and apply
scene edits afterward. Independent registries can handle different event types;
recursive dispatch through the same registry is not supported.

`zest::commands::SceneCommands::new(&scene)` collects scene edits for after event
dispatch. Queue `focus`, `set_enabled`, `push_modal` or `invalidate_paint` with the
scene and target ID; unknown/foreign targets return false without appending.
Bindings capture exact entity generations when queued. Keyed reorder/label changes
retain them; deleted/recreated nodes are skipped. `clear_focus()` and `pop_modal()`
are scene-wide intents applied to the current focus/stack, without a saved target.
These methods only enqueue; use `apply(&mut scene)` to execute in insertion order.
Earlier commands can change later eligibility (enable then focus, push then pop).
Commands obey the existing focus, availability and modal rules at application time.

`apply` returns `Some(CommandStats { applied, skipped, changed })` and drains its
owner's queue. Accepted no-ops count as applied; stale/rejected commands count as
skipped. `changed` includes local availability changes even under disabled ancestors.
Applying to a foreign scene returns None and retains the entire queue. `len`,
`is_empty` and `clear` inspect/discard pending intents. The queue is ordered, not
transactional, and does not rebuild declarations, run layout/hit testing or render.
After applying, reconcile input controllers and refresh geometry/eligibility before
the next dependent input. The demo applies its F7 bubble handler's focus intent
after dispatch and the original selection default, then refreshes input. Queueing
is independent of default prevention: handlers must explicitly choose whether
their intents should run when the event default was prevented.

`Scene::needs_paint()` reports pending scene appearance or structural changes.
`paint_invalidation()` returns a non-consuming `PaintInvalidation` snapshot:
`widgets` lists current dirty nodes in declaration preorder (including disabled or
modal-blocked nodes), while `structure_changed` records additions, removals and
reorders. Identity scopes are omitted. Removing the last root still produces
nonempty work through `structure_changed`, even though `widgets` is empty, so the
old frame can be cleared. This is a coarse signal; damage rectangles and automatic
display-list extraction are not implemented.

`take_paint_invalidation()` returns that work and clears only the paint flags and
structural signal. Focus, hover, pressed state and input ownership are preserved.
Later changes accumulate separately. This is a CPU drain, not a GPU acknowledgement:
if draining before rendering, retain the returned work across failure/suspension
and retry it. Alternatively inspect first, then drain after a successful complete
frame without intervening scene edits. The layout demo follows the latter rule;
skipped, suspended, resize-required and smoke clear-only frames retain work.

Changing application-owned visual state requires `invalidate_paint(id)` after
event dispatch. Repeated marks coalesce; unknown IDs return false and known nodes
may be marked regardless of input eligibility. The preview marks both previous and
new selection and its parent interception decoration. Invalidating/consuming paint
does not affect layout, paragraphs, GPU images or ECS archetypes. Initial queries
scan nodes; collecting/draining traverses the hierarchy. Window exposure, external
style/layout changes, resizing and device recovery can require repaint independently
of clean scene flags. The caller still schedules redraws and chooses repaint bounds.

`Scene::set_enabled(id, enabled)` changes a retained local setting. Its effective
value, `WidgetState::enabled()`, includes disabled ancestors; an enabled child
cannot override a disabled container. `Scene::locally_enabled` reads the local
setting. New children inherit effective availability, including during partial
updates. Reorders and label changes retain local settings; deletion resets them.
Availability propagation walks only the affected subtree, marks changed appearance dirty, and
immediately clears hover/focus/pressed/capture flags. Re-enabling before another
input event cannot revive an old gesture. These flags share the existing ECS
component and do not affect layout or archetypes. `focus` and `set_hovered(true)`
reject disabled targets. Update the caller's hit targets after availability changes.

`Scene::focus_next(FocusDirection::{Forward, Backward}, predicate)` cycles enabled
buttons in declaration preorder, wraps, and skips containers/disabled subtrees.
The predicate supplies application eligibility, such as current viewport bounds.
An excluded old focus starts at the first/last candidate; no candidates clears
focus. It returns whether focus changed. The preview uses this shared traversal.

`Scene::push_modal(root)` and `pop_modal()` manage a stack of modal input scopes.
The innermost `modal_root()` and its descendants are the only input-eligible nodes;
`input_allowed(id)` also checks effective availability. Use it for hit testing.
Explicit focus, hover, activation and Tab traversal enforce the same boundary.
Scope roots may be containers or individual buttons. Nested roots must belong to
the current scope; invalid pushes and empty pops return false without mutation.
Opening preserves an eligible focus inside the scope, or selects its first enabled
button. An empty scope traps input with no focused widget. Closing restores the
saved focus only if its exact ECS generation remains available inside the outer
scope; otherwise focus stays empty. Removing or disabling a scope root closes it
and every nested scope, restoring the invalidated outer scope's previous focus.
Re-enabling/recreating roots does not reopen them. Reorders retain live scopes.

Every successful scope transition revokes all in-flight gestures, including ones
inside the new scope, clears blocked hover, and marks the scene paint-dirty.
Text/layout caches, enabled settings and ECS archetypes remain unchanged. Call
`ActivationState::reconcile` after scope changes before querying cached capture.
This API manages input boundaries; applications paint overlays/portals, filter
visibility and handle outside clicks or window lifecycle. The preview retains
focus on outside clicks, keeps scopes across window blur, and clears clipped
restored focus using current geometry. Full dialog/overlay widgets remain future work.

## Semantic inspection

`Scene::semantic_snapshot()` copies the hierarchy, names, roles and interaction
state into an immutable `zest::semantics::SemanticSnapshot`. Clones share owned
data; old observations remain readable after reconciliation without borrowing the
scene. `nodes()` follows declaration preorder, `roots()` retains root order, and
`node(id)` looks up the same stable `WidgetId` used by layout/events. Identity-only
scopes are omitted. `SemanticNode` exposes parent/children, the scene label as its
name, and `SemanticRole::{Group, Button}` for containers/buttons. Containers initially
have empty names. State reports effective enabled, focused, hovered, pressed and
input-allowed values; the last includes the current modal boundary. Paint/capture
bookkeeping is excluded, and reading snapshots never consumes paint work.

`snapshot.find(SemanticQuery::new().role(SemanticRole::Button).named("Save"))`
requires exactly one match. No match returns `FindError::NotFound`; ambiguity
returns `FindError::Ambiguous` with all matching IDs in declaration order.
`find_all(query)` returns every match. Query filters `role`, `named`, `id`, `within`,
`enabled` and `input_allowed` combine with AND. Names match exactly, including case
and Unicode. `within(id)` includes that node and its descendants; an unknown scope
matches none. Disabled and modal-blocked nodes stay observable unless filtered out.

These initial snapshots copy the scene and queries scan their nodes. Roles/names
are derived from the current basic widgets; custom semantic metadata, semantic
actions and platform accessibility bridges remain future work. IDs describe paths,
so snapshots do not validate entity generations or queued actions. Query fresh
data and join with current layout/visibility before routing input or binding scene
commands. Headless and GPU smoke exercise semantic queries, immutable observations,
gesture state and modal/disabled eligibility alongside the existing input path.

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
boundaries and Cmd+Up/Down use document boundaries. Ctrl+Left/Right (Option on
macOS) move by visual word boundaries; Shift extends the selection. Without Shift,
an existing range first collapses toward the requested visual edge. Ctrl+Backspace
and Ctrl+Delete (Option on macOS) delete the selection or the previous/next word
in logical text order, as one undo step. PageUp/PageDown move about one viewport
height through displayed lines, retain the desired column and scroll with the
caret. Shift extends selection. At the first/last line, another outward page
step reaches the document edge. Ctrl/Alt/Cmd combinations with page keys are
currently consumed without an action. Click places
the caret using layout hit testing; Shift+click extends selection precisely.
Double-click selects a Unicode word (or punctuation/whitespace segment);
triple-click selects a displayed line, including its newline when present.
Dragging after a double/triple-click extends by complete words/lines, retaining
the whole original unit when crossing it. The demo recognizes successive left
presses within 500 ms and 4 logical pixels of the first press; the fourth starts
a new series. These are fixed demo thresholds, independent of OS preferences.
Moving farther, keyboard input, composition, scrolling, focus/lifecycle changes,
layout rebuilding or pointer cancellation break the series. Native IME enable/
disable notifications between clicks preserve it. Hold the left
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
affinity. Page tests also cover tiny/extreme viewport heights, fractional DPI,
short lines, trailing empty lines and progression through narrow wrapped bidi
text. GPU checks verify the caret moves between visual lines even when its byte
offset is unchanged, page scrolling retains its viewport-relative height, Shift
page selection paints correctly and navigation reuses the same bitmap/image.
Controlled-time pointer tests and GPU replay exercise double/triple-click
selection, whole-word drag reversal, RTL, newline copy and highlight pixels
without accessing the system clipboard or sleeping between synthetic presses.

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

`DisplayListCache<K>` retains one CPU snapshot under an application-owned key.
`get_or_try_build(key, builder)` returns an `Arc<DisplayList>`; equal keys skip the
builder and share command buffers. Changed keys publish a new snapshot only on
success. Errors are returned, rather than substituting old paint; errors/panics
leave the previous entry available under its previous key. `get(&key)` reads only
a matching entry, and `clear()` releases cache ownership. Previously returned Arcs
remain immutable CPU snapshots. Keys need `PartialEq`, with no Clone/Default bound;
builder errors may use any type.

The caller must include every paint input in the key, or clear the cache when it
changes. This includes resource identities and device recovery. The cache owns no
GPU images and does not schedule/acknowledge frames; keeping an old Arc does not
keep its referenced textures alive. Scene invalidation, layout comparison and GPU
resource validation remain separate contracts.

The layout window uses this cache with geometry identity, visible interaction
flags, selection/modal decoration, tint, scrollbar geometry/appearance and text
image identities/bitmap bounds. Dirty/capture bookkeeping is excluded, so retrying
or acknowledging a frame can reuse its commands. Clearing/restoring GPU images
replaces cached image handles while retaining CPU paragraphs. GPU smoke compares
cached/direct commands and pixels across interaction, clipping and scrolling.

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

`Movement::PreviousWord` / `NextWord` and `delete_previous_word()` /
`delete_next_word()` use Unicode word segments in logical string order. Backward
steps reach the previous segment's start; forward steps reach the next segment's
end, skipping intervening whitespace. Punctuation and emoji remain navigable
segments. Segmentation keeps full-document context for apostrophes and numbers,
and endpoints snap outward to extended grapheme boundaries. Existing selections
collapse before unextended motion; deletion replaces the selected range directly
and undo restores its original direction. This is Unicode segmentation, without
locale-specific dictionary or programming-language subword rules.

The paragraph layer adds `Navigation::WordLeft` / `WordRight` using Parley's visual
word boundaries, preserving bidi/wrap affinity and Shift anchors. It skips stops
inside model graphemes and resets the saved vertical goal column. Logical model
steps and visual paragraph steps can differ in mixed-direction text. Word motion
does not change history, rebuild the paragraph or upload another text image.
Word commands are blocked during preedit; an accepted deletion that changes no
text still reveals a manually hidden caret.

`Paragraph::move_selection_page(&selection, PageDirection::Down, height, extend)`
takes a positive finite viewport height in logical pixels; `PageDirection::Up`
moves backward. It uses displayed-line metrics, moves at least one line when
possible and clamps at the first/last line before reaching the document edge on
another outward step. Page and Up/Down moves share the saved horizontal goal,
including through short lines and grapheme normalization. Line lookup uses the
caret's center so small rounding overlaps between fractional-DPI line bounds do
not select a neighboring row. At wrapped bidi boundaries, it resolves the caret
affinity onto the target line to keep vertical navigation progressing. Invalid
heights and foreign snapshot states return
errors without mutation. Page navigation preserves history and the text image;
the demo shifts the viewport by the actual caret displacement, clamps it and
reveals the caret after manual scrolling. IME, focus and synthetic-event guards
also apply to page keys.

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

`Rasterizer::shape` returns immutable `ShapedText` with resolved fonts and glyphs.
`shape.measure(MeasureWidth::Definite(width))` returns logical typographic width,
height and line count; `MinContent` and `MaxContent` use actual line-breaking
passes. Width includes trailing whitespace and excludes ink overhangs. Measurement
reflows a cloned layout, preserving the original shaping; it creates no glyph
masks or RGBA bitmap. Shape options retain font-size/scale limits and the 64 KiB
text limit; measurement does not consume the bitmap dimension/pixel budget.
Shapes retain their font data after the rasterizer is dropped. They are
caller-owned; there is no global shaping cache or automatic font invalidation.
The current shaping API is part of `text-raster`, including its dependencies.

`Rasterizer::paragraph_from_shape(&shape, width)` reflows and rasterizes a saved
shape into a `Paragraph` without font lookup or another shaping pass. Its width
is logical and its fonts/font size/DPI come from the shape (`shape.scale()`).
The receiving rasterizer needs no registered fonts. Raster width, dimension,
pixel and mask budgets remain enforced, so zero width is valid for measurement
but cannot produce a paragraph bitmap. The shape and prior paragraphs stay
unchanged after success or failure; each new paragraph has its own selection
snapshot identity. Retain it while only selection, tint or position changes.

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
then `move_selection` with `Navigation`, or `move_selection_page` with a viewport
height and `PageDirection`, plus an extension flag. The state retains both
endpoint affinities and the horizontal goal for repeated vertical moves.
`selection_caret` and `selection_geometry` use that retained visual state;
`extend_selection` preserves the pointer anchor's affinity. `selection()` gives
grapheme-aligned model byte offsets. `same_position()` compares visual endpoints,
not the saved vertical goal column.

`selection_at_granularity(x, y, SelectionGranularity::Word)` selects the pointed
Unicode word segment; `Line` selects a displayed row including its hard line
break, and `Grapheme` places a caret. Range endpoints snap outward to extended
grapheme boundaries. `extend_selection_granularity` takes the original selection
from the press, coordinates and the same granularity on every drag update; it
keeps that complete initial word/line when crossing it. Whitespace, punctuation,
wrapped lines, empty paragraphs and final empty lines have defined selections.
These operations share the paragraph snapshot/coordinate validation and leave
the text, history and bitmap untouched. The demo handles click timing in its
pointer binding; the text library does not read a system clock.

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
their ECS entities and interaction state; removing a widget clears its focus.
Removing a modal root also closes its nested scopes, restores saved focus when
available and refreshes scene-wide input eligibility/paint. Unknown targets,
buttons used as targets, duplicate IDs and declarations
outside the target subtree are rejected before scene mutation.

`roots()` and `children(&id)` expose declaration order without exposing ECS types.
Child lookup returns `None` for an unknown ID and an empty slice for an existing
empty node. The retained hierarchy lets partial reconciliation walk only the
old and new descendants of the chosen container. Updates remain synchronous
and explicit; there is no automatic dependency tracking. The optional layout
tree is synchronized separately.

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
