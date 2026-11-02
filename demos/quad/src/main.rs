use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
use zest::{
    geometry::{Logical, Point, Rect, ScaleFactor},
    gpu::{Backend, Capture, Quad, RenderStatus, Renderer, RendererOptions},
    paint::{DisplayList, Image, ImageId, PaintError, Primitive},
};

static LOG_ERRORS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static LOADER_ERRORS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
struct Logger;
impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            if record.level() == log::Level::Error {
                let message = record.args().to_string();
                // Keep loader/environment diagnostics visible and report them
                // separately from renderer/validation failures. The Vulkan
                // callback logs object/queue labels as additional records.
                if message.starts_with("GENERAL [Loader Message") {
                    LOADER_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                } else if !message.starts_with('\t') {
                    LOG_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}
static LOGGER: Logger = Logger;

const BOUNDS: Rect<Logical> = Rect::from_xywh(48.0, 48.0, 240.0, 140.0);

struct App {
    renderer: Option<Renderer>,
    demo_image: Option<ImageId>,
    text: text_render::TextDemo,
    layout: Option<layout_render::LayoutDemo>,
    clipboard: clipboard::SystemClipboard,
    window: Option<Arc<Window>>,
    backend: Backend,
    smoke: bool,
    frames: usize,
    hovered: bool,
    cursor: Option<Point<Logical>>,
    drag_tick: Option<Instant>,
    resized: bool,
    resize_observed: bool,
    zero_checked: bool,
    smoke_complete: bool,
    capture_path: Option<std::path::PathBuf>,
    started: Instant,
    failure: Option<String>,
}

impl App {
    fn fail(&mut self, events: &ActiveEventLoop, error: impl std::fmt::Display) {
        if self.failure.is_none() {
            self.failure = Some(error.to_string());
        }
        events.exit();
    }

    fn draw(&mut self, events: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        let window = self.window.as_ref().unwrap();
        let scale = ScaleFactor::new(window.scale_factor())?;
        let renderer = self.renderer.as_mut().unwrap();
        if let Some(layout) = &mut self.layout {
            let size = window.inner_size();
            layout.prepare(
                renderer,
                zest::geometry::Size::new(
                    f64::from(size.width) / scale.get(),
                    f64::from(size.height) / scale.get(),
                ),
                scale.get() as f32,
            )?;
        } else {
            self.text.prepare(renderer, window.scale_factor() as f32)?;
        }
        if self.text.input.focused
            && self.text.input.ime_enabled
            && let Some(area) = self.text.ime_area()
        {
            window.set_ime_cursor_area(
                LogicalPosition::new(area.origin.x, area.origin.y),
                LogicalSize::new(area.size.width, area.size.height),
            );
        }
        let quad = Quad {
            bounds: BOUNDS,
            color: if self.hovered {
                [0.95, 0.7, 0.08, 1.0]
            } else {
                [0.6, 0.85, 0.04, 1.0]
            },
        };
        // The first two smoke frames exercise clear-only encoding in both slots
        // before either slot has allocated a vertex/upload buffer.
        let clear_only = self.smoke && self.frames < 2;
        let list = if clear_only {
            Arc::new(DisplayList::default())
        } else if let Some(layout) = &mut self.layout {
            layout.cached_paint()?
        } else {
            Arc::new(demo_list(
                self.hovered,
                self.demo_image.unwrap(),
                &self.text,
            )?)
        };
        let status = renderer.render_list(&list, scale)?;
        match status {
            RenderStatus::Presented => {
                let stats = renderer.last_frame_stats();
                let mut material = None;
                let mut draws = 0;
                for primitive in list.primitives() {
                    let next = match primitive {
                        Primitive::Quad(_) => None,
                        Primitive::Image(image) => Some(image.id),
                    };
                    if material != Some(next) {
                        draws += 1;
                        material = Some(next);
                    }
                }
                if stats.draw_calls != draws || stats.quads != list.len() {
                    return Err("batch draw counters disagree with the display list".into());
                }
                if clear_only && (stats.uploaded_bytes != 0 || stats.vertex_capacity != 0) {
                    return Err("clear-only frame allocated or uploaded vertex data".into());
                }
                self.frames += 1;
            }
            RenderStatus::NeedsResize => {
                let size = window.inner_size();
                renderer.resize(size.width, size.height)?;
                window.request_redraw();
            }
            RenderStatus::Suspended | RenderStatus::Skipped => {}
        }
        if let Some(layout) = &mut self.layout {
            layout.finish_frame(status, !clear_only);
        }
        if self.smoke {
            if self.frames >= 6 && !self.resized {
                let _ = window.request_inner_size(PhysicalSize::new(640, 360));
                self.resized = true;
            }
            if self.frames >= 12 && !self.zero_checked {
                renderer.resize(0, 0)?;
                if renderer.render(quad, scale)? != RenderStatus::Suspended {
                    return Err("zero-size surface submitted a frame".into());
                }
                let size = window.inner_size();
                renderer.resize(size.width, size.height)?;
                self.zero_checked = true;
            }
            if self.frames >= 20 && self.zero_checked && self.resize_observed {
                renderer.wait_idle()?;
                check_pixels(renderer)?;
                check_batch_pixels(renderer)?;
                image_checks::check(renderer)?;
                upload_checks::check(renderer)?;
                self.text.check(renderer)?;
                text_render::check_editor(renderer)?;
                layout_render::check(renderer)?;
                if let Some(path) = &self.capture_path {
                    let size = window.inner_size();
                    let capture = renderer.capture_list(&list, scale, size.width, size.height)?;
                    use std::io::Write;
                    let mut file = std::fs::File::create(path)?;
                    write!(file, "P6\n{} {}\n255\n", capture.width, capture.height)?;
                    for pixel in capture.rgba8.chunks_exact(4) {
                        file.write_all(&pixel[..3])?;
                    }
                    println!("Offscreen capture: {}", path.display());
                }
                if LOG_ERRORS.load(std::sync::atomic::Ordering::Relaxed) != 0 {
                    return Err("HAL emitted errors during the GPU smoke test".into());
                }
                self.smoke_complete = true;
                events.exit();
            }
        }
        Ok(())
    }
}

fn demo_list(
    hovered: bool,
    image: ImageId,
    text: &text_render::TextDemo,
) -> Result<DisplayList, PaintError> {
    DisplayList::build(|p| {
        p.quad(Quad {
            bounds: BOUNDS,
            color: if hovered {
                [0.95, 0.7, 0.08, 1.0]
            } else {
                [0.6, 0.85, 0.04, 1.0]
            },
        })?;
        p.with_clip(Rect::from_xywh(320.0, 48.0, 300.0, 220.0), |p| {
            p.quad(Quad {
                bounds: Rect::from_xywh(300.0, 32.0, 360.0, 260.0),
                color: [0.035, 0.055, 0.09, 1.0],
            })?;
            p.with_clip(Rect::from_xywh(336.0, 64.0, 260.0, 180.0), |p| {
                for i in 0..5 {
                    p.quad(Quad {
                        bounds: Rect::from_xywh(
                            310.0 + f64::from(i) * 52.0,
                            70.0 + f64::from(i) * 22.0,
                            100.0,
                            72.0,
                        ),
                        color: [0.05, 0.25 + i as f32 * 0.1, 0.9, 0.75],
                    })?;
                }
                Ok(())
            })
        })?;
        p.quad(Quad {
            bounds: Rect::from_xywh(192.0, 124.0, 160.0, 108.0),
            color: [0.05, 0.25, 0.9, 0.65],
        })?;
        p.with_clip(Rect::from_xywh(80.0, 248.0, 164.0, 80.0), |p| {
            p.image(Image::new(image, Rect::from_xywh(64.0, 240.0, 200.0, 96.0)))
        })?;
        text.paint(p)
    })
}

fn expect_pixel(
    image: &Capture,
    x: u32,
    y: u32,
    expected: [u8; 4],
) -> Result<(), Box<dyn std::error::Error>> {
    let offset = ((y * image.width + x) * 4) as usize;
    let pixel = &image.rgba8[offset..offset + 4];
    if pixel.iter().zip(expected).any(|(&a, b)| a.abs_diff(b) > 2) {
        return Err(format!("batch pixel ({x},{y}): got {pixel:?}, expected {expected:?}").into());
    }
    Ok(())
}

fn check_batch_pixels(renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let full = Rect::from_xywh(0.0, 0.0, 67.0, 63.0);
    let list = DisplayList::build(|p| {
        for _ in 0..128 {
            p.quad(Quad {
                bounds: full,
                color: [1.0, 0.0, 0.0, 1.0],
            })?;
        }
        p.with_clip(Rect::from_xywh(8.0, 8.0, 24.0, 24.0), |p| {
            p.with_clip(Rect::from_xywh(16.0, 0.0, 24.0, 48.0), |p| {
                p.quad(Quad {
                    bounds: full,
                    color: [0.0, 0.0, 1.0, 0.5],
                })
            })
        })
    })?;
    let image = renderer.capture_list(&list, ScaleFactor::default(), 67, 63)?;
    for (x, y) in [(16, 8), (31, 31), (20, 20)] {
        expect_pixel(&image, x, y, [188, 0, 188, 255])?;
    }
    for (x, y) in [(15, 20), (32, 20), (20, 7), (20, 32), (60, 60)] {
        expect_pixel(&image, x, y, [255, 0, 0, 255])?;
    }
    let grown = renderer.last_frame_stats();
    if grown.quads != 129
        || grown.draw_calls != 1
        || grown.uploaded_bytes != 129 * 192
        || grown.vertex_capacity < grown.uploaded_bytes
    {
        return Err("batch growth or draw counters are incorrect".into());
    }
    let reversed = DisplayList::build(|p| {
        for &quad in list.quads().iter().rev() {
            p.quad(quad)?;
        }
        Ok(())
    })?;
    let image = renderer.capture_list(&reversed, ScaleFactor::default(), 67, 63)?;
    expect_pixel(&image, 20, 20, [255, 0, 0, 255])?;
    let empty = renderer.capture_list(&DisplayList::default(), ScaleFactor::default(), 67, 63)?;
    for y in 0..63 {
        for x in 0..67 {
            expect_pixel(&empty, x, y, [33, 39, 48, 255])?;
        }
    }
    let stats = renderer.last_frame_stats();
    if stats.draw_calls != 0
        || stats.uploaded_bytes != 0
        || stats.vertex_capacity != grown.vertex_capacity
    {
        return Err("clear-only capture changed or uploaded the retained vertex buffer".into());
    }
    let image = renderer.capture_list(&list, ScaleFactor::default(), 67, 63)?;
    expect_pixel(&image, 20, 20, [188, 0, 188, 255])?;
    if renderer.last_frame_stats().vertex_capacity != grown.vertex_capacity {
        return Err("steady batch failed to reuse grown buffers".into());
    }
    println!(
        "GPU batch checks passed: 129 quads/one draw, nested clipping, reversed painter order, buffer growth/reuse and clear-only frames"
    );
    Ok(())
}

fn check_pixels(renderer: &mut Renderer) -> Result<Capture, Box<dyn std::error::Error>> {
    let mut opaque = None;
    let background = [0.015, 0.02, 0.03];
    let srgb = |linear: f64| -> u8 {
        let encoded = if linear <= 0.0031308 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    };
    for alpha in [1.0f32, 0.5] {
        // Width deliberately needs row padding; noncentral bounds detect both
        // vertical inversion and incorrect logical-to-physical conversion.
        let image = renderer.capture(
            Quad {
                bounds: Rect::from_xywh(8.0, 12.0, 16.0, 20.0),
                color: [1.0, 0.25, 0.0, alpha],
            },
            ScaleFactor::new(1.5)?,
            67,
            63,
        )?;
        for (x, y, inside) in [
            (12, 18, true),
            (35, 47, true),
            (23, 32, true),
            (36, 30, false),
            (11, 30, false),
            (20, 17, false),
            (20, 48, false),
            (0, 0, false),
            (66, 62, false),
        ] {
            let offset = ((y * image.width + x) * 4) as usize;
            let pixel = &image.rgba8[offset..offset + 4];
            let expected: Vec<_> = background
                .into_iter()
                .zip([1.0, 0.25, 0.0])
                .map(|(bg, fg)| {
                    srgb(if inside {
                        fg * f64::from(alpha) + bg * (1.0 - f64::from(alpha))
                    } else {
                        bg
                    })
                })
                .chain(std::iter::once(255))
                .collect();
            if pixel
                .iter()
                .zip(&expected)
                .any(|(&a, &b)| a.abs_diff(b) > 2)
            {
                return Err(format!(
                    "GPU pixel ({x},{y}), alpha={alpha}: got {pixel:?}, expected {expected:?}"
                )
                .into());
            }
        }
        if alpha == 1.0 {
            opaque = Some(image);
        }
    }
    println!(
        "GPU pixel checks passed: 18 samples, fractional DPI, padded rows, sRGB and alpha blending"
    );
    Ok(opaque.unwrap())
}

impl ApplicationHandler for App {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let window = Arc::new(
                events.create_window(
                    Window::default_attributes()
                        .with_title(if self.layout.is_some() {
                            "Zest — responsive text layout"
                        } else {
                            "Zest — direct HAL quad"
                        })
                        .with_inner_size(LogicalSize::new(800.0, 500.0)),
                )?,
            );
            let size = window.inner_size();
            let mut options = RendererOptions {
                backend: self.backend,
                ..RendererOptions::default()
            };
            if self.smoke {
                options.upload_ring_capacity = 4096;
            }
            let mut renderer =
                Renderer::with_options(window.clone(), size.width, size.height, options)?;
            println!("Backend: {}", renderer.backend());
            for failure in renderer.backend_failures() {
                eprintln!(
                    "Backend {} initialization failed: {}",
                    failure.backend, failure.error
                );
            }
            let pixels: Vec<_> = (0..16)
                .flat_map(|y| {
                    (0..16).flat_map(move |x| {
                        if (x / 4 + y / 4) % 2 == 0 {
                            [255, 160, 32, 255]
                        } else {
                            [32, 160, 255, 255]
                        }
                    })
                })
                .collect();
            self.demo_image = Some(renderer.upload_rgba8(16, 16, &pixels)?);
            println!("Adapter: {}", renderer.adapter_name());
            if self.layout.is_none() {
                println!(
                    "Click/drag to edit; double-click selects words, triple-click selects displayed lines. Arrows move visually, Up/Down preserve the column, Shift selects. PageUp/PageDown move by viewport height. Home/End use line edges; Ctrl/Cmd+Home/End use document edges. Ctrl/Cmd+C/X/V copy/cut/paste. Wheel scrolls. Esc cancels composition, then closes."
                );
                let word_modifier = if cfg!(target_os = "macos") {
                    "Option"
                } else {
                    "Ctrl"
                };
                println!(
                    "{word_modifier}+Left/Right moves by words; add Shift to select. {word_modifier}+Backspace/Delete deletes by words."
                );
            } else {
                println!(
                    "Layout preview: click or Enter/Space toggles selection; Tab/Shift+Tab reveals and focuses nodes. F2 toggles the body label's enabled state. F3 scopes input to the body. F4 clips the column; wheel, drag the scrollbar or click its track to scroll. F6 lets the parent intercept activations (orange top edge). Esc cancels the active drag/scope or closes the window. Resize reflows text. Text/IME editing is available in the default demo."
                );
            }
            self.text.focus(self.layout.is_none() && window.has_focus());
            if let Some(layout) = &mut self.layout {
                layout.focus_window(window.has_focus());
            }
            window.set_ime_allowed(self.text.input.focused);
            window.request_redraw();
            self.window = Some(window);
            self.renderer = Some(renderer);
            Ok(())
        })();
        if let Err(e) = result {
            self.fail(events, e);
        }
    }

    fn suspended(&mut self, _events: &ActiveEventLoop) {
        self.renderer = None;
        self.text.reset();
        if let Some(layout) = &mut self.layout {
            layout.reset_gpu();
            layout.focus_window(false);
        }
        self.cursor = None;
        self.drag_tick = None;
        self.hovered = false;
        self.demo_image = None;
        self.window = None;
    }

    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != id {
            return;
        }
        if let Some(layout) = &mut self.layout {
            let mut changed = false;
            // Geometry may have changed before its resize/DPI redraw arrives.
            if !self.smoke
                && matches!(
                    &event,
                    WindowEvent::CursorMoved { .. }
                        | WindowEvent::MouseInput {
                            button: MouseButton::Left,
                            ..
                        }
                        | WindowEvent::KeyboardInput { .. }
                        | WindowEvent::MouseWheel { .. }
                )
            {
                let size = window.inner_size();
                let scale = window.scale_factor();
                match layout.update_geometry(
                    zest::geometry::Size::new(
                        f64::from(size.width) / scale,
                        f64::from(size.height) / scale,
                    ),
                    scale as f32,
                ) {
                    Ok(updated) => changed |= updated,
                    Err(error) => {
                        self.fail(events, error);
                        return;
                    }
                }
            }
            match &event {
                WindowEvent::KeyboardInput {
                    event,
                    is_synthetic,
                    ..
                } => {
                    if !is_synthetic
                        && event.state == ElementState::Pressed
                        && !event.repeat
                        && event.logical_key
                            == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
                        && (!layout.modal_active() || self.smoke)
                        && !layout.scrollbar_dragging()
                    {
                        events.exit();
                    }
                    if !self.smoke {
                        changed |= layout.key_event(
                            &event.logical_key,
                            event.state == ElementState::Pressed,
                            event.repeat,
                            *is_synthetic,
                        );
                    }
                    if changed {
                        window.request_redraw();
                    }
                    return;
                }
                WindowEvent::Focused(focused) => {
                    if layout.focus_window(*focused) {
                        window.request_redraw();
                    }
                    window.set_ime_allowed(false);
                    return;
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    if layout.modifiers(modifiers.state()) {
                        window.request_redraw();
                    }
                    return;
                }
                WindowEvent::CursorMoved { position, .. } => {
                    if !self.smoke {
                        changed |= layout.pointer_moved(
                            Point::new(position.x, position.y).to_logical(
                                ScaleFactor::new(window.scale_factor())
                                    .expect("positive window DPI"),
                            ),
                        );
                    }
                    if changed {
                        window.request_redraw();
                    }
                    return;
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if !self.smoke && *button == MouseButton::Left {
                        changed |= match state {
                            ElementState::Pressed => layout.pointer_press(),
                            ElementState::Released => layout.pointer_release(),
                        };
                    }
                    if changed {
                        window.request_redraw();
                    }
                    return;
                }
                WindowEvent::CursorLeft { .. } => {
                    if layout.pointer_left() {
                        window.request_redraw();
                    }
                    return;
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if !self.smoke {
                        match layout.wheel(
                            *delta,
                            ScaleFactor::new(window.scale_factor()).expect("positive window DPI"),
                        ) {
                            Ok(updated) => changed |= updated,
                            Err(error) => {
                                eprintln!("Layout scrolling rejected: {error}");
                                changed |= layout.cancel_pointer();
                            }
                        }
                    }
                    if changed {
                        window.request_redraw();
                    }
                    return;
                }
                WindowEvent::Ime(_) => return,
                _ => {}
            }
        }
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                match self.text.key_physical(
                    &event.logical_key,
                    event.physical_key,
                    event.text.as_deref(),
                    event.state == ElementState::Pressed,
                    is_synthetic,
                ) {
                    Ok(result) => {
                        if let Some(action) = result.clipboard {
                            match self.text.clipboard(action, &mut self.clipboard) {
                                Ok(true) => window.request_redraw(),
                                Ok(false) => {}
                                Err(error) => eprintln!("Clipboard action failed: {error}"),
                            }
                        }
                        if result.close {
                            events.exit();
                        }
                        if result.reset_ime {
                            window.set_ime_allowed(false);
                            window.set_ime_allowed(self.text.input.focused);
                        }
                        if result.changed {
                            window.request_redraw();
                        }
                    }
                    Err(error) => {
                        eprintln!("Text input rejected: {error}");
                        self.text.cancel_composition();
                        window.set_ime_allowed(false);
                        window.set_ime_allowed(self.text.input.focused);
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.text.modifiers(modifiers.state()),
            WindowEvent::Ime(ime) => match self.text.ime(&ime) {
                Ok(true) => window.request_redraw(),
                Ok(false) => {}
                Err(error) => {
                    eprintln!("IME input rejected: {error}");
                    self.text.cancel_composition();
                    window.set_ime_allowed(false);
                    window.set_ime_allowed(self.text.input.focused);
                    window.request_redraw();
                }
            },
            WindowEvent::Focused(focused) => {
                self.text.focus(focused);
                self.drag_tick = None;
                window.set_ime_allowed(focused);
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(point) = self
                    .cursor
                    .filter(|&point| text_render::FIELD.contains(point))
                {
                    if let Err(error) = self.text.pointer_press(
                        point,
                        self.text.input.modifiers.shift_key(),
                        Instant::now(),
                    ) {
                        eprintln!("Text hit test rejected: {error}");
                    }
                    window.set_ime_allowed(false);
                    window.set_ime_allowed(true);
                } else {
                    self.text.focus(false);
                    window.set_ime_allowed(false);
                }
                self.drag_tick = None;
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.text.end_drag();
                self.drag_tick = None;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.text.dragging()
                    || self
                        .cursor
                        .is_some_and(|point| text_render::FIELD.contains(point))
                {
                    match self.text.wheel(
                        delta,
                        ScaleFactor::new(window.scale_factor()).expect("positive window DPI"),
                    ) {
                        Ok(true) => window.request_redraw(),
                        Ok(false) => {}
                        Err(error) => {
                            eprintln!("Text scrolling rejected: {error}");
                            self.text.cancel_pointer();
                        }
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if self.smoke && self.resized && size == PhysicalSize::new(640, 360) {
                    self.resize_observed = true;
                }
                if let Err(e) = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .resize(size.width, size.height)
                {
                    self.fail(events, e);
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(layout) = &mut self.layout {
                    layout.cancel_pointer();
                }
                self.cursor = None;
                self.text.cancel_pointer();
                self.drag_tick = None;
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let point = Point::new(position.x, position.y).to_logical(
                    ScaleFactor::new(window.scale_factor()).expect("positive window DPI"),
                );
                let hovered = BOUNDS.contains(point);
                self.cursor = Some(point);
                match self.text.drag_to(point) {
                    Ok(true) => window.request_redraw(),
                    Ok(false) => {}
                    Err(error) => {
                        eprintln!("Text drag rejected: {error}");
                        self.text.cancel_pointer();
                    }
                }
                if hovered != self.hovered {
                    self.hovered = hovered;
                    window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.text.cancel_pointer();
                self.drag_tick = None;
                self.hovered = false;
                window.request_redraw();
            }
            WindowEvent::Occluded(false) => window.request_redraw(),
            WindowEvent::Occluded(true) => {
                if let Some(layout) = &mut self.layout {
                    layout.cancel_pointer();
                }
                self.text.cancel_pointer();
                self.drag_tick = None;
            }
            WindowEvent::RedrawRequested => {
                window.pre_present_notify();
                if let Err(e) = self.draw(events) {
                    self.fail(events, e);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        if self.smoke {
            if self.started.elapsed() > Duration::from_secs(15) {
                self.fail(events, "GPU smoke timed out before completing 20 presents");
                return;
            }
            events.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        } else if self.text.autoscroll_active() {
            let now = Instant::now();
            let previous = *self.drag_tick.get_or_insert(now);
            if now.duration_since(previous) >= Duration::from_millis(16) {
                self.drag_tick = Some(now);
                match self
                    .text
                    .drag_scroll(now.duration_since(previous).as_secs_f64())
                {
                    Ok(true) => {
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        eprintln!("Text autoscroll rejected: {error}");
                        self.text.cancel_pointer();
                    }
                }
            }
            if self.text.autoscroll_active() {
                events.set_control_flow(ControlFlow::WaitUntil(
                    self.drag_tick.unwrap() + Duration::from_millis(16),
                ));
            } else {
                self.drag_tick = None;
                events.set_control_flow(ControlFlow::Wait);
            }
        } else {
            self.drag_tick = None;
            events.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    log::set_logger(&LOGGER).expect("single demo logger");
    log::set_max_level(log::LevelFilter::Warn);
    let mut smoke = false;
    let mut layout_preview = false;
    let mut backend = Backend::Auto;
    let mut capture_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke" => smoke = true,
            "--layout" => layout_preview = true,
            "--backend" => {
                backend = args
                    .next()
                    .ok_or("--backend requires auto, dx12, vulkan or metal")?
                    .parse()?;
            }
            "--capture" => {
                capture_path = Some(std::path::PathBuf::from(
                    args.next().ok_or("--capture requires a PPM path")?,
                ));
                smoke = true;
            }
            "--help" | "-h" => {
                println!(
                    "zest-quad [--layout] [--backend auto|dx12|vulkan|metal] [--smoke] [--capture image.ppm]\n--layout: responsive scene/text layout preview (F2 toggles body; F3 scopes input; F4 clips column; F6 parent intercepts activations; wheel/scrollbar scroll; Esc cancels/unscopes/closes)\n--backend: native API (default auto; explicit choices never fall back)\n--smoke: 20 presents, resize/suspend and offscreen pixel checks\n--capture: run smoke and save the offscreen image as PPM"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let events = EventLoop::new()?;
    let mut app = App {
        renderer: None,
        demo_image: None,
        text: text_render::TextDemo::new()?,
        layout: if layout_preview {
            Some(layout_render::LayoutDemo::new()?)
        } else {
            None
        },
        clipboard: clipboard::SystemClipboard::default(),
        window: None,
        backend,
        smoke,
        frames: 0,
        hovered: false,
        cursor: None,
        drag_tick: None,
        resized: false,
        resize_observed: false,
        zero_checked: false,
        smoke_complete: false,
        capture_path,
        started: Instant::now(),
        failure: None,
    };
    events.run_app(&mut app)?;
    drop(app.clipboard);
    // Include destructor diagnostics in the result, and release the GPU owner
    // before the app's window reference. Closing early must not pass smoke.
    drop(app.renderer.take());
    drop(app.window.take());
    if let Some(failure) = app.failure.take() {
        return Err(failure.into());
    }
    if smoke {
        if !app.smoke_complete {
            return Err("GPU smoke closed before completing its checks".into());
        }
        if LOG_ERRORS.load(std::sync::atomic::Ordering::Relaxed) != 0 {
            return Err("HAL emitted errors during GPU shutdown".into());
        }
        println!(
            "GPU smoke passed: {} presents, observed resize, zero-size suspend/resume and pixel checks; loader diagnostics={}",
            app.frames,
            LOADER_ERRORS.load(std::sync::atomic::Ordering::Relaxed)
        );
    }
    Ok(())
}

mod clipboard;
mod image_checks;
mod layout_render;
mod text_input;
mod text_render;
mod upload_checks;
