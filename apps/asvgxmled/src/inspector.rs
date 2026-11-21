use std::ops::Range;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_core::text::FontWeight;
use repose_core::{Dp, Modifier, PaddingValues, Sp, View, request_frame, theme};
use repose_material::Icon;
use repose_material::material3::{Card, CardConfig, TextField, TextFieldConfig};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::{Box, Column, Row, Spacer, Text, TextStyle as _, ViewExt};
use vector_model::{ColorSpec, Node, Rgba, VectorDoc};
use vector_model::parse::Diagnostic;

use crate::edit;
use crate::icons::Symbols;
use crate::session::Session;

struct Entry {
    index: usize,
    depth: usize,
    label: String,
    span: Range<usize>,
    kind: &'static str,
}

fn flatten(nodes: &[Node], depth: usize, next: &mut usize, out: &mut Vec<Entry>) {
    for node in nodes {
        let index = *next;
        *next += 1;
        match node {
            Node::Path(p) => out.push(Entry {
                index,
                depth,
                label: p.name.clone().unwrap_or_else(|| "path".into()),
                span: p.span.clone(),
                kind: "path",
            }),
            Node::Clip(c) => out.push(Entry {
                index,
                depth,
                label: c.name.clone().unwrap_or_else(|| "clip-path".into()),
                span: c.span.clone(),
                kind: "clip",
            }),
            Node::Group(g) => {
                out.push(Entry {
                    index,
                    depth,
                    label: g.name.clone().unwrap_or_else(|| "group".into()),
                    span: g.span.clone(),
                    kind: "group",
                });
                flatten(&g.children, depth + 1, next, out);
            }
        }
    }
}

fn find_node<'a>(nodes: &'a [Node], target: usize, next: &mut usize) -> Option<&'a Node> {
    for node in nodes {
        let index = *next;
        *next += 1;
        if index == target {
            return Some(node);
        }
        if let Node::Group(g) = node {
            if let Some(found) = find_node(&g.children, target, next) {
                return Some(found);
            }
        }
    }
    None
}

fn card(title: &str, content: View) -> View {
    Card(CardConfig::default(), || {
        Column(Modifier::new().fill_max_width().padding_values(PaddingValues {
            left: Dp(12.0), right: Dp(12.0), top: Dp(10.0), bottom: Dp(12.0),
        }).gap(Dp(6.0))).child((
            Text(title.to_owned()).size(Sp(11.0)).font_weight(FontWeight(700)).color(theme().primary),
            content,
        ))
    })
}

