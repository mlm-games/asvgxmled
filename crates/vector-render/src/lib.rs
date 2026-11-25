use std::sync::Arc;

use kurbo::{BezPath, ParamCurve, PathSeg, Shape};
use lyon_path::Path as LyonPath;
use lyon_path::math::{Point as LyonPoint, point};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, StrokeOptions,
    StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
};
use repose_core::{PaintDesc, VectorMeshData, VectorVertex};
use vector_model::color::{ColorSpec, Rgba};
use vector_model::model::{GradientUnits, LineCap, LineJoin, Node, Paint, VectorDoc};

pub type Transform = [f32; 6];

pub const IDENTITY: Transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub theme_color: Rgba,
    pub tint: Option<ColorSpec>,
    pub mirrored: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { theme_color: Rgba::BLACK, tint: None, mirrored: false }
    }
}

#[derive(Clone, Debug)]
pub struct RenderedMesh {
    pub mesh: Arc<VectorMeshData>,
    pub transform: Transform,
}

#[derive(Clone, Debug)]
pub enum RenderOp {
    Mesh(RenderedMesh),
    PushClip(Arc<VectorMeshData>, Transform),
    PopClip,
}

#[derive(Clone)]
enum VertexPaint {
    Solid([f32; 4]),
    Linear { start: (f32, f32), end: (f32, f32), stops: Vec<(f32, [f32; 4])> },
    Radial { center: (f32, f32), radius: f32, stops: Vec<(f32, [f32; 4])> },
}

struct VtxCtor {
    paint: VertexPaint,
}

impl VtxCtor {
    fn color_at(&self, p: LyonPoint) -> [f32; 4] {
        match &self.paint {
            VertexPaint::Solid(c) => *c,
            VertexPaint::Linear { start, end, stops } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let len2 = dx * dx + dy * dy;
                let t = if len2 < 1e-6 {
                    0.0
                } else {
                    ((p.x - start.0) * dx + (p.y - start.1) * dy) / len2
                };
                sample_stops(stops, t)
            }
            VertexPaint::Radial { center, radius, stops } => {
                let t = ((p.x - center.0).powi(2) + (p.y - center.1).powi(2)).sqrt() / radius.max(1e-6);
                sample_stops(stops, t)
            }
        }
    }
}

fn sample_stops(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    if stops.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let t = t.clamp(0.0, 1.0);
    let mut i = 0;
    while i + 1 < stops.len() && stops[i + 1].0 < t {
        i += 1;
    }
    if i + 1 >= stops.len() {
        return stops.last().copied().unwrap().1;
    }
    let (o0, c0) = stops[i];
    let (o1, c1) = stops[i + 1];
    let f = if o1 > o0 { ((t - o0) / (o1 - o0)).clamp(0.0, 1.0) } else { 0.0 };
    std::array::from_fn(|k| c0[k] + (c1[k] - c0[k]) * f)
}

impl FillVertexConstructor<VectorVertex> for VtxCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> VectorVertex {
        make_vertex(vertex.position(), self.color_at(vertex.position()))
    }
}

impl StrokeVertexConstructor<VectorVertex> for VtxCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> VectorVertex {
        make_vertex(vertex.position(), self.color_at(vertex.position()))
    }
}

