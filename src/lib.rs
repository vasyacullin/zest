//! Zest facade: a headless ECS scene and optional experimental HAL renderer
//! behind the `gpu` feature. The `text` feature adds a window-independent Unicode
//! editing model; `text-raster` adds explicit-font paragraph rasterization.
//! The optional `layout` feature adds retained headless Flexbox scene layout.
//! `layout-text` connects shaped paragraph measurement to Flexbox leaves.
//! Scene widgets are not yet connected to rendering.
//!
//! ```
//! use zest::geometry::{Logical, Point, Rect, ScaleFactor};
//! let scale = ScaleFactor::new(1.25)?;
//! let bounds = Rect::<Logical>::from_xywh(0.0, 0.0, 100.0, 40.0);
//! assert!(bounds.contains(Point::<Logical>::new(10.0, 5.0)));
//! let physical = bounds.to_physical(scale);
//! assert_eq!(physical.size.width, 125.0);
//! # Ok::<(), zest::geometry::InvalidScaleFactor>(())
//! ```
pub use zest_core::commands;
pub use zest_core::event;
pub use zest_core::geometry;
pub use zest_core::input;
pub use zest_core::resource;
pub use zest_core::semantics;
pub use zest_core::{
    FocusDirection, NodeKind, PaintInvalidation, ReconcileError, Scene, SceneStats, Ui,
    UpdateStats, WidgetId, WidgetState,
};
#[cfg(feature = "gpu")]
pub use zest_gpu as gpu;
#[cfg(feature = "layout")]
pub use zest_layout as layout;
pub use zest_paint as paint;
#[cfg(feature = "text")]
pub use zest_text as text;
