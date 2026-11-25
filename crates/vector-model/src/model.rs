use kurbo::BezPath;

use crate::color::ColorSpec;
use crate::path::FillType;

#[derive(Clone, Debug, PartialEq)]
pub enum GradientUnits {
    ObjectBoundingBox,
    UserSpace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: ColorSpec,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub units: GradientUnits,
    pub stops: Vec<GradientStop>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    pub center: (f64, f64),
    pub radius: f64,
    pub units: GradientUnits,
    pub stops: Vec<GradientStop>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SweepGradient {
    pub center: (f64, f64),
    pub units: GradientUnits,
    pub stops: Vec<GradientStop>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Color(ColorSpec),
    Linear(LinearGradient),
    Radial(RadialGradient),
    Sweep(SweepGradient),
}

impl Paint {
    pub fn stops(&self) -> &[GradientStop] {
        match self {
            Self::Color(_) => &[],
            Self::Linear(g) => &g.stops,
            Self::Radial(g) => &g.stops,
            Self::Sweep(g) => &g.stops,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathNode {
    pub name: Option<String>,
    pub path_data: String,
    pub path: BezPath,
    pub fill: Option<Paint>,
    pub stroke: Option<Paint>,
    pub fill_alpha: f32,
    pub stroke_alpha: f32,
    pub stroke_width: f64,
    pub stroke_cap: LineCap,
    pub stroke_join: LineJoin,
    pub stroke_miter: f64,
    pub fill_type: FillType,
    pub trim_start: f64,
    pub trim_end: f64,
    pub trim_offset: f64,
    pub alpha: f32,
    pub span: std::ops::Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupNode {
    pub name: Option<String>,
    pub rotation: f64,
    pub pivot_x: f64,
    pub pivot_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub translate_x: f64,
    pub translate_y: f64,
    pub alpha: f32,
    pub children: Vec<Node>,
    pub span: std::ops::Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipNode {
    pub name: Option<String>,
    pub path_data: String,
    pub path: BezPath,
    pub span: std::ops::Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Path(PathNode),
    Group(GroupNode),
    Clip(ClipNode),
}

impl Node {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Path(n) => n.name.as_deref(),
            Self::Group(n) => n.name.as_deref(),
            Self::Clip(n) => n.name.as_deref(),
        }
    }

    pub fn span(&self) -> std::ops::Range<usize> {
        match self {
            Self::Path(n) => n.span.clone(),
            Self::Group(n) => n.span.clone(),
            Self::Clip(n) => n.span.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Restart,
    Reverse,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnimValue {
    Number(f64),
    Color(ColorSpec),
    Path(String, BezPath),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animator {
    pub property: String,
    pub from: AnimValue,
    pub to: AnimValue,
    pub duration_ms: u64,
    pub start_offset_ms: u64,
    pub repeat_count: i32,
    pub repeat_mode: RepeatMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub target: String,
    pub animators: Vec<Animator>,
}

impl Animation {
    pub fn duration_ms(&self) -> u64 {
        self.animators
            .iter()
            .map(|a| a.start_offset_ms + a.duration_ms)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorDoc {
    pub width: f64,
    pub height: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub alpha: f32,
    pub tint: Option<ColorSpec>,
    pub auto_mirrored: bool,
    pub nodes: Vec<Node>,
    pub animations: Vec<Animation>,
}

impl VectorDoc {
    pub fn empty() -> Self {
        Self {
            width: 24.0,
            height: 24.0,
            viewport_width: 24.0,
            viewport_height: 24.0,
            alpha: 1.0,
            tint: None,
            auto_mirrored: false,
            nodes: Vec::new(),
            animations: Vec::new(),
        }
    }

    pub fn scale(&self) -> (f64, f64) {
        (self.width / self.viewport_width, self.height / self.viewport_height)
    }
}
