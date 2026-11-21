use kurbo::{BezPath, PathEl, Point};

use crate::color::ColorSpec;
use crate::model::{AnimValue, Animation, Animator, Node, Paint, RepeatMode, VectorDoc};

pub fn total_duration(doc: &VectorDoc) -> u64 {
    doc.animations.iter().map(Animation::duration_ms).max().unwrap_or(0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_point(a: Point, b: Point, t: f64) -> Point {
    Point::new(lerp(a.x, b.x, t), lerp(a.y, b.y, t))
}

fn lerp_path(a: &BezPath, b: &BezPath, t: f64) -> BezPath {
    let ea = a.elements();
    let eb = b.elements();
    if ea.len() != eb.len() {
        return if t < 0.5 { a.clone() } else { b.clone() };
    }
    let mut out = BezPath::new();
    for (x, y) in ea.iter().zip(eb.iter()) {
        match (x, y) {
            (PathEl::MoveTo(p), PathEl::MoveTo(q)) => out.move_to(lerp_point(*p, *q, t)),
            (PathEl::LineTo(p), PathEl::LineTo(q)) => out.line_to(lerp_point(*p, *q, t)),
            (PathEl::QuadTo(c, p), PathEl::QuadTo(d, q)) => {
                out.quad_to(lerp_point(*c, *d, t), lerp_point(*p, *q, t));
            }
            (PathEl::CurveTo(c1, c2, p), PathEl::CurveTo(d1, d2, q)) => {
                out.curve_to(lerp_point(*c1, *d1, t), lerp_point(*c2, *d2, t), lerp_point(*p, *q, t));
            }
            (PathEl::ClosePath, PathEl::ClosePath) => out.close_path(),
            _ => return if t < 0.5 { a.clone() } else { b.clone() },
        }
    }
    out
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

fn path_data(path: &BezPath) -> String {
    let mut out = String::new();
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => out.push_str(&format!("M{} {}", fmt(p.x), fmt(p.y))),
            PathEl::LineTo(p) => out.push_str(&format!("L{} {}", fmt(p.x), fmt(p.y))),
            PathEl::QuadTo(c, p) => out.push_str(&format!("Q{} {} {} {}", fmt(c.x), fmt(c.y), fmt(p.x), fmt(p.y))),
            PathEl::CurveTo(c1, c2, p) => out.push_str(&format!(
                "C{} {} {} {} {} {}", fmt(c1.x), fmt(c1.y), fmt(c2.x), fmt(c2.y), fmt(p.x), fmt(p.y)
            )),
            PathEl::ClosePath => out.push('Z'),
        }
    }
    out
}

fn animator_progress(a: &Animator, time_ms: u64) -> f64 {
    let t = time_ms.saturating_sub(a.start_offset_ms);
    if t >= a.duration_ms.max(1) && a.repeat_count == 0 {
        return 1.0;
    }
    let period = a.duration_ms.max(1);
    let cycles = if a.repeat_count < 0 { usize::MAX } else { (a.repeat_count.max(0) + 1) as usize };
    let cycle = (t / period) as usize;
    if cycle >= cycles {
        return 1.0;
    }
    let mut p = (t % period) as f64 / period as f64;
    if a.repeat_mode == RepeatMode::Reverse && cycle % 2 == 1 {
        p = 1.0 - p;
    }
    p.clamp(0.0, 1.0)
}

fn color_of(value: &AnimValue) -> Option<ColorSpec> {
    match value {
        AnimValue::Color(c) => Some(c.clone()),
        AnimValue::Number(n) => ColorSpec::parse(&format!("{n:.0}")),
        AnimValue::Path(_, _) => None,
    }
}

fn number_of(value: &AnimValue) -> Option<f64> {
    match value {
        AnimValue::Number(n) => Some(*n),
        AnimValue::Color(c) => match c {
            ColorSpec::Literal(v) => Some(v.a as f64 / 255.0),
            ColorSpec::Reference(_) => None,
        },
        AnimValue::Path(_, _) => None,
    }
}

pub fn apply(doc: &VectorDoc, time_ms: u64) -> VectorDoc {
    if doc.animations.is_empty() {
        return doc.clone();
    }
    let mut out = doc.clone();
    for animation in &doc.animations {
        for animator in &animation.animators {
            let p = animator_progress(animator, time_ms);
            let value = match (&animator.from, &animator.to) {
                (AnimValue::Number(a), AnimValue::Number(b)) => AnimValue::Number(lerp(*a, *b, p)),
                (AnimValue::Path(_, a), AnimValue::Path(_, b)) => {
                    let path = lerp_path(a, b, p);
                    AnimValue::Path(path_data(&path), path)
                }
                (a, b) => {
                    let ca = color_of(a);
                    let cb = color_of(b);
                    match (ca, cb) {
                        (Some(ColorSpec::Literal(x)), Some(ColorSpec::Literal(y))) => AnimValue::Color(ColorSpec::Literal(
                            crate::color::Rgba {
                                r: lerp(x.r as f64, y.r as f64, p).round() as u8,
                                g: lerp(x.g as f64, y.g as f64, p).round() as u8,
                                b: lerp(x.b as f64, y.b as f64, p).round() as u8,
                                a: lerp(x.a as f64, y.a as f64, p).round() as u8,
                            },
                        )),
                        (Some(c), _) => AnimValue::Color(c),
                        _ => continue,
                    }
                }
            };
            apply_to_node(&mut out.nodes, &animation.target, animator, &value);
        }
    }
    out
}

fn apply_to_node(nodes: &mut [Node], target: &str, animator: &Animator, value: &AnimValue) -> bool {
    for node in nodes.iter_mut() {
        match node {
            Node::Group(g) => {
                if g.name.as_deref() == Some(target) {
                    if let Some(n) = number_of(value) {
                        match animator.property.as_str() {
                            "rotation" => g.rotation = n,
                            "pivotX" => g.pivot_x = n,
                            "pivotY" => g.pivot_y = n,
                            "scaleX" => g.scale_x = n,
                            "scaleY" => g.scale_y = n,
                            "translateX" => g.translate_x = n,
                            "translateY" => g.translate_y = n,
                            "alpha" => g.alpha = n as f32,
                            _ => {}
                        }
                    }
                    return true;
                }
                if apply_to_node(&mut g.children, target, animator, value) {
                    return true;
                }
            }
            Node::Path(p) => {
                if p.name.as_deref() == Some(target) {
                    if let AnimValue::Path(data, path) = value {
                        p.path_data = data.clone();
                        p.path = path.clone();
                    } else if let Some(n) = number_of(value) {
                        match animator.property.as_str() {
                            "fillAlpha" => p.fill_alpha = n as f32,
                            "strokeAlpha" => p.stroke_alpha = n as f32,
                            "strokeWidth" => p.stroke_width = n,
                            "alpha" => p.alpha = n as f32,
                            "trimPathStart" => p.trim_start = n,
                            "trimPathEnd" => p.trim_end = n,
                            "trimPathOffset" => p.trim_offset = n,
                            _ => {}
                        }
                    } else if let AnimValue::Color(c) = value {
                        match animator.property.as_str() {
                            "fillColor" => p.fill = Some(Paint::Color(c.clone())),
                            "strokeColor" => p.stroke = Some(Paint::Color(c.clone())),
                            _ => {}
                        }
                    }
                    return true;
                }
            }
            Node::Clip(_) => {}
        }
    }
    false
}