pub fn inspector_pane(session: &Rc<Session>) -> View {
    let (result, _) = session.render();
    let Some(doc) = result.doc.as_ref() else {
        return ScrollArea(Modifier::new().fill_max_size(), remember_scroll_state("inspector-empty"),
            Text("Invalid XML — fix the source to see the outline.").size(Sp(12.0)));
    };
    let mut entries = Vec::new();
    flatten(&doc.nodes, 0, &mut 0, &mut entries);
    let selected = session.selected.get();

    let outline: Vec<View> = entries.iter().map(|e| {
        let index = e.index;
        let label = e.label.clone();
        let span = e.span.clone();
        let active = selected == Some(index);
        let indent = e.depth * 12;
        let s = session.clone();
        let glyph = match e.kind {
            "group" => Symbols::FOLDER,
            "clip" => Symbols::VISIBILITY,
            _ => Symbols::CODE,
        };
        Box(Modifier::new()
            .fill_max_width()
            .height(Dp(26.0))
            .padding_values(PaddingValues { left: Dp(indent as f32), right: Dp(4.0), top: Dp(2.0), bottom: Dp(2.0) })
            .background(if active { theme().secondary_container } else { Color::TRANSPARENT })
            .clip_rounded(Dp(6.0))
            .on_pointer_down(move |_| {
                s.selected.set(Some(index));
                let mut t = s.text.borrow_mut();
                t.selection = span.start..span.start;
                request_frame();
            }))
        .child(Row(Modifier::new().fill_max_height().gap(Dp(6.0))).child((
            Icon(glyph).size(Sp(13.0)).color(theme().on_surface_variant),
            Text(label).size(Sp(11.0)),
        )))
    }).collect();

    let properties = selected
        .and_then(|at| find_node(&doc.nodes, at, &mut 0))
        .map(|node| properties_view(session, doc, node))
        .unwrap_or_else(|| Text("Select a layer to inspect its paint.").size(Sp(11.0)).color(theme().on_surface_variant));

    let colors = color_inventory(doc);
    let color_views: Vec<View> = colors.iter().map(|(label, c)| {
        let swatch = match c {
            ColorSpec::Literal(v) => Color::from_rgba(v.r, v.g, v.b, 255),
            ColorSpec::Reference(_) => theme().primary,
        };
        Row(Modifier::new().fill_max_width().gap(Dp(8.0))).child((
            Box(Modifier::new().size(Dp(16.0), Dp(16.0)).background(swatch).clip_rounded(Dp(4.0))),
            Text(label.clone()).size(Sp(10.0)),
            Spacer(),
            Text(match c {
                ColorSpec::Literal(v) => v.to_argb_hex(),
                ColorSpec::Reference(r) => r.clone(),
            }).size(Sp(10.0)).color(theme().on_surface_variant),
        ))
    }).collect();

    let issues: Vec<View> = result.diagnostics.iter().map(|d| issue_view(d, session)).collect();
    let issue_count = format!("Issues ({})", result.diagnostics.len());

    ScrollArea(Modifier::new().fill_max_size(), remember_scroll_state("inspector"),
        Column(Modifier::new().fill_max_width().padding(Dp(10.0)).gap(Dp(10.0))).child((
            card("Outline", Column(Modifier::new().fill_max_width().gap(Dp(1.0))).with_children(outline)),
            card("Properties", properties),
            card("Colors", Column(Modifier::new().fill_max_width().gap(Dp(4.0))).with_children(color_views)),
            card(&issue_count, if issues.is_empty() {
                Text("No issues found.").size(Sp(10.0)).color(theme().on_surface_variant)
            } else {
                Column(Modifier::new().fill_max_width().gap(Dp(4.0))).with_children(issues)
            }),
        )))
}

fn issue_view(d: &Diagnostic, session: &Rc<Session>) -> View {
    let (line, col) = vector_model::line_col(&session.source.get(), d.offset);
    let color = if d.severity == vector_model::Severity::Error { theme().error } else { theme().tertiary };
    let glyph = if d.severity == vector_model::Severity::Error { Symbols::ERROR } else { Symbols::WARNING };
    let s = session.clone();
    let offset = d.offset;
    Row(Modifier::new().fill_max_width().gap(Dp(6.0)).on_pointer_down(move |_| {
        s.selected.set(None);
        let mut t = s.text.borrow_mut();
        t.selection = offset..offset;
        request_frame();
    })).child((
        Icon(glyph).size(Sp(13.0)).color(color),
        Text(format!("{line}:{col} {}", d.message)).size(Sp(10.0)),
    ))
}

fn properties_view(session: &Rc<Session>, doc: &VectorDoc, node: &Node) -> View {
    let Node::Path(p) = node else {
        return Text(format!("{} layer", match node { Node::Group(_) => "group", _ => "clip-path" }))
            .size(Sp(11.0)).color(theme().on_surface_variant);
    };
    let attr = |name: &str, value: String| {
        TextField(
            Modifier::new().fill_max_width(),
            value,
            {
                let s = session.clone();
                let span = p.span.clone();
                let name = name.to_owned();
                move |v| {
                    let source = edit::set_attr(&s.source.get(), &span, &name, &v);
                    s.set_source(source);
                }
            },
            TextFieldConfig {
                label: Some(name.to_owned()),
                single_line: true,
                ..Default::default()
            },
        )
    };
    Column(Modifier::new().fill_max_width().gap(Dp(6.0))).child((
        attr("fillColor", paint_hex(&p.fill, doc)),
        attr("strokeColor", paint_hex(&p.stroke, doc)),
        attr("strokeWidth", format!("{}", p.stroke_width)),
        attr("fillAlpha", format!("{}", p.fill_alpha)),
        attr("strokeAlpha", format!("{}", p.stroke_alpha)),
        Text(format!("fillType: {}", if p.fill_type == vector_model::path::FillType::EvenOdd { "evenOdd" } else { "nonZero" }))
            .size(Sp(10.0)).color(theme().on_surface_variant),
    ))
}