fn make_vertex(p: LyonPoint, color: [f32; 4]) -> VectorVertex {
    VectorVertex { pos: [p.x, p.y], color, uv: [0.0, 0.0] }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_color(rgba: Rgba, alpha: f32) -> [f32; 4] {
    let a = (rgba.a as f32 / 255.0 * alpha).clamp(0.0, 1.0);
    [
        srgb_to_linear(rgba.r as f32 / 255.0) * a,
        srgb_to_linear(rgba.g as f32 / 255.0) * a,
        srgb_to_linear(rgba.b as f32 / 255.0) * a,
        a,
    ]
}

fn resolved_rgba(spec: &ColorSpec, opts: &RenderOptions) -> Rgba {
    let source_alpha = match spec {
        ColorSpec::Literal(c) => c.a,
        ColorSpec::Reference(_) => 255,
    };
    let rgb = match opts.tint.clone().unwrap_or_else(|| spec.clone()) {
        ColorSpec::Literal(c) => c,
        ColorSpec::Reference(_) => opts.theme_color,
    };
    Rgba { r: rgb.r, g: rgb.g, b: rgb.b, a: source_alpha }
}

fn stops_for(paint: &Paint, opts: &RenderOptions, alpha: f32) -> Vec<(f32, [f32; 4])> {
    paint
        .stops()
        .iter()
        .map(|s| (s.offset, linear_color(resolved_rgba(&s.color, opts), alpha)))
        .collect()
}

fn vertex_paint(paint: &Paint, path: &BezPath, opts: &RenderOptions, alpha: f32) -> VertexPaint {
    match paint {
        Paint::Color(c) => VertexPaint::Solid(linear_color(resolved_rgba(c, opts), alpha)),
        Paint::Linear(g) => {
            let bbox = path.bounding_box();
            let (sx, sy) = if g.units == GradientUnits::ObjectBoundingBox {
                (bbox.x0 + g.start.0 * bbox.width(), bbox.y0 + g.start.1 * bbox.height())
            } else {
                g.start
            };
            let (ex, ey) = if g.units == GradientUnits::ObjectBoundingBox {
                (bbox.x0 + g.end.0 * bbox.width(), bbox.y0 + g.end.1 * bbox.height())
            } else {
                g.end
            };
            VertexPaint::Linear {
                start: (sx as f32, sy as f32),
                end: (ex as f32, ey as f32),
                stops: stops_for(paint, opts, alpha),
            }
        }
        Paint::Radial(g) => {
            let bbox = path.bounding_box();
            let (cx, cy) = if g.units == GradientUnits::ObjectBoundingBox {
                (bbox.x0 + g.center.0 * bbox.width(), bbox.y0 + g.center.1 * bbox.height())
            } else {
                g.center
            };
            let r = if g.units == GradientUnits::ObjectBoundingBox {
                g.radius * bbox.width().max(bbox.height())
            } else {
                g.radius
            };
            VertexPaint::Radial {
                center: (cx as f32, cy as f32),
                radius: r as f32,
                stops: stops_for(paint, opts, alpha),
            }
        }
        Paint::Sweep(g) => {
            let bbox = path.bounding_box();
            let (cx, cy) = if g.units == GradientUnits::ObjectBoundingBox {
                (bbox.x0 + g.center.0 * bbox.width(), bbox.y0 + g.center.1 * bbox.height())
            } else {
                g.center
            };
            VertexPaint::Linear {
                start: (cx as f32, cy as f32),
                end: ((cx + bbox.width()) as f32, cy as f32),
                stops: stops_for(paint, opts, alpha),
            }
        }
    }
}

fn segment_length(seg: &PathSeg) -> f64 {
    let mut single = BezPath::new();
    single.move_to(seg.start());
    single.push(seg.as_path_el());
    single.perimeter(0.05)
}

fn append_range(path: &BezPath, out: &mut BezPath, from: f64, to: f64) {
    if to - from < 1e-6 {
        return;
    }
    let total: f64 = path.segments().map(|s| segment_length(&s)).sum();
    if total < 1e-6 {
        return;
    }
    let mut cursor = 0.0;
    let mut started = false;
    for seg in path.segments() {
        let len = segment_length(&seg);
        if len < 1e-6 {
            continue;
        }
        let a = cursor / total;
        let b = (cursor + len) / total;
        cursor += len;
        if b <= from || a >= to {
            continue;
        }
        let lo = ((from - a) / (b - a)).clamp(0.0, 1.0);
        let hi = ((to - a) / (b - a)).clamp(0.0, 1.0);
        if hi - lo < 1e-6 {
            continue;
        }
        let sub = seg.subsegment(lo..hi);
        if !started {
            out.move_to(sub.start());
            started = true;
        }
        out.push(sub.as_path_el());
    }
}

fn trimmed(path: &BezPath, start: f64, end: f64, offset: f64) -> BezPath {
    if start <= 0.0 && end >= 1.0 && offset == 0.0 {
        return path.clone();
    }
    let s = (start + offset).rem_euclid(1.0);
    let e = (end + offset).rem_euclid(1.0);
    let mut out = BezPath::new();
    if s < e {
        append_range(path, &mut out, s, e);
    } else {
        append_range(path, &mut out, s, 1.0);
        append_range(path, &mut out, 0.0, e);
    }
    out
}

fn bez_to_lyon(path: &BezPath) -> LyonPath {
    let mut b = LyonPath::builder();
    let mut started = false;
    for el in path.elements() {
        match *el {
            kurbo::PathEl::MoveTo(p) => {
                if started {
                    b.end(false);
                }
                b.begin(point(p.x as f32, p.y as f32));
                started = true;
            }
            kurbo::PathEl::LineTo(p) => {
                b.line_to(point(p.x as f32, p.y as f32));
            }
            kurbo::PathEl::QuadTo(c, p) => {
                b.quadratic_bezier_to(point(c.x as f32, c.y as f32), point(p.x as f32, p.y as f32));
            }
            kurbo::PathEl::CurveTo(c1, c2, p) => {
                b.cubic_bezier_to(
                    point(c1.x as f32, c1.y as f32),
                    point(c2.x as f32, c2.y as f32),
                    point(p.x as f32, p.y as f32),
                );
            }
            kurbo::PathEl::ClosePath => {
                b.close();
                started = false;
            }
        }
    }
    if started {
        b.end(false);
    }
    b.build()
}

fn tessellate_fill(path: &LyonPath, vp: VertexPaint, even_odd: bool) -> Option<Arc<VectorMeshData>> {
    let mut buffers: VertexBuffers<VectorVertex, u32> = VertexBuffers::new();
    let ctor = VtxCtor { paint: vp };
    let rule = if even_odd { lyon_tessellation::FillRule::EvenOdd } else { lyon_tessellation::FillRule::NonZero };
    let opts = FillOptions::tolerance(0.12).with_fill_rule(rule);
    FillTessellator::new()
        .tessellate_path(path, &opts, &mut BuffersBuilder::new(&mut buffers, ctor))
        .ok()?;
    if buffers.indices.is_empty() {
        return None;
    }
    Some(Arc::new(VectorMeshData { vertices: buffers.vertices.into(), indices: buffers.indices.into() }))
}

fn tessellate_stroke(path: &LyonPath, vp: VertexPaint, width: f64, cap: LineCap, join: LineJoin, miter: f64) -> Option<Arc<VectorMeshData>> {
    if width <= 0.0 {
        return None;
    }
    let mut buffers: VertexBuffers<VectorVertex, u32> = VertexBuffers::new();
    let cap = match cap {
        LineCap::Butt => lyon_tessellation::LineCap::Butt,
        LineCap::Round => lyon_tessellation::LineCap::Round,
        LineCap::Square => lyon_tessellation::LineCap::Square,
    };
    let join = match join {
        LineJoin::Miter => lyon_tessellation::LineJoin::Miter,
        LineJoin::Round => lyon_tessellation::LineJoin::Round,
        LineJoin::Bevel => lyon_tessellation::LineJoin::Bevel,
    };
    let opts = StrokeOptions::tolerance(0.12)
        .with_line_width(width as f32)
        .with_line_cap(cap)
        .with_line_join(join)
        .with_miter_limit(miter.max(1.0) as f32);
    StrokeTessellator::new()
        .tessellate_path(path, &opts, &mut BuffersBuilder::new(&mut buffers, VtxCtor { paint: vp }))
        .ok()?;
    if buffers.indices.is_empty() {
        return None;
    }
    Some(Arc::new(VectorMeshData { vertices: buffers.vertices.into(), indices: buffers.indices.into() }))
}

fn mul(a: Transform, b: Transform) -> Transform {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

fn translate(x: f64, y: f64) -> Transform {
    [1.0, 0.0, 0.0, 1.0, x as f32, y as f32]
}

fn scale(x: f64, y: f64) -> Transform {
    [x as f32, 0.0, 0.0, y as f32, 0.0, 0.0]
}

fn rotate(deg: f64) -> Transform {
    let r = deg.to_radians();
    let (c, s) = (r.cos() as f32, r.sin() as f32);
    [c, s, -s, c, 0.0, 0.0]
}

pub fn transform_mesh(mesh: Arc<VectorMeshData>, t: Transform) -> Arc<VectorMeshData> {
    let vertices: Vec<VectorVertex> = mesh
        .vertices
        .iter()
        .map(|v| {
            let x = v.pos[0];
            let y = v.pos[1];
            VectorVertex {
                pos: [t[0] * x + t[2] * y + t[4], t[1] * x + t[3] * y + t[5]],
                ..*v
            }
        })
        .collect();
    Arc::new(VectorMeshData { vertices: vertices.into(), indices: mesh.indices.clone() })
}

fn group_transform(g: &vector_model::model::GroupNode) -> Transform {
    let mut m = translate(g.translate_x, g.translate_y);
    m = mul(m, translate(g.pivot_x, g.pivot_y));
    m = mul(m, rotate(g.rotation));
    m = mul(m, scale(g.scale_x, g.scale_y));
    mul(m, translate(-g.pivot_x, -g.pivot_y))
}

pub fn viewport_transform(doc: &VectorDoc, opts: &RenderOptions) -> Transform {
    let sx = doc.width / doc.viewport_width;
    let sy = doc.height / doc.viewport_height;
    if opts.mirrored {
        [-(sx as f32), 0.0, 0.0, sy as f32, (sx * doc.viewport_width) as f32, 0.0]
    } else {
        [sx as f32, 0.0, 0.0, sy as f32, 0.0, 0.0]
    }
}

pub fn build(doc: &VectorDoc, opts: &RenderOptions, outer: Transform) -> Vec<RenderOp> {
    let mut ops = Vec::new();
    let viewport = mul(outer, viewport_transform(doc, opts));
    let mut parent = IDENTITY;
    let alpha = doc.alpha;
    emit_nodes(&doc.nodes, opts, viewport, &mut parent, alpha, &mut ops);
    ops
}

fn emit_nodes(nodes: &[Node], opts: &RenderOptions, viewport: Transform, parent: &mut Transform, alpha: f32, ops: &mut Vec<RenderOp>) {
    for node in nodes {
        match node {
            Node::Group(g) => {
                let saved = *parent;
                let local = group_transform(g);
                *parent = mul(*parent, local);
                let mut clips = 0;
                emit_nodes(&g.children, opts, viewport, parent, alpha * g.alpha, ops);
                for n in &g.children {
                    if matches!(n, Node::Clip(_)) {
                        clips += 1;
                    }
                }
                for _ in 0..clips {
                    ops.push(RenderOp::PopClip);
                }
                *parent = saved;
            }
            Node::Clip(c) => {
                let lyon = bez_to_lyon(&c.path);
                if let Some(mesh) = tessellate_fill(&lyon, VertexPaint::Solid([1.0, 1.0, 1.0, 1.0]), false) {
                    ops.push(RenderOp::PushClip(mesh, mul(viewport, *parent)));
                }
            }
            Node::Path(p) => {
                let path = trimmed(&p.path, p.trim_start, p.trim_end, p.trim_offset);
                let lyon = bez_to_lyon(&path);
                let alpha = alpha * p.alpha;
                if let Some(fill) = &p.fill {
                    let vp = vertex_paint(fill, &p.path, opts, alpha * p.fill_alpha);
                    if let Some(mesh) = tessellate_fill(&lyon, vp, p.fill_type == vector_model::path::FillType::EvenOdd) {
                        ops.push(RenderOp::Mesh(RenderedMesh { mesh, transform: mul(viewport, *parent) }));
                    }
                }
                if let Some(stroke) = &p.stroke {
                    let vp = vertex_paint(stroke, &p.path, opts, alpha * p.stroke_alpha);
                    if let Some(mesh) = tessellate_stroke(
                        &lyon,
                        vp,
                        p.stroke_width,
                        p.stroke_cap,
                        p.stroke_join,
                        p.stroke_miter,
                    ) {
                        ops.push(RenderOp::Mesh(RenderedMesh { mesh, transform: mul(viewport, *parent) }));
                    }
                }
            }
        }
    }
}

pub fn compose(a: Transform, b: Transform) -> Transform {
    mul(a, b)
}

pub fn with_outer(ops: &[RenderOp], outer: Transform) -> Vec<RenderOp> {
    ops.iter()
        .map(|op| match op {
            RenderOp::Mesh(m) => RenderOp::Mesh(RenderedMesh {
                mesh: m.mesh.clone(),
                transform: mul(outer, m.transform),
            }),
            RenderOp::PushClip(mesh, t) => RenderOp::PushClip(mesh.clone(), mul(outer, *t)),
            RenderOp::PopClip => RenderOp::PopClip,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_path_keeps_only_selected_fraction() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((100.0, 0.0));
        let full = path.perimeter(0.01);
        let half = trimmed(&path, 0.0, 0.5, 0.0).perimeter(0.01);
        assert!((half - full * 0.5).abs() < 1.0, "full={full} half={half}");
    }
}

pub fn paint(ops: &[RenderOp], scope: &mut repose_canvas::DrawScope) {
    for op in ops {
        match op {
            RenderOp::Mesh(m) => scope.draw_vector_mesh(m.mesh.clone(), m.transform, PaintDesc::Solid),
            RenderOp::PushClip(mesh, t) => scope.push_vector_clip(transform_mesh(mesh.clone(), *t)),
            RenderOp::PopClip => scope.pop_vector_clip(),
        }
    }
}
