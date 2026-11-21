use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::color::{ColorSpec, Rgba};
use crate::model::{GradientUnits, Node, Paint, VectorDoc};

#[derive(Debug)]
enum Token {
    Decl(String),
    Comment(String),
    Start(String, Vec<(String, String)>),
    Empty(String, Vec<(String, String)>),
    End(String),
    Text(String),
}

fn raw(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn tokenize(src: &str) -> (Vec<Token>, Vec<String>) {
    let mut reader = Reader::from_reader(src.as_bytes());
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    loop {
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(e) => {
                errors.push(e.to_string());
                break;
            }
        };
        let is_empty = matches!(event, Event::Empty(_));
        match event {
            Event::Decl(e) => tokens.push(Token::Decl(String::from_utf8_lossy(e.as_ref()).into_owned())),
            Event::Comment(e) => tokens.push(Token::Comment(String::from_utf8_lossy(e.as_ref()).into_owned())),
            Event::Start(e) | Event::Empty(e) => {
                let empty = is_empty;
                let mut attrs = Vec::new();
                for attr in e.attributes().flatten() {
                    let key = raw(attr.key.as_ref());
                    let value = attr.normalized_value(XmlVersion::Implicit1_0).map(|v| v.into_owned()).unwrap_or_default();
                    attrs.push((key, value));
                }
                if empty {
                    tokens.push(Token::Empty(raw(e.name().as_ref()), attrs));
                } else {
                    tokens.push(Token::Start(raw(e.name().as_ref()), attrs));
                }
            }
            Event::End(e) => tokens.push(Token::End(raw(e.name().as_ref()))),
            Event::Text(e) => {
                let text = String::from_utf8_lossy(e.as_ref()).into_owned();
                if !text.trim().is_empty() {
                    tokens.push(Token::Text(text.trim().to_owned()));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    (tokens, errors)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn write(tokens: &[Token], out: &mut String, indent: bool) {
    let mut depth = 0usize;
    let mut first = true;
    for token in tokens {
        if !first && indent {
            out.push('\n');
        }
        first = false;
        match token {
            Token::Decl(s) => out.push_str(&format!("<?{s}?>")),
            Token::Comment(s) => out.push_str(&format!("<!--{s}-->")),
            Token::Start(name, attrs) => {
                if indent {
                    out.push_str(&"  ".repeat(depth));
                }
                out.push('<');
                out.push_str(name);
                write_attrs(attrs, out);
                out.push('>');
                depth += 1;
            }
            Token::Empty(name, attrs) => {
                if indent {
                    out.push_str(&"  ".repeat(depth));
                }
                out.push('<');
                out.push_str(name);
                write_attrs(attrs, out);
                out.push_str("/>");
            }
            Token::End(name) => {
                depth = depth.saturating_sub(1);
                if indent {
                    out.push_str(&"  ".repeat(depth));
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
            Token::Text(s) => {
                if indent {
                    out.push_str(&"  ".repeat(depth));
                }
                out.push_str(&escape(s));
            }
        }
    }
}

fn write_attrs(attrs: &[(String, String)], out: &mut String) {
    for (k, v) in attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&escape(v));
        out.push('"');
    }
}

pub fn prettify(src: &str) -> String {
    let (tokens, _) = tokenize(src);
    let mut out = String::new();
    write(&tokens, &mut out, true);
    out.push('\n');
    out
}

fn is_default(name: &str, value: &str) -> bool {
    let v = value.trim();
    matches!(
        (name, v),
        ("alpha", "1") | ("alpha", "1.0")
            | ("fillAlpha", "1") | ("fillAlpha", "1.0")
            | ("strokeAlpha", "1") | ("strokeAlpha", "1.0")
            | ("strokeWidth", "0") | ("strokeWidth", "0.0")
            | ("trimPathStart", "0") | ("trimPathStart", "0.0")
            | ("trimPathEnd", "1") | ("trimPathEnd", "1.0")
            | ("trimPathOffset", "0") | ("trimPathOffset", "0.0")
            | ("fillType", "nonZero")
            | ("strokeLineCap", "butt")
            | ("strokeLineJoin", "miter")
            | ("strokeMiterLimit", "4") | ("strokeMiterLimit", "4.0")
            | ("rotation", "0") | ("rotation", "0.0")
            | ("pivotX", "0") | ("pivotX", "0.0")
            | ("pivotY", "0") | ("pivotY", "0.0")
            | ("scaleX", "1") | ("scaleX", "1.0")
            | ("scaleY", "1") | ("scaleY", "1.0")
            | ("translateX", "0") | ("translateX", "0.0")
            | ("translateY", "0") | ("translateY", "0.0")
    )
}

fn optimize_value(name: &str, value: &str) -> String {
    if name.ends_with("Color") || name == "color" || name == "tint" {
        if let Some(c) = Rgba::parse(value) {
            return c.to_argb_hex();
        }
    }
    let suffix = if value.ends_with("dp") { "dp" } else { "" };
    if let Ok(n) = value.trim_end_matches("dp").trim().parse::<f64>() {
        let number = if n.fract() == 0.0 {
            (n as i64).to_string()
        } else {
            let s = format!("{n:.3}").trim_end_matches('0').trim_end_matches('.').to_owned();
            if s == "-0" { "0".into() } else { s }
        };
        return format!("{number}{suffix}");
    }
    value.to_owned()
}

pub fn optimize(src: &str) -> (String, usize, usize) {
    let before = src.len();
    let (tokens, _) = tokenize(src);
    let cleaned: Vec<Token> = tokens
        .into_iter()
        .filter(|t| !matches!(t, Token::Comment(_)))
        .map(|t| match t {
            Token::Start(n, attrs) => Token::Start(n, clean_attrs(attrs)),
            Token::Empty(n, attrs) => Token::Empty(n, clean_attrs(attrs)),
            other => other,
        })
        .collect();
    let mut out = String::new();
    write(&cleaned, &mut out, false);
    let after = out.len();
    (out, before, after)
}

fn clean_attrs(attrs: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    attrs
        .into_iter()
        .filter(|(k, v)| {
            let name = k.rsplit(':').next().unwrap_or(k);
            !is_default(name, v)
        })
        .map(|(k, v)| {
            let name = k.rsplit(':').next().unwrap_or(&k).to_owned();
            (k, optimize_value(&name, &v))
        })
        .filter(|(k, _)| seen.insert(k.clone()))
        .collect()
}

fn resolve_color(spec: &ColorSpec, tint: &Option<ColorSpec>, theme: Rgba) -> Rgba {
    let source_alpha = match spec {
        ColorSpec::Literal(c) => c.a,
        ColorSpec::Reference(_) => 255,
    };
    let rgb = match tint.clone().unwrap_or_else(|| spec.clone()) {
        ColorSpec::Literal(c) => c,
        ColorSpec::Reference(_) => theme,
    };
    Rgba { r: rgb.r, g: rgb.g, b: rgb.b, a: source_alpha }
}

fn color_hex(spec: &ColorSpec, tint: &Option<ColorSpec>, theme: Rgba) -> String {
    let c = resolve_color(spec, tint, theme);
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn color_alpha(spec: &ColorSpec, tint: &Option<ColorSpec>, theme: Rgba) -> f32 {
    resolve_color(spec, tint, theme).a as f32 / 255.0
}

fn gradient_defs(doc: &VectorDoc, tint: &Option<ColorSpec>, theme: Rgba, out: &mut String) -> Vec<String> {
    let mut ids = Vec::new();
    let mut n = 0;
    fn walk(nodes: &[Node], out: &mut String, ids: &mut Vec<String>, n: &mut usize, tint: &Option<ColorSpec>, theme: Rgba) {
        for node in nodes {
            match node {
                Node::Group(g) => walk(&g.children, out, ids, n, tint, theme),
                Node::Path(p) => {
                    for paint in [&p.fill, &p.stroke].into_iter().flatten() {
                        let (tag, attrs, units, stops) = match paint {
                            Paint::Color(_) => continue,
                            Paint::Linear(g) => ("linearGradient", format!("x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"", g.start.0, g.start.1, g.end.0, g.end.1), &g.units, &g.stops),
                            Paint::Radial(g) => ("radialGradient", format!("cx=\"{}\" cy=\"{}\" r=\"{}\"", g.center.0, g.center.1, g.radius), &g.units, &g.stops),
                            Paint::Sweep(g) => ("linearGradient", format!("x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"", g.center.0, g.center.1, g.center.0 + 1.0, g.center.1), &g.units, &g.stops),
                        };
                        let id = format!("grad{n}");
                        *n += 1;
                        let unit_str = if *units == GradientUnits::ObjectBoundingBox { "objectBoundingBox" } else { "userSpaceOnUse" };
                        out.push_str(&format!("<{tag} id=\"{id}\" gradientUnits=\"{unit_str}\" {attrs}>"));
                        for stop in stops {
                            out.push_str(&format!(
                                "<stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\"/>",
                                stop.offset,
                                color_hex(&stop.color, tint, theme),
                                color_alpha(&stop.color, tint, theme)
                            ));
                        }
                        out.push_str(&format!("</{tag}>"));
                        ids.push(id);
                    }
                }
                Node::Clip(_) => {}
            }
        }
    }
    walk(&doc.nodes, out, &mut ids, &mut n, tint, theme);
    ids
}

pub fn to_svg(doc: &VectorDoc, theme: Rgba) -> String {
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        doc.width, doc.height, doc.viewport_width, doc.viewport_height
    );
    out.push_str("<defs>");
    let grads = gradient_defs(doc, &doc.tint, theme, &mut out);
    out.push_str("</defs>");
    let mut gi = 0;
    emit_nodes(&doc.nodes, doc, theme, &mut gi, &grads, &mut out, 1.0);
    out.push_str("</svg>");
    out
}

fn emit_nodes(nodes: &[Node], doc: &VectorDoc, theme: Rgba, gi: &mut usize, grads: &[String], out: &mut String, alpha: f32) {
    for node in nodes {
        match node {
            Node::Group(g) => {
                let mut t = format!(
                    "translate({} {}) rotate({}) scale({} {}) translate({} {})",
                    g.translate_x + g.pivot_x,
                    g.translate_y + g.pivot_y,
                    g.rotation,
                    g.scale_x,
                    g.scale_y,
                    -g.pivot_x,
                    -g.pivot_y
                );
                if doc.auto_mirrored {
                    t = format!("translate({} 0) scale(-1 1) {t}", doc.viewport_width);
                }
                out.push_str(&format!("<g transform=\"{t}\" opacity=\"{}\">", g.alpha * alpha));
                emit_nodes(&g.children, doc, theme, gi, grads, out, g.alpha * alpha);
                out.push_str("</g>");
            }
            Node::Clip(_) => {}
            Node::Path(p) => {
                let fill = match &p.fill {
                    None => ("none".to_owned(), 1.0),
                    Some(Paint::Color(c)) => (color_hex(c, &doc.tint, theme), color_alpha(c, &doc.tint, theme) * p.fill_alpha),
                    Some(_) => {
                        let id = grads.get(*gi).cloned().unwrap_or_default();
                        *gi += 1;
                        (format!("url(#{id})"), p.fill_alpha)
                    }
                };
                let stroke = match &p.stroke {
                    None => ("none".to_owned(), 1.0),
                    Some(Paint::Color(c)) => (color_hex(c, &doc.tint, theme), color_alpha(c, &doc.tint, theme) * p.stroke_alpha),
                    Some(_) => {
                        let id = grads.get(*gi).cloned().unwrap_or_default();
                        *gi += 1;
                        (format!("url(#{id})"), p.stroke_alpha)
                    }
                };
                let rule = if p.fill_type == crate::path::FillType::EvenOdd { "evenodd" } else { "nonzero" };
                let cap = match p.stroke_cap { crate::model::LineCap::Butt => "butt", crate::model::LineCap::Round => "round", crate::model::LineCap::Square => "square" };
                let join = match p.stroke_join { crate::model::LineJoin::Miter => "miter", crate::model::LineJoin::Round => "round", crate::model::LineJoin::Bevel => "bevel" };
                out.push_str(&format!(
                    "<path d=\"{}\" fill=\"{}\" fill-opacity=\"{}\" fill-rule=\"{}\" stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\" stroke-miterlimit=\"{}\" opacity=\"{}\"/>",
                    p.path_data, fill.0, fill.1, rule, stroke.0, stroke.1, p.stroke_width, cap, join, p.stroke_miter, p.alpha * alpha
                ));
            }
        }
    }
}
