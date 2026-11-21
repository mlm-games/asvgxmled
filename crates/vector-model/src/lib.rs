pub mod anim;
pub mod color;
pub mod format;
pub mod model;
pub mod parse;
pub mod path;

pub use anim::{apply as apply_animations, total_duration};
pub use color::{ColorSpec, Rgba};
pub use model::{LinearGradient, Node, Paint, RadialGradient, SweepGradient, VectorDoc};
pub use parse::{Diagnostic, ParseResult, Severity, line_col, parse};