fn paint_hex(paint: &Option<vector_model::Paint>, doc: &VectorDoc) -> String {
    let spec = match paint {
        Some(vector_model::Paint::Color(c)) => Some(c.clone()),
        Some(_) => None,
        None => None,
    };
    let c = spec.unwrap_or_else(|| doc.tint.clone().unwrap_or(ColorSpec::Literal(Rgba::TRANSPARENT)));
    match c {
        ColorSpec::Literal(v) => v.to_argb_hex(),
        ColorSpec::Reference(r) => r,
    }
}

fn color_inventory(doc: &VectorDoc) -> Vec<(String, ColorSpec)> {
    let mut out = Vec::new();
    if let Some(t) = &doc.tint {
        out.push(("tint".into(), t.clone()));
    }
    fn walk(nodes: &[Node], out: &mut Vec<(String, ColorSpec)>) {
        for n in nodes {
            match n {
                Node::Path(p) => {
                    if let Some(vector_model::Paint::Color(c)) = &p.fill { out.push(("fillColor".into(), c.clone())); }
                    if let Some(vector_model::Paint::Color(c)) = &p.stroke { out.push(("strokeColor".into(), c.clone())); }
                    for paint in [&p.fill, &p.stroke].into_iter().flatten() {
                        for stop in paint.stops() {
                            out.push(("gradient stop".into(), stop.color.clone()));
                        }
                    }
                }
                Node::Group(g) => walk(&g.children, out),
                Node::Clip(_) => {}
            }
        }
    }
    walk(&doc.nodes, &mut out);
    out
}

pub fn docs_pane() -> View {
    const DOCS: &str = "Usage\nPaste VectorDrawable XML or open a .xml file. The preview updates as you type. Background, zoom, RTL mirror, theme color and tint change the preview only — the source XML is untouched.\n\nInput, output, limits\nInput is VectorDrawable XML or a .xml file. Output is a live vector preview plus SVG/PNG export. animated-vector previews only its inner static <vector>. @color and ?attr resolve to the theme color.\n\nVectorDrawable syntax\n<vector> viewportWidth/Height, width/height (dp), alpha, tint, autoMirrored.\n<path> pathData, fillColor, fillAlpha, fillType (evenOdd/nonZero), strokeColor, strokeWidth, strokeLineCap/Join.\n<group> rotation, pivotX/Y, scaleX/Y, translateX/Y applied to children.\n<clip-path> clips drawing in the same group.\nColors: #RGB / #ARGB / #RRGGBB / #AARRGGBB — Android puts alpha first.\nAPI 24+ linear and radial gradients via aapt:attr. Sweep is approximated.\n\nFAQ\nDoes AnimatedVectorDrawable play? No — only the static <vector> is drawn.\nWhy is the color wrong? Android fillColor is #AARRGGBB, not SVG #RRGGBBAA.\nIs XML uploaded? No. Everything stays on this device.";

    ScrollArea(Modifier::new().fill_max_size(), remember_scroll_state("docs"),
        Column(Modifier::new().fill_max_width().padding(Dp(14.0)).gap(Dp(10.0))).child(
            DOCS.split("\n\n").map(|block| {
                let mut lines = block.lines();
                let title = lines.next().unwrap_or("");
                let body = lines.collect::<Vec<_>>().join("\n");
                Column(Modifier::new().fill_max_width().gap(Dp(4.0))).child((
                    Text(title.to_owned()).size(Sp(13.0)).font_weight(FontWeight(700)).color(theme().primary),
                    Text(body).size(Sp(11.0)).color(theme().on_surface_variant),
                ))
            }).collect::<Vec<View>>()
        ))
}
