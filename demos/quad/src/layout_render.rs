//! Responsive scene/layout/text preview. Retains one paragraph/image per label.
use super::*;
use zest::{
    FocusDirection, Scene, WidgetId,
    commands::SceneCommands,
    event::{EventControl, EventHandlers, EventPhase},
    geometry::Size,
    input::{ActivationKey, ActivationSource, ActivationState, InputResult},
    layout::{Edges, LayoutSnapshot, LayoutTree, Length, RevealPlan, Style, TextContent},
    semantics::SemanticProperties,
    text::raster::{Paragraph, Rasterizer, ShapeOptions},
};
mod clip_checks;
mod command_checks;
mod diff_checks;
mod event_checks;
mod focus_checks;
mod interaction_checks;
mod metadata_checks;
mod modal_checks;
mod paint_cache;
mod paint_cache_checks;
mod paint_checks;
mod scroll_chain_checks;
mod scroll_checks;
mod scrollbar_checks;
mod scrollbar_input;
mod semantic_checks;
mod semantic_diff_checks;

const CLIP_HEIGHT: f64 = 96.25;

struct Prepared {
    content: TextContent,
    width: f32,
    paragraph: Paragraph,
    image: Option<ImageId>,
}
struct Label {
    id: WidgetId,
    source: &'static str,
    font_size: f32,
    content: Option<TextContent>,
    prepared: Option<Prepared>,
}
struct ActivationEffects {
    intercept: bool,
    focus_footer: bool,
    commands: SceneCommands,
}
pub(super) struct LayoutDemo {
    scene: Scene,
    tree: LayoutTree,
    raster: Rasterizer,
    root: WidgetId,
    footer: WidgetId,
    labels: Vec<Label>,
    scale: Option<f32>,
    snapshot: Option<LayoutSnapshot>,
    pointer: Option<Point<Logical>>,
    hovered: Option<WidgetId>,
    activation: ActivationState,
    selected: Option<WidgetId>,
    clipped: bool,
    intercept_activation: bool,
    focus_footer: bool,
    activation_handlers: EventHandlers<ActivationSource, ActivationEffects>,
    scroll_offset: Point<Logical>,
    scrollbar_drag: Option<scrollbar_input::Drag>,
    window_focused: bool,
    modifiers: winit::keyboard::ModifiersState,
    paint_cache: zest::paint::DisplayListCache<paint_cache::PaintKey>,
    pub tint: [f32; 4],
}
impl LayoutDemo {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut raster = Rasterizer::new();
        raster.register_font(include_bytes!("../../../assets/fonts/NotoSans.ttf").to_vec())?;
        raster
            .register_font(include_bytes!("../../../assets/fonts/NotoSansArabic.ttf").to_vec())?;
        let mut scene = Scene::default();
        let (mut root, mut footer) = (None, None);
        let mut labels = Vec::new();
        scene.update(|ui| {
            root = Some(ui.container("preview", |ui| {
                for (key, source, font_size) in [
                    ("title", "Zest • Layout", 24.0),
                    (
                        "body",
                        "Resize this window to reflow text. Привет, мир! العربية office e\u{301}.",
                        18.0,
                    ),
                ] {
                    let id = ui.button(key, source);
                    ui.set_semantics(
                        &id,
                        SemanticProperties::default().test_id(format!("layout.{key}")),
                    );
                    labels.push(Label {
                        id,
                        source,
                        font_size,
                        content: None,
                        prepared: None,
                    });
                }
                let id = ui.button("footer", "Layout follows text");
                ui.set_semantics(
                    &id,
                    SemanticProperties::default()
                        .named("Layout status")
                        .described("Follows measured text")
                        .test_id("layout.footer"),
                );
                footer = Some(id);
            }));
            ui.set_semantics(
                root.as_ref().unwrap(),
                SemanticProperties::default()
                    .named("Layout preview")
                    .test_id("layout.root"),
            );
        })?;
        let root = root.unwrap();
        let mut activation_handlers = EventHandlers::new(&scene);
        activation_handlers.set(
            &scene,
            &root,
            EventPhase::Capture,
            |_, _, effects: &mut ActivationEffects| {
                if effects.intercept {
                    EventControl::Consume
                } else {
                    EventControl::Continue
                }
            },
        );
        let focus_target = footer.as_ref().unwrap().clone();
        activation_handlers.set(&scene, &root, EventPhase::Bubble, move |ctx, _, effects| {
            if effects.focus_footer && !ctx.default_prevented {
                effects.commands.focus(ctx.scene(), &focus_target);
            }
            EventControl::Continue
        });
        Ok(Self {
            scene,
            tree: LayoutTree::default(),
            raster,
            root,
            footer: footer.unwrap(),
            labels,
            scale: None,
            snapshot: None,
            pointer: None,
            hovered: None,
            activation: ActivationState::default(),
            selected: None,
            clipped: false,
            intercept_activation: false,
            focus_footer: false,
            activation_handlers,
            scroll_offset: Point::new(0.0, 0.0),
            scrollbar_drag: None,
            window_focused: true,
            modifiers: winit::keyboard::ModifiersState::empty(),
            paint_cache: Default::default(),
            tint: [1.0; 4],
        })
    }
    /// The renderer destroys its images during suspension; CPU paragraphs stay reusable.
    pub fn reset_gpu(&mut self) {
        self.paint_cache.clear();
        for label in &mut self.labels {
            if let Some(prepared) = &mut label.prepared {
                prepared.image = None;
            }
        }
    }
    pub fn prepare(
        &mut self,
        renderer: &mut Renderer,
        viewport: Size<Logical>,
        scale: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update_geometry(viewport, scale)?;
        let snapshot = self.snapshot.as_ref().unwrap();
        for label in &mut self.labels {
            let Some(bounds) = snapshot.bounds(&label.id) else {
                if let Some(old) = label.prepared.take().and_then(|prepared| prepared.image) {
                    renderer.remove_image(old)?;
                }
                continue;
            };
            let width = bounds.size.width as f32;
            if width * scale < 1.0 {
                if let Some(old) = label.prepared.take().and_then(|prepared| prepared.image) {
                    renderer.remove_image(old)?;
                }
                continue;
            }
            let content = label.content.as_ref().unwrap();
            if label
                .prepared
                .as_ref()
                .is_none_or(|prepared| prepared.width != width || &prepared.content != content)
            {
                let paragraph = self.raster.paragraph_from_shape(content.shaped(), width)?;
                let bitmap = paragraph.bitmap();
                let image = renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?;
                if let Some(old) = label.prepared.take().and_then(|prepared| prepared.image) {
                    renderer.remove_image(old)?;
                }
                label.prepared = Some(Prepared {
                    content: content.clone(),
                    width,
                    paragraph,
                    image: Some(image),
                });
            } else if let Some(prepared) = &mut label.prepared
                && prepared.image.is_none()
            {
                let bitmap = prepared.paragraph.bitmap();
                prepared.image =
                    Some(renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?);
            }
        }
        Ok(())
    }
    /// Refresh CPU geometry before position-dependent input, independently of
    /// redraw/upload scheduling. Re-hit a stationary pointer after a resize.
    pub fn update_geometry(
        &mut self,
        viewport: Size<Logical>,
        scale: f32,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        self.update_geometry_impl(viewport, scale, false)
    }
    fn update_geometry_impl(
        &mut self,
        viewport: Size<Logical>,
        scale: f32,
        keep_drag: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.scale != Some(scale) {
            if self.scale.is_some() {
                self.pointer = None;
            }
            for label in &mut self.labels {
                label.content = Some(TextContent::new(self.raster.shape(
                    label.source,
                    &["Noto Sans", "Noto Sans Arabic"],
                    ShapeOptions {
                        font_size: label.font_size,
                        scale,
                    },
                )?));
            }
            self.scale = Some(scale);
        }
        // Bound the demonstration's readable column and each bitmap's physical width.
        let column = viewport
            .width
            .min(800.0)
            .min(4090.0 / f64::from(scale) + 48.0);
        self.tree.sync_with_text(&self.scene, |id| {
            if id == &self.root {
                (
                    Style {
                        width: Length::Px(column as f32),
                        height: Length::Px(CLIP_HEIGHT as f32),
                        clip_children: self.clipped,
                        scroll_offset: self.scroll_offset,
                        padding: Edges::all(24.0),
                        gap: 16.0,
                        shrink: 0.0,
                        ..Default::default()
                    },
                    None,
                )
            } else if id == &self.footer {
                (
                    Style {
                        height: Length::Px(12.0),
                        shrink: 0.0,
                        ..Default::default()
                    },
                    None,
                )
            } else {
                (
                    Style {
                        shrink: 0.0,
                        ..Default::default()
                    },
                    self.labels
                        .iter()
                        .find(|label| &label.id == id)
                        .unwrap()
                        .content
                        .clone(),
                )
            }
        })?;
        let snapshot = self.tree.compute(viewport)?.snapshot;
        self.scroll_offset = snapshot.scroll(&self.root).unwrap_or_default().offset;
        if let Some(old) = &self.snapshot {
            let changes = snapshot.changes_since(old);
            for id in changes.added.iter().chain(&changes.changed) {
                self.scene.invalidate_paint(id);
            }
            if changes.viewport_changed || changes.order_changed || !changes.removed.is_empty() {
                self.scene.invalidate_paint(&self.root);
            }
        }
        let mut changed = self
            .snapshot
            .as_ref()
            .is_none_or(|old| !old.same_snapshot(&snapshot));
        if changed {
            self.activation.cancel(&mut self.scene);
            if !keep_drag {
                self.clear_scrollbar_drag();
            }
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|id| self.scene.state(id).is_none())
        {
            self.selected = None;
            changed = true;
        }
        self.snapshot = Some(snapshot);
        changed |= self.refresh_input();
        Ok(changed)
    }
    fn refresh_input(&mut self) -> bool {
        let mut changed = self.validate_scrollbar_drag();
        changed |= self.activation.reconcile(&mut self.scene);
        let target = self
            .pointer
            .filter(|_| self.window_focused && !self.scrollbar_dragging())
            .and_then(|point| self.hit(point));
        changed |= self.set_hovered(target);
        if self
            .scene
            .focused()
            .is_some_and(|id| !self.window_focused || !self.interactive(id) || !self.visible(id))
        {
            changed |= self.set_focus(None);
        }
        changed
    }
    fn interactive(&self, id: &WidgetId) -> bool {
        (id == &self.footer || self.labels.iter().any(|label| &label.id == id))
            && self.scene.input_allowed(id)
    }
    fn visible(&self, id: &WidgetId) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.visible_bounds(id).is_some_and(|bounds| {
                bounds.size.width * f64::from(self.scale.unwrap_or(1.0)) >= 1.0
                    && bounds.size.height * f64::from(self.scale.unwrap_or(1.0)) >= 1.0
            })
        })
    }
    fn focus_plan(&self, id: &WidgetId) -> Option<RevealPlan> {
        if !self.interactive(id) {
            return None;
        }
        let plan = self.snapshot.as_ref()?.reveal_plan(id)?;
        let scale = f64::from(self.scale.unwrap_or(1.0));
        (plan.visible_bounds.size.width * scale >= 1.0
            && plan.visible_bounds.size.height * scale >= 1.0
            && plan
                .adjustments
                .iter()
                .all(|change| change.widget == self.root))
        .then_some(plan)
    }
    fn reveal_focus(&mut self) -> bool {
        let Some(plan) = self.scene.focused().and_then(|id| self.focus_plan(id)) else {
            return false;
        };
        let Some(change) = plan.adjustments.first() else {
            return false;
        };
        let viewport = self.snapshot.as_ref().unwrap().viewport();
        let previous = self.scroll_offset;
        self.scroll_offset = change.offset;
        if let Err(error) = self.update_geometry(viewport, self.scale.unwrap()) {
            self.scroll_offset = previous;
            eprintln!("Focus reveal rejected: {error}");
            self.set_focus(None);
        }
        true
    }
    fn hit(&self, point: Point<Logical>) -> Option<WidgetId> {
        if self
            .scrollbar()
            .is_some_and(|bar| bar.hit_test(point).is_some())
        {
            return None;
        }
        self.snapshot
            .as_ref()?
            .hit_test(point, |id| self.interactive(id) && self.visible(id))
            .cloned()
    }
    fn set_hovered(&mut self, target: Option<WidgetId>) -> bool {
        if self.hovered == target {
            return false;
        }
        if let Some(old) = self.hovered.take() {
            self.scene.set_hovered(&old, false);
        }
        if let Some(id) = &target {
            self.scene.set_hovered(id, true);
        }
        self.hovered = target;
        true
    }
    fn set_focus(&mut self, target: Option<WidgetId>) -> bool {
        if self.scene.focused() == target.as_ref() {
            return false;
        }
        self.activation.cancel(&mut self.scene);
        match target {
            Some(id) => self.scene.focus(&id),
            None => {
                self.scene.clear_focus();
                true
            }
        }
    }
    fn set_enabled(&mut self, id: &WidgetId, enabled: bool) -> bool {
        if self
            .scene
            .locally_enabled(id)
            .is_none_or(|old| old == enabled)
        {
            return false;
        }
        self.scene.set_enabled(id, enabled);
        self.refresh_input();
        true
    }
    pub fn modal_active(&self) -> bool {
        self.scene.modal_root().is_some()
    }
    fn close_modal(&mut self) -> bool {
        if !self.scene.pop_modal() {
            return false;
        }
        self.refresh_input();
        true
    }
    fn toggle_modal(&mut self) -> bool {
        if self.modal_active() {
            return self.close_modal();
        }
        let body = self.labels[1].id.clone();
        if !self.visible(&body) || !self.scene.push_modal(&body) {
            return false;
        }
        self.refresh_input();
        true
    }
    fn toggle_clip(&mut self) -> bool {
        let Some(viewport) = self.snapshot.as_ref().map(LayoutSnapshot::viewport) else {
            return false;
        };
        let scale = self.scale.unwrap();
        let previous_scroll = self.scroll_offset;
        self.clipped = !self.clipped;
        if !self.clipped {
            self.scroll_offset = Point::new(0.0, 0.0);
        }
        match self.update_geometry(viewport, scale) {
            Ok(_) => true,
            Err(error) => {
                self.clipped = !self.clipped;
                self.scroll_offset = previous_scroll;
                eprintln!("Layout clip change rejected: {error}");
                false
            }
        }
    }
    /// Wheel deltas move content, opposite to its offset. As in the editor,
    /// lines use 36 logical pixels and touchpad deltas use physical pixels.
    pub fn wheel(
        &mut self,
        delta: winit::event::MouseScrollDelta,
        scale: ScaleFactor,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if !self.window_focused || !self.clipped || self.unsupported_modifiers() {
            return Ok(false);
        }
        let Some(snapshot) = &self.snapshot else {
            return Ok(false);
        };
        let Some(pointer) = self.pointer else {
            return Ok(false);
        };
        if !snapshot
            .visible_bounds(&self.root)
            .is_some_and(|bounds| bounds.contains(pointer))
            || self
                .scene
                .state(&self.root)
                .is_none_or(|state| !state.enabled())
        {
            return Ok(false);
        }
        let (mut x, mut y) = match delta {
            winit::event::MouseScrollDelta::LineDelta(x, y) => {
                (f64::from(x) * 36.0, f64::from(y) * 36.0)
            }
            winit::event::MouseScrollDelta::PixelDelta(point) => {
                (point.x / scale.get(), point.y / scale.get())
            }
        };
        if !x.is_finite() || !y.is_finite() || (x == 0.0 && y == 0.0) {
            return Ok(false);
        }
        if self.modifiers.shift_key() && x == 0.0 {
            x = y;
            y = 0.0;
        }
        let Some(target) = snapshot.hit_test(pointer, |id| self.scene.input_allowed(id)) else {
            return Ok(false);
        };
        let Some(plan) = snapshot.scroll_plan(target, Point::new(-x, -y), self.scene.modal_root())
        else {
            return Ok(false);
        };
        // The preview owns one scrollport; the general plan can update multiple
        // ancestor styles in applications with nested scrollable containers.
        let next = plan
            .adjustments
            .iter()
            .find(|change| change.widget == self.root)
            .map(|change| change.offset);
        let cancelled = self.clear_scrollbar_drag();
        let changed = match next {
            Some(offset) => self.scroll_to(offset, false)?,
            None => false,
        };
        if cancelled {
            self.refresh_input();
        }
        Ok(changed || cancelled)
    }
    pub fn pointer_moved(&mut self, point: Point<Logical>) -> bool {
        if !self.window_focused || !point.x.is_finite() || !point.y.is_finite() {
            return false;
        }
        let was_hovered = self.scrollbar_hovered();
        self.pointer = Some(point);
        if let Some(changed) = self.drag_scrollbar(point) {
            return self.refresh_input() || changed;
        }
        let target = self.hit(point);
        let changed = self
            .activation
            .pointer_move(&mut self.scene, target.as_ref());
        self.set_hovered(target) || changed || was_hovered != self.scrollbar_hovered()
    }
    pub fn pointer_left(&mut self) -> bool {
        let was_hovered = self.scrollbar_hovered();
        self.pointer = None;
        let changed = self.activation.pointer_move(&mut self.scene, None);
        self.set_hovered(None) || changed || was_hovered
    }
    pub fn cancel_pointer(&mut self) -> bool {
        let dragged = self.clear_scrollbar_drag();
        let changed = self.activation.cancel(&mut self.scene);
        self.pointer_left() || changed || dragged
    }
    pub fn pointer_press(&mut self) -> bool {
        if !self.window_focused || self.activation.pointer_down() {
            return false;
        }
        if let Some(changed) = self.press_scrollbar() {
            return changed;
        }
        let target = self.pointer.and_then(|point| self.hit(point));
        let changed = if target.is_some() || !self.modal_active() {
            self.set_focus(target.clone())
        } else {
            false
        };
        let result = self
            .activation
            .pointer_press(&mut self.scene, target.as_ref());
        self.apply_input(result) || changed
    }
    pub fn pointer_release(&mut self) -> bool {
        if !self.window_focused {
            return false;
        }
        if self.clear_scrollbar_drag() {
            self.activation.pointer_release(&mut self.scene, None);
            self.refresh_input();
            return true;
        }
        let target = self.pointer.and_then(|point| self.hit(point));
        let result = self
            .activation
            .pointer_release(&mut self.scene, target.as_ref());
        self.apply_input(result)
    }
    fn apply_input(&mut self, result: InputResult) -> bool {
        let mut changed = result.changed;
        if let Some(action) = result.activated {
            let mut source = action.source;
            let mut effects = ActivationEffects {
                intercept: self.intercept_activation,
                focus_footer: self.focus_footer,
                commands: SceneCommands::new(&self.scene),
            };
            let dispatch = self.activation_handlers.dispatch(
                &self.scene,
                &action.widget,
                &mut source,
                &mut effects,
            );
            if dispatch.is_some_and(|result| !result.default_prevented) {
                if let Some(old) = &self.selected {
                    self.scene.invalidate_paint(old);
                }
                self.scene.invalidate_paint(&action.widget);
                self.selected = if self.selected.as_ref() == Some(&action.widget) {
                    None
                } else {
                    Some(action.widget)
                };
                changed = true;
            }
            let stats = effects.commands.apply(&mut self.scene).unwrap();
            if stats.applied != 0 {
                changed |= stats.changed;
                changed |= self.refresh_input();
            }
        }
        changed
    }
    pub fn focus_window(&mut self, focused: bool) -> bool {
        self.window_focused = focused;
        if focused {
            return false;
        }
        self.modifiers = winit::keyboard::ModifiersState::empty();
        let dragged = self.clear_scrollbar_drag();
        let changed = self.activation.reset(&mut self.scene) | self.pointer_left();
        self.set_focus(None) || changed || dragged
    }
    pub fn modifiers(&mut self, modifiers: winit::keyboard::ModifiersState) -> bool {
        self.modifiers = if self.window_focused {
            modifiers
        } else {
            winit::keyboard::ModifiersState::empty()
        };
        if self.unsupported_modifiers() {
            let changed = self.activation.cancel(&mut self.scene);
            let dragged = self.clear_scrollbar_drag();
            if dragged {
                self.refresh_input();
            }
            dragged || changed
        } else {
            false
        }
    }
    fn unsupported_modifiers(&self) -> bool {
        self.modifiers.control_key() || self.modifiers.alt_key() || self.modifiers.super_key()
    }
    pub fn key(&mut self, key: &winit::keyboard::Key, pressed: bool, synthetic: bool) -> bool {
        self.key_event(key, pressed, false, synthetic)
    }
    pub fn key_event(
        &mut self,
        key: &winit::keyboard::Key,
        pressed: bool,
        repeat: bool,
        synthetic: bool,
    ) -> bool {
        if !self.window_focused || synthetic {
            return false;
        }
        let command = matches!(
            key,
            winit::keyboard::Key::Named(
                winit::keyboard::NamedKey::Tab
                    | winit::keyboard::NamedKey::Enter
                    | winit::keyboard::NamedKey::Space
                    | winit::keyboard::NamedKey::Escape
                    | winit::keyboard::NamedKey::F2
                    | winit::keyboard::NamedKey::F3
                    | winit::keyboard::NamedKey::F4
                    | winit::keyboard::NamedKey::F6
                    | winit::keyboard::NamedKey::F7
            )
        ) || matches!(key, winit::keyboard::Key::Character(value) if value == " ");
        let single_press = matches!(
            key,
            winit::keyboard::Key::Named(
                winit::keyboard::NamedKey::Escape
                    | winit::keyboard::NamedKey::F2
                    | winit::keyboard::NamedKey::F3
                    | winit::keyboard::NamedKey::F4
                    | winit::keyboard::NamedKey::F6
                    | winit::keyboard::NamedKey::F7
            )
        );
        let cancelled =
            pressed && command && (!single_press || !repeat) && self.clear_scrollbar_drag();
        let changed = self.key_action(key, pressed, repeat);
        if cancelled {
            self.refresh_input();
        }
        changed || cancelled
    }
    fn key_action(&mut self, key: &winit::keyboard::Key, pressed: bool, repeat: bool) -> bool {
        if key == &winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) {
            return pressed && !repeat && self.close_modal();
        }
        if key == &winit::keyboard::Key::Named(winit::keyboard::NamedKey::F3) {
            return pressed && !repeat && !self.unsupported_modifiers() && self.toggle_modal();
        }
        if key == &winit::keyboard::Key::Named(winit::keyboard::NamedKey::F4) {
            return pressed && !repeat && !self.unsupported_modifiers() && self.toggle_clip();
        }
        if key == &winit::keyboard::Key::Named(winit::keyboard::NamedKey::F6) {
            if !pressed || repeat || self.unsupported_modifiers() {
                return false;
            }
            self.intercept_activation = !self.intercept_activation;
            self.scene.invalidate_paint(&self.root);
            return true;
        }
        if key == &winit::keyboard::Key::Named(winit::keyboard::NamedKey::F7) {
            if !pressed || repeat || self.unsupported_modifiers() {
                return false;
            }
            self.focus_footer = !self.focus_footer;
            self.scene.invalidate_paint(&self.root);
            return true;
        }
        if key == &winit::keyboard::Key::Named(winit::keyboard::NamedKey::F2) {
            if !pressed || repeat || self.unsupported_modifiers() {
                return false;
            }
            let body = self.labels[1].id.clone();
            let Some(enabled) = self.scene.locally_enabled(&body) else {
                return false;
            };
            return self.set_enabled(&body, !enabled);
        }
        let activation_key = match key {
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter) => {
                Some(ActivationKey::Enter)
            }
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) => {
                Some(ActivationKey::Space)
            }
            winit::keyboard::Key::Character(value) if value == " " => Some(ActivationKey::Space),
            _ => None,
        };
        if let Some(key) = activation_key {
            let target = self
                .scene
                .focused()
                .filter(|id| {
                    !self.unsupported_modifiers() && self.interactive(id) && self.visible(id)
                })
                .cloned();
            let result = self.activation.key(
                &mut self.scene,
                key,
                pressed,
                repeat,
                false,
                target.as_ref(),
            );
            return self.apply_input(result);
        }
        if !pressed
            || self.unsupported_modifiers()
            || key != &winit::keyboard::Key::Named(winit::keyboard::NamedKey::Tab)
        {
            return false;
        }
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let cancelled = self.activation.cancel(&mut self.scene);
        let eligible: std::collections::HashSet<_> = snapshot
            .boxes()
            .iter()
            .filter(|node| self.focus_plan(&node.id).is_some())
            .map(|node| node.id.clone())
            .collect();
        let direction = if self.modifiers.shift_key() {
            FocusDirection::Backward
        } else {
            FocusDirection::Forward
        };
        let focused = self.scene.focus_next(direction, |id| eligible.contains(id));
        self.reveal_focus() || focused || cancelled
    }
    /// Called after painting this scene, without intervening scene edits.
    /// An unsuccessful/clear-only frame must retain the pending CPU work.
    pub fn finish_frame(&mut self, status: RenderStatus, scene_painted: bool) {
        if scene_painted && status == RenderStatus::Presented {
            self.scene.take_paint_invalidation();
        }
    }
    pub fn paint(&self) -> Result<DisplayList, PaintError> {
        let Some(snapshot) = &self.snapshot else {
            return Ok(DisplayList::default());
        };
        let viewport = snapshot.viewport();
        if viewport.width <= 0.0 || viewport.height <= 0.0 {
            return Ok(DisplayList::default());
        }
        DisplayList::build(|painter| {
            painter.with_clip(
                Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height),
                |painter| {
                    painter.quad(Quad {
                        bounds: Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height),
                        color: [0.0, 0.0, 0.0, 1.0],
                    })?;
                    if self.intercept_activation
                        && let Some(bounds) = snapshot.visible_bounds(&self.root)
                    {
                        painter.quad(Quad {
                            bounds: Rect::from_xywh(
                                bounds.origin.x,
                                bounds.origin.y,
                                bounds.size.width,
                                bounds.size.height.min(2.0),
                            ),
                            color: [1.0, 0.4, 0.05, 1.0],
                        })?;
                    }
                    if self.focus_footer
                        && let Some(bounds) = snapshot.visible_bounds(&self.root)
                    {
                        let height = bounds.size.height.min(2.0);
                        painter.quad(Quad {
                            bounds: Rect::from_xywh(
                                bounds.origin.x,
                                bounds.origin.y + bounds.size.height - height,
                                bounds.size.width,
                                height,
                            ),
                            color: [0.1, 0.8, 1.0, 1.0],
                        })?;
                    }
                    for node in snapshot.boxes() {
                        painter.with_clip(node.clip, |painter| {
                            let bounds = node.bounds;
                            if node.id == self.footer {
                                if !bounds.is_empty() {
                                    let state = self.scene.state(&self.footer).unwrap();
                                    painter.quad(Quad {
                                        bounds,
                                        color: if !self.scene.input_allowed(&self.footer) {
                                            [0.12, 0.12, 0.12, 1.0]
                                        } else if state.pressed() {
                                            [0.05, 0.3, 0.6, 1.0]
                                        } else if self.selected.as_ref() == Some(&self.footer) {
                                            [0.1, 0.75, 0.3, 1.0]
                                        } else if state.focused() {
                                            [0.2, 0.6, 1.0, 1.0]
                                        } else if state.hovered() {
                                            [0.4, 0.8, 1.0, 1.0]
                                        } else {
                                            [1.0; 4]
                                        },
                                    })?;
                                }
                                return Ok(());
                            }
                            let Some(label) = self.labels.iter().find(|label| label.id == node.id)
                            else {
                                return Ok(());
                            };
                            let Some(prepared) = &label.prepared else {
                                return Ok(());
                            };
                            let Some(id) = prepared.image else {
                                return Ok(());
                            };
                            let state = self.scene.state(&label.id).unwrap();
                            if !bounds.is_empty()
                                && (state.pressed() || state.focused() || state.hovered())
                            {
                                painter.quad(Quad {
                                    bounds,
                                    color: if state.pressed() {
                                        [0.15, 0.4, 0.65, 1.0]
                                    } else if state.focused() {
                                        [0.08, 0.18, 0.3, 1.0]
                                    } else {
                                        [0.035, 0.06, 0.09, 1.0]
                                    },
                                })?;
                            }
                            let bitmap = prepared.paragraph.bitmap();
                            let scale = f64::from(bitmap.scale);
                            let mut image = Image::new(
                                id,
                                Rect::from_xywh(
                                    bounds.origin.x + f64::from(bitmap.origin_x) / scale,
                                    bounds.origin.y + f64::from(bitmap.origin_y) / scale,
                                    f64::from(bitmap.width) / scale,
                                    f64::from(bitmap.height) / scale,
                                ),
                            );
                            image.tint = self.tint;
                            if !self.scene.input_allowed(&label.id) {
                                for channel in &mut image.tint[..3] {
                                    *channel *= 0.35;
                                }
                            }
                            painter.image(image)?;
                            if self.selected.as_ref() == Some(&label.id) && !bounds.is_empty() {
                                let height = bounds.size.height.min(2.0);
                                painter.quad(Quad {
                                    bounds: Rect::from_xywh(
                                        bounds.origin.x,
                                        bounds.origin.y + bounds.size.height - height,
                                        bounds.size.width,
                                        height,
                                    ),
                                    color: if self.scene.input_allowed(&label.id) {
                                        [0.1, 0.75, 0.3, 1.0]
                                    } else {
                                        [0.12, 0.12, 0.12, 1.0]
                                    },
                                })?;
                            }
                            if self.scene.modal_root() == Some(&label.id) && !bounds.is_empty() {
                                painter.quad(Quad {
                                    bounds: Rect::from_xywh(
                                        bounds.origin.x,
                                        bounds.origin.y,
                                        bounds.size.width,
                                        bounds.size.height.min(2.0),
                                    ),
                                    color: [1.0, 0.6, 0.1, 1.0],
                                })?;
                            }
                            Ok(())
                        })?;
                    }
                    self.paint_scrollbar(painter)?;
                    Ok(())
                },
            )
        })
    }
    fn remove_images(&mut self, renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
        self.paint_cache.clear();
        for label in &mut self.labels {
            if let Some(image) = label
                .prepared
                .as_mut()
                .and_then(|prepared| prepared.image.take())
            {
                renderer.remove_image(image)?;
            }
        }
        Ok(())
    }
}

