//! Where a picture sits on a stage, and how text looks there.
//!
//! Lengths are in stage pixels: a stage has a size (see
//! [`Stage`](crate::Stage)) and every screen that shows it scales the whole
//! stage to fit, so a cue looks the same on a projector and on a tablet.

use serde::{Deserialize, Serialize};

/// How a picture fills the stage before it is moved, scaled and turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// As large as fits whole, keeping its shape.
    #[default]
    Fit,
    /// Covers the stage, keeping its shape and cutting what spills over.
    Fill,
    /// Covers the stage exactly, changing its shape if needed.
    Stretch,
    /// One pixel of the file to one pixel of the stage.
    Native,
}

/// Where a picture or a text sits on its stage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Geometry {
    #[serde(skip_serializing_if = "is_default_fit")]
    pub fit: Fit,
    /// How far its centre is from the stage's centre, in stage pixels:
    /// right and down are positive.
    #[serde(skip_serializing_if = "is_zero")]
    pub x: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub y: f64,
    #[serde(skip_serializing_if = "is_unit")]
    pub scale_x: f64,
    #[serde(skip_serializing_if = "is_unit")]
    pub scale_y: f64,
    /// Degrees, clockwise.
    #[serde(skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// What is cut from each side, as a fraction of the picture (0–1).
    #[serde(skip_serializing_if = "is_zero")]
    pub crop_top: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub crop_right: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub crop_bottom: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub crop_left: f64,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            fit: Fit::Fit,
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            crop_top: 0.0,
            crop_right: 0.0,
            crop_bottom: 0.0,
            crop_left: 0.0,
        }
    }
}

impl Geometry {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The geometry a fraction `t` (0–1) of the way from `self` to `to`. The
    /// fit is not something that moves: it is `to`'s from the start.
    pub fn lerp(&self, to: &Self, t: f64) -> Self {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        Self {
            fit: to.fit,
            x: mix(self.x, to.x),
            y: mix(self.y, to.y),
            scale_x: mix(self.scale_x, to.scale_x),
            scale_y: mix(self.scale_y, to.scale_y),
            rotation: mix(self.rotation, to.rotation),
            crop_top: mix(self.crop_top, to.crop_top),
            crop_right: mix(self.crop_right, to.crop_right),
            crop_bottom: mix(self.crop_bottom, to.crop_bottom),
            crop_left: mix(self.crop_left, to.crop_left),
        }
    }

    /// The same geometry with every value finite and in range: scales of
    /// zero or less, crops past the middle and NaNs become something a stage
    /// can draw.
    pub fn sanitized(&self) -> Self {
        let finite = |value: f64, default: f64| if value.is_finite() { value } else { default };
        let scale = |value: f64| {
            let value = finite(value, 1.0);
            if value.abs() < 0.001 { 0.001 } else { value }
        };
        let crop = |value: f64| finite(value, 0.0).clamp(0.0, 0.49);
        Self {
            fit: self.fit,
            x: finite(self.x, 0.0),
            y: finite(self.y, 0.0),
            scale_x: scale(self.scale_x),
            scale_y: scale(self.scale_y),
            rotation: finite(self.rotation, 0.0),
            crop_top: crop(self.crop_top),
            crop_right: crop(self.crop_right),
            crop_bottom: crop(self.crop_bottom),
            crop_left: crop(self.crop_left),
        }
    }
}

/// How the lines of a text line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// How a Text cue looks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    /// A font family the stage's browser has, or a generic one such as
    /// `serif`.
    #[serde(skip_serializing_if = "is_default_font")]
    pub font: String,
    /// Size in stage pixels.
    #[serde(skip_serializing_if = "is_default_size")]
    pub size: f64,
    /// `#rrggbb`.
    #[serde(skip_serializing_if = "is_default_color")]
    pub color: String,
    #[serde(skip_serializing_if = "is_default_align")]
    pub align: TextAlign,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Line height, as a multiple of the size.
    #[serde(skip_serializing_if = "is_default_line_height")]
    pub line_height: f64,
    /// Lines wrap at this width, in stage pixels; without one they wrap
    /// only at the stage's edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// A box behind the text, `#rrggbb` or `#rrggbbaa`, for surtitles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

pub const DEFAULT_TEXT_SIZE: f64 = 72.0;
const DEFAULT_FONT: &str = "sans-serif";
const DEFAULT_COLOR: &str = "#ffffff";
const DEFAULT_LINE_HEIGHT: f64 = 1.2;

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: DEFAULT_FONT.into(),
            size: DEFAULT_TEXT_SIZE,
            color: DEFAULT_COLOR.into(),
            align: TextAlign::Center,
            bold: false,
            italic: false,
            line_height: DEFAULT_LINE_HEIGHT,
            width: None,
            background: None,
        }
    }
}

impl TextStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

fn is_unit(value: &f64) -> bool {
    *value == 1.0
}

fn is_default_font(font: &str) -> bool {
    font == DEFAULT_FONT
}

fn is_default_size(size: &f64) -> bool {
    *size == DEFAULT_TEXT_SIZE
}

fn is_default_color(color: &str) -> bool {
    color == DEFAULT_COLOR
}

fn is_default_fit(fit: &Fit) -> bool {
    *fit == Fit::Fit
}

fn is_default_align(align: &TextAlign) -> bool {
    *align == TextAlign::Center
}

fn is_default_line_height(value: &f64) -> bool {
    *value == DEFAULT_LINE_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_differs_from_the_default_is_written() {
        let geometry = Geometry {
            x: -120.0,
            scale_x: 0.5,
            scale_y: 0.5,
            ..Geometry::default()
        };
        let json = serde_json::to_value(geometry).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "x": -120.0, "scale_x": 0.5, "scale_y": 0.5 })
        );
        let back: Geometry = serde_json::from_value(json).unwrap();
        assert_eq!(back, geometry);
        let style = TextStyle {
            size: 40.0,
            ..TextStyle::default()
        };
        assert_eq!(
            serde_json::to_value(&style).unwrap(),
            serde_json::json!({ "size": 40.0 })
        );
        assert_eq!(
            serde_json::from_str::<TextStyle>("{\"size\": 40}").unwrap(),
            style
        );
    }

    #[test]
    fn a_geometry_part_way_to_another() {
        let from = Geometry::default();
        let to = Geometry {
            fit: Fit::Fill,
            x: 100.0,
            rotation: 90.0,
            crop_left: 0.2,
            ..Geometry::default()
        };
        let half = from.lerp(&to, 0.5);
        assert_eq!(half.fit, Fit::Fill);
        assert_eq!((half.x, half.rotation, half.crop_left), (50.0, 45.0, 0.1));
        assert_eq!(from.lerp(&to, 1.0), to);
    }

    #[test]
    fn values_a_stage_cannot_draw_are_made_safe() {
        let wild = Geometry {
            x: f64::NAN,
            scale_x: 0.0,
            scale_y: -2.0,
            crop_top: 0.8,
            crop_left: -1.0,
            ..Geometry::default()
        };
        let safe = wild.sanitized();
        assert_eq!(safe.x, 0.0);
        assert_eq!((safe.scale_x, safe.scale_y), (0.001, -2.0));
        assert_eq!((safe.crop_top, safe.crop_left), (0.49, 0.0));
    }
}
