use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::color::ColorSpec;
use crate::model::{AnimValue, Animation, Animator, ClipNode, GradientStop, GradientUnits, GroupNode, LineCap, LineJoin, LinearGradient, Node, Paint, PathNode, RadialGradient, RepeatMode, SweepGradient, VectorDoc};
use crate::path::{FillType, parse_path_data};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParseResult {
    pub doc: Option<VectorDoc>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(src.len());
    while end > 0 && !src.is_char_boundary(end) {
        end -= 1;
    }
    let upto = &src[..end];
    let line = upto.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = upto.rsplit('\n').next().map_or(0, |s| s.chars().count()) + 1;
    (line, col)
}

#[derive(Debug)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
    start: usize,
    end: usize,
}

impl Element {
    fn attr(&self, short: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == short || k.ends_with(&format!(":{short}")))
            .map(|(_, v)| v.as_str())
    }

    fn find_child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
}

struct Builder<'a> {
    src: &'a str,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Builder<'a> {
    fn error(&mut self, offset: usize, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic { severity: Severity::Error, message: message.into(), offset });
    }

    fn warn(&mut self, offset: usize, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic { severity: Severity::Warning, message: message.into(), offset });
    }

    fn number(&mut self, el: &Element, name: &str, default: f64) -> f64 {
        match el.attr(name) {
            None => default,
            Some(raw) => match parse_number(raw) {
                Some(v) => v,
                None => {
                    self.warn(el.start, format!("invalid {name} value '{raw}', using {default}"));
                    default
                }
            },
        }
    }

    fn float(&mut self, el: &Element, name: &str, default: f32) -> f32 {
        self.number(el, name, default as f64) as f32
    }

    fn boolean(&mut self, el: &Element, name: &str, default: bool) -> bool {
        match el.attr(name) {
            None => default,
            Some("true") => true,
            Some("false") => false,
            Some(raw) => {
                self.warn(el.start, format!("invalid {name} value '{raw}', using {default}"));
                default
            }
        }
    }

    fn color(&mut self, el: &Element, name: &str) -> Option<ColorSpec> {
        let raw = el.attr(name)?;
        match ColorSpec::parse(raw) {
            Some(c) => Some(c),
            None => {
                self.warn(el.start, format!("unresolved {name} '{raw}', using theme color"));
                Some(ColorSpec::Reference(raw.to_owned()))
            }
        }
    }

    fn dimension(&mut self, el: &Element, name: &str, default: f64) -> f64 {
        let Some(raw) = el.attr(name) else { return default };
        let trimmed = raw.trim().trim_end_matches("dp").trim().trim_end_matches("px").trim();
        match trimmed.parse::<f64>() {
            Ok(v) => v,
            Err(_) => {
                self.warn(el.start, format!("invalid {name} '{raw}', using {default}"));
                default
            }
        }
    }

    fn paint(&mut self, el: &Element, attr_name: &str) -> Option<Paint> {
        if let Some(attr) = el.children.iter().find(|c| {
            c.name == "attr"
                && c.attr("name").is_some_and(|v| v.rsplit(':').next() == Some(attr_name))
        }) {
            if let Some(gradient) = attr.find_child("gradient") {
                return self.gradient(gradient);
            }
            self.warn(attr.start, format!("aapt:attr {attr_name} has no gradient child"));
            return None;
        }
        self.color(el, attr_name).map(Paint::Color)
    }

    fn gradient(&mut self, el: &Element) -> Option<Paint> {
        let kind = el.attr("type").unwrap_or("linear");
        let units = match el.attr("gradientUnits").unwrap_or("objectBoundingBox") {
            "objectBoundingBox" => GradientUnits::ObjectBoundingBox,
            _ => GradientUnits::UserSpace,
        };
        let mut stops = Vec::new();
        for item in el.children.iter().filter(|c| c.name == "item") {
            let offset = match item.attr("offset") {
                Some(raw) => parse_number(raw.trim_end_matches('%')).unwrap_or(0.0)
                    / if raw.ends_with('%') { 100.0 } else { 1.0 },
                None => 0.0,
            };
            let color = match item.attr("color").and_then(ColorSpec::parse) {
                Some(c) => c,
                None => {
                    self.warn(item.start, "gradient item has invalid or missing color");
                    continue;
                }
            };
            stops.push(GradientStop { offset: offset as f32, color });
        }
        if stops.is_empty() {
            self.warn(el.start, "gradient has no valid stops");
            return None;
        }
        match kind {
            "linear" => Some(Paint::Linear(LinearGradient {
                start: (self.number(el, "startX", 0.0), self.number(el, "startY", 0.0)),
                end: (self.number(el, "endX", 0.0), self.number(el, "endY", 0.0)),
                units,
                stops,
            })),
            "radial" => Some(Paint::Radial(RadialGradient {
                center: (self.number(el, "centerX", 0.0), self.number(el, "centerY", 0.0)),
                radius: self.number(el, "radius", 0.0),
                units,
                stops,
            })),
            "sweep" => {
                self.warn(el.start, "sweep gradients are approximated; Android GPU sweep differs");
                Some(Paint::Sweep(SweepGradient {
                    center: (self.number(el, "centerX", 0.0), self.number(el, "centerY", 0.0)),
                    units,
                    stops,
                }))
            }
            other => {
                self.warn(el.start, format!("unknown gradient type '{other}'"));
                None
            }
        }
    }

    fn path_data(&mut self, el: &Element, name: &str) -> Option<(String, kurbo::BezPath)> {
        let raw = el.attr(name)?;
        match parse_path_data(raw) {
            Ok(path) => Some((raw.to_owned(), path)),
            Err(e) => {
                let base = attr_value_offset(self.src, el.start..el.end, name).unwrap_or(el.start);
                self.error(base + e.offset, format!("{name}: {}", e.message));
                None
            }
        }
    }

    fn path_node(&mut self, el: &Element) -> Option<Node> {
        let (path_data, path) = self.path_data(el, "pathData")?;
        let fill_type = match el.attr("fillType").unwrap_or("nonZero") {
            "evenOdd" => FillType::EvenOdd,
            _ => FillType::NonZero,
        };
        Some(Node::Path(PathNode {
            name: el.attr("name").map(str::to_owned),
            path_data,
            path,
            fill: self.paint(el, "fillColor"),
            stroke: self.paint(el, "strokeColor"),
            fill_alpha: self.float(el, "fillAlpha", 1.0),
            stroke_alpha: self.float(el, "strokeAlpha", 1.0),
            stroke_width: self.number(el, "strokeWidth", 0.0),
            stroke_cap: match el.attr("strokeLineCap").unwrap_or("butt") {
                "round" => LineCap::Round,
                "square" => LineCap::Square,
                _ => LineCap::Butt,
            },
            stroke_join: match el.attr("strokeLineJoin").unwrap_or("miter") {
                "round" => LineJoin::Round,
                "bevel" => LineJoin::Bevel,
                _ => LineJoin::Miter,
            },
            stroke_miter: self.number(el, "strokeMiterLimit", 4.0),
            fill_type,
            trim_start: self.number(el, "trimPathStart", 0.0),
            trim_end: self.number(el, "trimPathEnd", 1.0),
            trim_offset: self.number(el, "trimPathOffset", 0.0),
            alpha: self.float(el, "alpha", 1.0),
            span: el.start..el.end,
        }))
    }

    fn group_node(&mut self, el: &Element) -> Option<Node> {
        let mut children = Vec::new();
        for child in &el.children {
            if let Some(n) = self.node(child) {
                children.push(n);
            }
        }
        Some(Node::Group(GroupNode {
            name: el.attr("name").map(str::to_owned),
            rotation: self.number(el, "rotation", 0.0),
            pivot_x: self.number(el, "pivotX", 0.0),
            pivot_y: self.number(el, "pivotY", 0.0),
            scale_x: self.number(el, "scaleX", 1.0),
            scale_y: self.number(el, "scaleY", 1.0),
            translate_x: self.number(el, "translateX", 0.0),
            translate_y: self.number(el, "translateY", 0.0),
            alpha: self.float(el, "alpha", 1.0),
            children,
            span: el.start..el.end,
        }))
    }

    fn node(&mut self, el: &Element) -> Option<Node> {
        match el.name.as_str() {
            "path" => self.path_node(el),
            "group" => self.group_node(el),
            "clip-path" => {
                let (path_data, path) = self.path_data(el, "pathData")?;
                Some(Node::Clip(ClipNode {
                    name: el.attr("name").map(str::to_owned),
                    path_data,
                    path,
                    span: el.start..el.end,
                }))
            }
            "aapt" | "attr" | "gradient" | "item" | "target" | "objectAnimator" | "valueAnimator" | "set" => None,
            other => {
                self.warn(el.start, format!("unsupported element <{other}>"));
                None
            }
        }
    }
}