pub(super) fn check(renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let live = renderer.image_stats().live;
    let mut demo = LayoutDemo::new()?;
    let mut previous_height = 0.0;
    for width in [480.0, 240.0] {
        demo.prepare(renderer, Size::new(width, 450.0), 1.25)?;
        let snapshot = demo.snapshot.as_ref().unwrap();
        let height = snapshot.bounds(&demo.labels[1].id).unwrap().size.height;
        if previous_height > 0.0 && height <= previous_height {
            return Err("narrow layout made the wrapped paragraph shorter".into());
        }
        previous_height = height;
        let footer = snapshot.bounds(&demo.footer).unwrap();
        let capture = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 600, 563)?;
        expect_pixel(&capture, 5, 5, [0, 0, 0, 255])?;
        let (x, y) = opaque_title_pixel(&demo);
        expect_pixel(&capture, x, y, [255; 4])?;
        expect_pixel(
            &capture,
            ((footer.origin.x + 2.0) * 1.25) as u32,
            ((footer.origin.y + 2.0) * 1.25) as u32,
            [255; 4],
        )?;
    }
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let staged = renderer.upload_stats().staged_bytes;
    let guard = demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .layout_selection(zest::text::Selection::caret(0))?;
    let images: Vec<_> = demo
        .labels
        .iter()
        .map(|label| label.prepared.as_ref().unwrap().image)
        .collect();
    demo.tint = [0.5, 0.8, 1.0, 1.0];
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    let capture = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    let (x, y) = opaque_title_pixel(&demo);
    expect_pixel(&capture, x, y, [188, 231, 255, 255])?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if !snapshot.same_snapshot(demo.snapshot.as_ref().unwrap())
        || renderer.upload_stats().staged_bytes != staged
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err("tint/unchanged layout rerasterized or reuploaded text".into());
    }
    interaction_checks::check(&mut demo, renderer)?;
    clip_checks::check(&mut demo, renderer)?;
    scroll_checks::check(&mut demo, renderer)?;
    focus_checks::check(&mut demo, renderer)?;
    scrollbar_checks::check(&mut demo, renderer)?;
    scroll_chain_checks::check(renderer)?;
    event_checks::check(&mut demo, renderer)?;
    paint_checks::check(&mut demo, renderer)?;
    diff_checks::check(&mut demo, renderer)?;
    paint_cache_checks::check(&mut demo, renderer)?;
    command_checks::check(&mut demo, renderer)?;
    semantic_checks::check(&mut demo, renderer)?;
    metadata_checks::check(&mut demo, renderer)?;
    semantic_diff_checks::check(&mut demo, renderer)?;
    demo.remove_images(renderer)?;
    demo.reset_gpu();
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    demo.prepare(renderer, Size::new(40.0, 40.0), 1.25)?;
    let tiny = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 50, 50)?;
    expect_pixel(&tiny, 25, 25, [0, 0, 0, 255])?;
    demo.prepare(renderer, Size::new(0.0, 0.0), 1.25)?;
    demo.paint()?;
    demo.prepare(renderer, Size::new(480.0, 450.0), 1.5)?;
    let capture = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.5)?, 720, 675)?;
    let (x, y) = opaque_title_pixel(&demo);
    expect_pixel(&capture, x, y, [188, 231, 255, 255])?;
    demo.remove_images(renderer)?;
    renderer.wait_idle()?;
    if renderer.image_stats().live != live || renderer.image_stats().retired != 0 {
        return Err("layout text image replacement leaked textures".into());
    }
    println!(
        "GPU layout checks passed: responsive paragraph/footer bounds, shared-shape rasterization, tint/image reuse, zero viewport, DPI and image restoration/retirement"
    );
    Ok(())
}

fn opaque_title_pixel(demo: &LayoutDemo) -> (u32, u32) {
    let label = &demo.labels[0];
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&label.id).unwrap();
    let bitmap = label.prepared.as_ref().unwrap().paragraph.bitmap();
    let index = bitmap
        .rgba8
        .chunks_exact(4)
        .position(|pixel| pixel[3] == 255)
        .expect("demo title has opaque glyph pixels");
    let x = (bounds.origin.x * f64::from(bitmap.scale)).round() as i32
        + bitmap.origin_x
        + (index % bitmap.width as usize) as i32;
    let y = (bounds.origin.y * f64::from(bitmap.scale)).round() as i32
        + bitmap.origin_y
        + (index / bitmap.width as usize) as i32;
    (
        u32::try_from(x).expect("positive title margin"),
        u32::try_from(y).expect("positive title margin"),
    )
}

#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod paint_cache_tests;
#[cfg(test)]
mod paint_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod scrollbar_tests;
#[cfg(test)]
mod tests;
