use super::*;
use zest::{
    paint::Painter,
    text::raster::{RasterOptions, RasterizedText, Rasterizer},
};

const FAMILIES: &[&str] = &["Noto Sans", "Noto Sans Arabic"];
const SAMPLE: &str = "Zest • Привет, мир!\noffice e\u{301} • العربية";

pub(super) struct TextDemo {
    raster: Rasterizer,
    image: Option<ImageId>,
    bitmap: Option<RasterizedText>,
}
impl TextDemo {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut raster = Rasterizer::new();
        raster.register_font(include_bytes!("../../../assets/fonts/NotoSans.ttf").to_vec())?;
        raster
            .register_font(include_bytes!("../../../assets/fonts/NotoSansArabic.ttf").to_vec())?;
        Ok(Self {
            raster,
            image: None,
            bitmap: None,
        })
    }
    pub fn reset(&mut self) {
        self.image = None;
        self.bitmap = None;
    }
    pub fn prepare(
        &mut self,
        renderer: &mut Renderer,
        scale: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.bitmap.as_ref().is_some_and(|b| b.scale == scale) {
            return Ok(());
        }
        let bitmap = self.raster.rasterize(
            SAMPLE,
            FAMILIES,
            RasterOptions {
                font_size: 18.0,
                width: 320.0,
                scale,
            },
        )?;
        if bitmap.metrics.missing_glyphs != 0 {
            return Err("demo fonts did not cover the sample text".into());
        }
        let id = renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?;
        self.bitmap = Some(bitmap);
        if let Some(old) = self.image.replace(id) {
            renderer.remove_image(old)?;
        }
        Ok(())
    }
    pub fn paint(&self, painter: &mut Painter<'_>) -> Result<(), PaintError> {
        let bitmap = self.bitmap.as_ref().expect("prepare before paint");
        let scale = f64::from(bitmap.scale);
        let mut image = Image::new(
            self.image.unwrap(),
            Rect::from_xywh(
                300.0 + f64::from(bitmap.origin_x) / scale,
                280.0 + f64::from(bitmap.origin_y) / scale,
                f64::from(bitmap.width) / scale,
                f64::from(bitmap.height) / scale,
            ),
        );
        image.tint = [0.82, 0.9, 1.0, 1.0];
        painter.with_clip(Rect::from_xywh(280.0, 272.0, 350.0, 84.0), |p| {
            p.image(image)
        })
    }

    pub fn check(&mut self, renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
        let live = renderer.image_stats().live;
        for scale in [1.0, 1.25, 1.5] {
            let bitmap = self.raster.rasterize(
                "office Привет\nلا العربية e\u{301}",
                FAMILIES,
                RasterOptions {
                    width: 190.0,
                    scale,
                    ..Default::default()
                },
            )?;
            if bitmap.metrics.missing_glyphs != 0
                || bitmap.metrics.rtl_runs == 0
                || bitmap.metrics.fonts != 2
            {
                return Err(format!("text shaping/fallback failed: {:?}", bitmap.metrics).into());
            }
            let id = renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?;
            // An opaque black quad gives a known linear background. The white
            // texture's coverage must be blended in linear space then sRGB encoded.
            let clip = (scale == 1.25).then(|| {
                Rect::<Logical>::from_xywh(
                    7.0 / f64::from(scale),
                    3.0 / f64::from(scale),
                    f64::from(bitmap.width - 16) / f64::from(scale),
                    f64::from(bitmap.height - 8) / f64::from(scale),
                )
            });
            let list = DisplayList::build(|p| {
                p.quad(Quad {
                    bounds: Rect::from_xywh(
                        0.0,
                        0.0,
                        f64::from(bitmap.width) / f64::from(scale),
                        f64::from(bitmap.height) / f64::from(scale),
                    ),
                    color: [0.0, 0.0, 0.0, 1.0],
                })?;
                let image = Image::new(
                    id,
                    Rect::from_xywh(
                        0.0,
                        0.0,
                        f64::from(bitmap.width) / f64::from(scale),
                        f64::from(bitmap.height) / f64::from(scale),
                    ),
                );
                match clip {
                    Some(clip) => p.with_clip(clip, |p| p.image(image)),
                    None => p.image(image),
                }
            })?;
            let capture = renderer.capture_list(
                &list,
                ScaleFactor::new(f64::from(scale))?,
                bitmap.width,
                bitmap.height,
            )?;
            let mut ink = 0;
            for (index, source) in bitmap.rgba8.chunks_exact(4).enumerate() {
                let x = index as u32 % bitmap.width;
                let y = index as u32 / bitmap.width;
                let clipped = clip.is_some()
                    && (x < 7 || y < 3 || x >= bitmap.width - 9 || y >= bitmap.height - 5);
                let linear = if clipped {
                    0.0
                } else {
                    f64::from(source[3]) / 255.0
                };
                let srgb = if linear <= 0.0031308 {
                    12.92 * linear
                } else {
                    1.055 * linear.powf(1.0 / 2.4) - 0.055
                };
                let expected = (srgb * 255.0).round() as u8;
                expect_pixel(&capture, x, y, [expected, expected, expected, 255])?;
                ink += usize::from(source[3] != 0);
            }
            if ink < 100 {
                return Err("text capture had no meaningful glyph coverage".into());
            }
            renderer.remove_image(id)?;
        }
        renderer.wait_idle()?;
        if renderer.image_stats().live != live {
            return Err("text checks leaked images".into());
        }
        println!(
            "GPU text checks passed: Latin/Cyrillic/Arabic fallback, ligatures, every coverage pixel at DPI 1/1.25/1.5, clipping and image retirement"
        );
        Ok(())
    }
}
