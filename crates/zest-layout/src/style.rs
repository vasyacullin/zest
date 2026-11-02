use zest_core::geometry::{Logical, Point, Size};

/// Dimensions use logical pixels; percentage values are fractions (0.5 = 50%).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Row,
    #[default]
    Column,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}
impl Edges {
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }
}

/// Initial no-wrap Flexbox style. Sizes, gap and padding must be nonnegative
/// and finite. Minimum sizes default to zero; leaves use caller-provided
/// intrinsic content sizes when their dimensions are auto. Containers measure
/// their children instead. Text/font measurement remains the caller's job.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub axis: Axis,
    pub width: Length,
    pub height: Length,
    pub min_size: Size<Logical>,
    pub intrinsic_size: Size<Logical>,
    pub gap: f32,
    pub padding: Edges,
    pub grow: f32,
    pub shrink: f32,
    pub align: Align,
    pub justify: Justify,
    /// Clip descendants to this node's border bounds. Does not change layout or
    /// clip this node's own paint; viewport/ancestor clips still apply to it.
    pub clip_children: bool,
    /// Requested positive content scroll in logical pixels. Applied to children
    /// only when `clip_children` is true, clamped to the computed scroll range.
    pub scroll_offset: Point<Logical>,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            axis: Axis::Column,
            width: Length::Auto,
            height: Length::Auto,
            min_size: Size::new(0.0, 0.0),
            intrinsic_size: Size::new(0.0, 0.0),
            gap: 0.0,
            padding: Edges::default(),
            grow: 0.0,
            shrink: 1.0,
            align: Align::Stretch,
            justify: Justify::Start,
            clip_children: false,
            scroll_offset: Point::new(0.0, 0.0),
        }
    }
}
pub(crate) fn valid_size(size: Size<Logical>) -> bool {
    [size.width, size.height]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0 && value <= f64::from(f32::MAX))
}
impl Style {
    pub(crate) fn same_layout(self, other: Self) -> bool {
        Self {
            clip_children: false,
            scroll_offset: Point::new(0.0, 0.0),
            ..self
        } == Self {
            clip_children: false,
            scroll_offset: Point::new(0.0, 0.0),
            ..other
        }
    }
    pub(crate) fn invalid_field(self) -> Option<&'static str> {
        for (field, value) in [
            ("scroll_offset.x", self.scroll_offset.x),
            ("scroll_offset.y", self.scroll_offset.y),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Some(field);
            }
        }
        for (field, length) in [("width", self.width), ("height", self.height)] {
            match length {
                Length::Auto => {}
                Length::Px(value) | Length::Percent(value) if !value.is_finite() || value < 0.0 => {
                    return Some(field);
                }
                _ => {}
            }
        }
        for (field, value) in [
            ("gap", self.gap),
            ("padding.left", self.padding.left),
            ("padding.right", self.padding.right),
            ("padding.top", self.padding.top),
            ("padding.bottom", self.padding.bottom),
            ("grow", self.grow),
            ("shrink", self.shrink),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Some(field);
            }
        }
        if !valid_size(self.min_size) {
            return Some("min_size");
        }
        if !valid_size(self.intrinsic_size) {
            return Some("intrinsic_size");
        }
        None
    }
    pub(crate) fn taffy(self) -> taffy::Style {
        use taffy::{
            AlignItems, Dimension, FlexDirection, JustifyContent, LengthPercentage,
            LengthPercentageAuto, Rect, Size,
        };
        let dimension = |length| match length {
            Length::Auto => Dimension::auto(),
            Length::Px(value) => Dimension::length(value),
            Length::Percent(value) => Dimension::percent(value),
        };
        taffy::Style {
            display: taffy::Display::Flex,
            size: Size {
                width: dimension(self.width),
                height: dimension(self.height),
            },
            min_size: Size {
                width: LengthPercentageAuto::length(self.min_size.width as f32),
                height: LengthPercentageAuto::length(self.min_size.height as f32),
            },
            flex_direction: match self.axis {
                Axis::Row => FlexDirection::Row,
                Axis::Column => FlexDirection::Column,
            },
            flex_grow: self.grow,
            flex_shrink: self.shrink,
            gap: Size {
                width: LengthPercentage::length(self.gap),
                height: LengthPercentage::length(self.gap),
            },
            padding: Rect {
                left: LengthPercentage::length(self.padding.left),
                right: LengthPercentage::length(self.padding.right),
                top: LengthPercentage::length(self.padding.top),
                bottom: LengthPercentage::length(self.padding.bottom),
            },
            align_items: match self.align {
                Align::Start => AlignItems::START,
                Align::Center => AlignItems::CENTER,
                Align::End => AlignItems::END,
                Align::Stretch => AlignItems::STRETCH,
            },
            justify_content: match self.justify {
                Justify::Start => JustifyContent::START,
                Justify::Center => JustifyContent::CENTER,
                Justify::End => JustifyContent::END,
                Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
            },
            ..Default::default()
        }
    }
}
