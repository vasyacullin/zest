use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};
use zest::{
    geometry::{Logical, Point, Rect, ScaleFactor},
    gpu::{Capture, Quad, RenderStatus, Renderer},
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
    window: Option<Arc<Window>>,
    smoke: bool,
    frames: usize,
    hovered: bool,
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
        let quad = Quad {
            bounds: BOUNDS,
            color: if self.hovered {
                [0.95, 0.7, 0.08, 1.0]
            } else {
                [0.6, 0.85, 0.04, 1.0]
            },
        };
        match renderer.render(quad, scale)? {
            RenderStatus::Presented => self.frames += 1,
            RenderStatus::NeedsResize => {
                let size = window.inner_size();
                renderer.resize(size.width, size.height)?;
                window.request_redraw();
            }
            RenderStatus::Suspended | RenderStatus::Skipped => {}
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
                let capture = check_pixels(renderer)?;
                if let Some(path) = &self.capture_path {
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
                        .with_title("Zest — direct HAL quad")
                        .with_inner_size(LogicalSize::new(800.0, 500.0)),
                )?,
            );
            let size = window.inner_size();
            let renderer = Renderer::new(window.clone(), size.width, size.height)?;
            println!("Adapter: {}", renderer.adapter_name());
            println!(
                "Hover the quad; resize the window; Esc closes. Rendering sleeps between events."
            );
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
        self.window = None;
    }

    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                events.exit()
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
            WindowEvent::ScaleFactorChanged { .. } => window.request_redraw(),
            WindowEvent::CursorMoved { position, .. } => {
                let point = Point::new(position.x, position.y).to_logical(
                    ScaleFactor::new(window.scale_factor()).expect("positive window DPI"),
                );
                let hovered = BOUNDS.contains(point);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } if self.hovered => {
                self.hovered = false;
                window.request_redraw();
            }
            WindowEvent::Occluded(false) => window.request_redraw(),
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
        } else {
            events.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    log::set_logger(&LOGGER).expect("single demo logger");
    log::set_max_level(log::LevelFilter::Warn);
    let mut smoke = false;
    let mut capture_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke" => smoke = true,
            "--capture" => {
                capture_path = Some(std::path::PathBuf::from(
                    args.next().ok_or("--capture requires a PPM path")?,
                ));
                smoke = true;
            }
            "--help" | "-h" => {
                println!(
                    "zest-quad [--smoke] [--capture image.ppm]\n--smoke: 20 presents, resize/suspend and offscreen pixel checks\n--capture: run smoke and save the offscreen image as PPM"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let events = EventLoop::new()?;
    let mut app = App {
        renderer: None,
        window: None,
        smoke,
        frames: 0,
        hovered: false,
        resized: false,
        resize_observed: false,
        zero_checked: false,
        smoke_complete: false,
        capture_path,
        started: Instant::now(),
        failure: None,
    };
    events.run_app(&mut app)?;
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