fn attr_value_offset(src: &str, range: std::ops::Range<usize>, name: &str) -> Option<usize> {
    let chunk = src.get(range.clone())?;
    for needle in [format!("android:{name}=\""), format!("{name}=\"")] {
        if let Some(i) = chunk.find(&needle) {
            return Some(range.start + i + needle.len());
        }
    }
    None
}

fn find_vector(el: &Element) -> Option<&Element> {
    if el.name == "vector" {
        return Some(el);
    }
    el.children.iter().find_map(find_vector)
}

fn collect_targets<'a>(el: &'a Element, out: &mut Vec<(&'a Element, String)>) {
    if el.name == "target" {
        if let Some(name) = el.attr("name") {
            out.push((el, name.to_owned()));
        }
    }
    for child in &el.children {
        collect_targets(child, out);
    }
}

fn repeat_count(raw: Option<&str>) -> i32 {
    match raw {
        Some("infinite") => -1,
        Some(raw) => parse_number(raw).unwrap_or(0.0) as i32,
        None => 0,
    }
}

fn repeat_mode(raw: Option<&str>) -> RepeatMode {
    if raw == Some("reverse") { RepeatMode::Reverse } else { RepeatMode::Restart }
}

fn collect_animators<'a>(
    el: &'a Element,
    out: &mut Vec<(&'a Element, i32, RepeatMode)>,
    inherited: (i32, RepeatMode),
) {
    if matches!(el.name.as_str(), "objectAnimator" | "valueAnimator") {
        out.push((el, inherited.0, inherited.1));
    }
    let next = if el.name == "set" {
        (
            repeat_count(el.attr("repeatCount")),
            repeat_mode(el.attr("repeatMode")),
        )
    } else {
        inherited
    };
    for child in &el.children {
        collect_animators(child, out, next);
    }
}

fn anim_value(raw: &str, value_type: Option<&str>, diagnostics: &mut Vec<Diagnostic>, offset: usize) -> AnimValue {
    if value_type == Some("pathType") || (value_type.is_none() && raw.contains('M')) {
        match parse_path_data(raw) {
            Ok(path) => return AnimValue::Path(raw.to_owned(), path),
            Err(e) => diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                message: format!("invalid pathType animator value: {}", e.message),
                offset,
            }),
        }
    }
    if value_type == Some("colorType") {
        if let Some(c) = ColorSpec::parse(raw) {
            return AnimValue::Color(c);
        }
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            message: format!("invalid colorType animator value '{raw}'"),
            offset,
        });
    }
    AnimValue::Number(parse_number(raw).unwrap_or(0.0))
}

fn parse_animations(root: &Element, diagnostics: &mut Vec<Diagnostic>) -> Vec<Animation> {
    let mut targets = Vec::new();
    collect_targets(root, &mut targets);
    targets
        .into_iter()
        .filter_map(|(el, name)| {
            let mut animators = Vec::new();
            collect_animators(el, &mut animators, (0, RepeatMode::Restart));
            let parsed: Vec<Animator> = animators.iter().filter_map(|(a, set_repeat, set_mode)| {
                let property = a.attr("propertyName")?.to_owned();
                let kind = a.attr("valueType");
                let from = a.attr("valueFrom").map(|v| anim_value(v, kind, diagnostics, a.start));
                let to = a.attr("valueTo").map(|v| anim_value(v, kind, diagnostics, a.start));
                let (from, to) = match (from, to) {
                    (Some(f), Some(t)) => (f, t),
                    (Some(f), None) => (f.clone(), f),
                    (None, Some(t)) => (t.clone(), t),
                    (None, None) => return None,
                };
                Some(Animator {
                    property,
                    from,
                    to,
                    duration_ms: a.attr("duration").and_then(parse_number).unwrap_or(300.0) as u64,
                    start_offset_ms: a.attr("startOffset").and_then(parse_number).unwrap_or(0.0) as u64,
                    repeat_count: if a.attr("repeatCount").is_some() {
                        repeat_count(a.attr("repeatCount"))
                    } else {
                        *set_repeat
                    },
                    repeat_mode: if a.attr("repeatMode").is_some() {
                        repeat_mode(a.attr("repeatMode"))
                    } else {
                        *set_mode
                    },
                })
            }).collect();
            if parsed.is_empty() { None } else { Some(Animation { target: name, animators: parsed }) }
        })
        .collect()
}

fn parse_number(raw: &str) -> Option<f64> {
    raw.trim().parse::<f64>().ok()
}

fn local_name(bytes: &[u8]) -> String {
    let text = std::str::from_utf8(bytes).unwrap_or("");
    text.rsplit(':').next().unwrap_or(text).to_owned()
}

fn parse_tree(src: &str, diagnostics: &mut Vec<Diagnostic>) -> Option<Element> {
    let mut reader = Reader::from_reader(src.as_bytes());
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    loop {
        let start = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(e) => {
                diagnostics.push(Diagnostic { severity: Severity::Error, message: format!("XML: {e}"), offset: start });
                break;
            }
        };
        let is_empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let mut attrs = Vec::new();
                for attr in e.attributes() {
                    match attr {
                        Ok(a) => {
                            let key = local_name(a.key.as_ref());
                            let value = a.normalized_value(XmlVersion::Implicit1_0).map(|v| v.into_owned()).unwrap_or_default();
                            attrs.push((key, value));
                        }
                        Err(err) => diagnostics.push(Diagnostic { severity: Severity::Error, message: format!("XML attribute: {err}"), offset: start }),
                    }
                }
                let element = Element {
                    name: local_name(e.name().as_ref()),
                    attrs,
                    children: Vec::new(),
                    start,
                    end: start,
                };
                if is_empty {
                    let mut element = element;
                    element.end = reader.buffer_position() as usize;
                    attach(element, &mut stack, &mut root, diagnostics);
                } else {
                    stack.push(element);
                }
            }
            Event::End(e) => {
                let end_name = local_name(e.name().as_ref());
                if let Some(mut el) = stack.pop() {
                    if el.name != end_name {
                        diagnostics.push(Diagnostic {
                            severity: Severity::Error,
                            message: format!("mismatched closing tag </{end_name}>, expected </{}>", el.name),
                            offset: start,
                        });
                    }
                    el.end = reader.buffer_position() as usize;
                    attach(el, &mut stack, &mut root, diagnostics);
                } else {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        message: format!("unexpected closing tag </{end_name}>"),
                        offset: start,
                    });
                }
            }
            Event::Eof => {
                for el in &stack {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        message: format!("unclosed element <{}>", el.name),
                        offset: el.start,
                    });
                }
                break;
            }
            _ => {}
        }
    }
    root
}

fn attach(mut el: Element, stack: &mut Vec<Element>, root: &mut Option<Element>, diagnostics: &mut Vec<Diagnostic>) {
    el.end = el.end.max(el.start);
    if let Some(parent) = stack.last_mut() {
        parent.children.push(el);
    } else if root.is_none() {
        *root = Some(el);
    } else {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: format!("multiple root elements: <{}>", el.name),
            offset: el.start,
        });
    }
}

pub fn parse(src: &str) -> ParseResult {
    let mut diagnostics = Vec::new();
    let Some(root) = parse_tree(src, &mut diagnostics) else {
        if diagnostics.is_empty() {
            diagnostics.push(Diagnostic { severity: Severity::Error, message: "no root element".into(), offset: 0 });
        }
        return ParseResult { doc: None, diagnostics };
    };
    let vector = if root.name == "vector" {
        &root
    } else if let Some(inner) = find_vector(&root) {
        inner
    } else {
        diagnostics.push(Diagnostic { severity: Severity::Error, message: format!("expected <vector>, found <{}>", root.name), offset: root.start });
        return ParseResult { doc: None, diagnostics };
    };

    let mut b = Builder { src, diagnostics };
    let width = b.dimension(vector, "width", 24.0);
    let height = b.dimension(vector, "height", 24.0);
    let viewport_width = b.number(vector, "viewportWidth", width);
    let viewport_height = b.number(vector, "viewportHeight", height);
    let doc = VectorDoc {
        width,
        height,
        viewport_width,
        viewport_height,
        alpha: b.float(vector, "alpha", 1.0),
        tint: b.color(vector, "tint"),
        auto_mirrored: b.boolean(vector, "autoMirrored", false),
        nodes: vector.children.iter().filter_map(|c| b.node(c)).collect(),
        animations: if root.name == "animated-vector" { parse_animations(&root, &mut b.diagnostics) } else { Vec::new() },
    };
    let _ = b.src;
    ParseResult { doc: Some(doc), diagnostics: b.diagnostics }
}
