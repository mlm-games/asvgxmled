use std::cell::Cell;
use std::rc::Rc;

use repose_canvas::Canvas;
use repose_core::input::{PointerButton, PointerEvent, PointerEventKind};
use repose_core::prelude::*;
use repose_core::text::FontWeight;
use repose_core::{
    Dp, Modifier, Sp, TextStyle, TextFieldLineLimits, Vec2, View, request_frame, theme,
};
use repose_material::{Icon, Symbol};
use repose_material::material3::{
    Button, ButtonConfig, IconButton, IconButtonConfig, SegmentedButton,
    SegmentConfig, SegmentedButtonConfig, Slider, SliderConfig, Switch, SwitchConfig,
};
use repose_ui::{BasicTextField, Box, Column, Row, Spacer, Text, TextStyle as _, ViewExt};
use vector_model::Rgba;

use crate::icons::Symbols;
use crate::io;
use crate::session::{Background, SAMPLE, Session, Settings};
use crate::syntax::XmlHighlighter;

fn icon_button(glyph: Symbol, on_click: impl Fn() + 'static) -> View {
    IconButton(
        Icon(glyph).size(Sp(19.0)),
        on_click,
        IconButtonConfig {
            container_size: Some(Dp(34.0)),
            ..Default::default()
        },
    )
}

fn section_title(label: &str) -> View {
    Text(label.to_owned())
        .size(Sp(11.0))
        .font_weight(FontWeight(700))
        .color(theme().primary)
}

fn surface(modifier: Modifier, content: View) -> View {
    Box(modifier.background(theme().surface_container_low).border(Dp(1.0), theme().outline_variant.with_alpha_f32(0.4), Dp(10.0)).clip_rounded(Dp(10.0)))
        .child(content)
}

pub fn editor_pane(session: &Rc<Session>) -> View {
    let source = session.source.clone();
    let text = session.text.clone();
    let settings = session.settings.get();
    if text.borrow().text != source.get() {
        text.borrow_mut().apply_controlled_value(&source.get());
    }
    let src = source.get();
    let info = cursor_info(&text.borrow(), &src);

    Column(Modifier::new().fill_max_size().padding(Dp(12.0)).gap(Dp(8.0)))
        .child((
            Row(Modifier::new().fill_max_width().gap(Dp(4.0)).align_items(AlignItems::CENTER)).with_children(vec![
                icon_button(Symbols::FILE_OPEN, {
                    let s = session.clone();
                    move || io::queue_open(s.files.clone())
                }),
                icon_button(Symbols::REFRESH, {
                    let s = session.clone();
                    move || s.set_source(SAMPLE.to_owned())
                }),
                icon_button(Symbols::DELETE, {
                    let s = session.clone();
                    move || s.set_source(String::new())
                }),
                icon_button(Symbols::CONTENT_COPY, {
                    let s = session.clone();
                    move || copy_text(&s.source.get())
                }),
                Box(Modifier::new().width(Dp(8.0))),
                Button(Modifier::new().height(Dp(32.0)), {
                    let s = session.clone();
                    move || {
                        let (out, _, _) = vector_model::format::optimize(&s.source.get());
                        s.set_source(out);
                        s.status.set("Optimized XML".into());
                    }
                }, ButtonConfig::default(), || Text("Optimize").size(Sp(11.0))),
                Button(Modifier::new().height(Dp(32.0)), {
                    let s = session.clone();
                    move || {
                        let out = vector_model::format::prettify(&s.source.get());
                        s.set_source(out);
                        s.status.set("Formatted XML".into());
                    }
                }, ButtonConfig::default(), || Text("Prettify").size(Sp(11.0))),
                Switch(settings.word_wrap, {
                    let s = session.clone();
                    move |on| s.settings.set(s.settings.get().with(|v| v.word_wrap = on))
                }, SwitchConfig::default()),
                Text("Wrap").size(Sp(10.0)),
            ]),
            surface(
                Modifier::new().fill_max_size().padding(Dp(6.0)),
                Row(Modifier::new().fill_max_size()).child((
                    gutter(&text, settings.font_size, settings.word_wrap),
                    BasicTextField(
                        text.clone(),
                        Modifier::new().fill_max_size().padding(Dp(4.0)),
                        "Paste VectorDrawable XML…",
                        repose_ui::TextFieldConfig {
                            text_style: TextStyle {
                                font_size: Sp(settings.font_size),
                                font_family: Some("monospace"),
                                line_height: Sp(settings.font_size * 1.55),
                                ..Default::default()
                            },
                            line_limits: TextFieldLineLimits::MultiLine {
                                min_height_in_lines: 4,
                                max_height_in_lines: usize::MAX,
                            },
                            visual_transformation: Some(Rc::new(XmlHighlighter::new(settings.palette()))),
                            on_change: Some(Rc::new(move |value: String| source.set(value))),
                            ..Default::default()
                        },
                    ),
                )),
            ),
            Row(Modifier::new().fill_max_width().gap(Dp(14.0))).child((
                Text(format!("Lines: {}", info.lines)).size(Sp(10.0)).color(theme().on_surface_variant),
                Text(format!("Columns: {}", info.column)).size(Sp(10.0)).color(theme().on_surface_variant),
                Text(format!("Selection: {} chars", info.selected)).size(Sp(10.0)).color(theme().on_surface_variant),
                Spacer(),
                Text(format!("Font {}", settings.font_size as i32)).size(Sp(10.0)).color(theme().on_surface_variant),
                icon_button(Symbols::ZOOM_OUT, {
                    let s = session.clone();
                    move || s.settings.set(s.settings.get().with(|v| v.font_size = (v.font_size - 1.0).max(8.0)))
                }),
                icon_button(Symbols::ZOOM_IN, {
                    let s = session.clone();
                    move || s.settings.set(s.settings.get().with(|v| v.font_size = (v.font_size + 1.0).min(24.0)))
                }),
            )),
        ))
}

struct CursorInfo {
    lines: usize,
    column: usize,
    selected: usize,
}

fn cursor_info(state: &repose_ui::TextFieldState, src: &str) -> CursorInfo {
    let sel = state.selection.clone();
    let caret = sel.end.min(src.len());
    let before = &src[..caret];
    let line_start = before.rfind('\n').map(|n| n + 1).unwrap_or(0);
    CursorInfo {
        lines: src.split('\n').count(),
        column: src[line_start..caret].chars().count() + 1,
        selected: src.get(sel).map_or(0, |s| s.chars().count()),
    }
}

fn gutter(state: &Rc<std::cell::RefCell<repose_ui::TextFieldState>>, font_size: f32, wrap: bool) -> View {
    let s = state.clone();
    Canvas(
        Modifier::new().width(Dp(38.0)).fill_max_height(),
        move |scope: &mut repose_canvas::DrawScope| {
            let field = s.borrow();
            let font_px = Sp(font_size).to_px().0;
            let lh = Sp(font_size * 1.55).to_px().0;
            let pad = Dp(4.0).to_px().0;
            let baseline = (lh - font_px) * 0.5;
            let offset = field.scroll_offset_y;
            scope.draw_rect(
                Rect { x: 0.0, y: 0.0, w: scope.size.width, h: scope.size.height },
                theme().surface_container,
                Px(0.0),
            );
            let char_w = font_px * 0.6;
            let cols = if wrap {
                ((field.inner_width.max(0.0) - pad * 2.0) / char_w).floor().max(1.0) as usize
            } else {
                usize::MAX
            };
            let mut display_row = 0usize;
            for (i, line) in field.text.split('\n').enumerate() {
                let number = (i + 1).to_string();
                let y = display_row as f32 * lh + pad + baseline - offset;
                if y >= -lh && y <= scope.size.height {
                    scope.draw_text_with_family(
                        number.clone(),
                        Vec2 {
                            x: scope.size.width - Dp(6.0).to_px().0 - number.len() as f32 * char_w * 0.9,
                            y,
                        },
                        theme().on_surface_variant,
                        Px(font_px * 0.9),
                        Some("monospace"),
                    );
                }
                let width = line.chars().count().max(1);
                display_row += if cols == usize::MAX { 1 } else { width.div_ceil(cols) };
            }
        },
    )
}

fn copy_text(text: &str) {
    crate::clipboard::write(text);
}

pub fn preview_pane(session: &Rc<Session>, settings: &Settings) -> View {
    let drag = Rc::new(Cell::new(None::<(f32, f32)>));
    let backgrounds = [
        SegmentConfig { label: "Light".into(), icon: None, on_click: Rc::new({
            let s = session.clone();
            move || s.settings.set(s.settings.get().with(|v| v.background = Background::Light))
        }), enabled: true, interaction_source: None },
        SegmentConfig { label: "Dark".into(), icon: None, on_click: Rc::new({
            let s = session.clone();
            move || s.settings.set(s.settings.get().with(|v| v.background = Background::Dark))
        }), enabled: true, interaction_source: None },
        SegmentConfig { label: "Check".into(), icon: None, on_click: Rc::new({
            let s = session.clone();
            move || s.settings.set(s.settings.get().with(|v| v.background = Background::Checker))
        }), enabled: true, interaction_source: None },
    ];
    let bg_index = match settings.background {
        Background::Light => 0,
        Background::Dark => 1,
        Background::Checker => 2,
    };

    let total_ms = session.render().0.doc.as_ref().map(vector_model::total_duration).unwrap_or(0);

    Column(Modifier::new().fill_max_size().padding(Dp(12.0)).gap(Dp(8.0)))
        .child((
            Row(Modifier::new().fill_max_width().gap(Dp(8.0))).child((
                section_title("Preview"),
                Spacer(),
                SegmentedButton(&[bg_index], backgrounds.to_vec(), SegmentedButtonConfig::default()),
            )),
            surface(
                Modifier::new().fill_max_size().padding(Dp(4.0)),
                preview_canvas(session, drag),
            ),
            preview_controls(session, settings),
            animation_bar(session, total_ms),
        ))
}

fn animation_bar(session: &Rc<Session>, total_ms: u64) -> View {
    if total_ms == 0 {
        return Spacer();
    }
    let playing = session.playing.get();
    let at = session.playhead_ms.get();
    Row(Modifier::new().fill_max_width().gap(Dp(8.0)).align_items(AlignItems::CENTER)).with_children(vec![
        section_title("AnimatedVectorDrawable"),
        icon_button(if playing { Symbols::PAUSE } else { Symbols::PLAY_ARROW }, {
            let s = session.clone();
            move || s.playing.set(!s.playing.get())
        }),
        icon_button(Symbols::REPLAY, {
            let s = session.clone();
            move || {
                s.playhead_ms.set(0);
                s.last_tick.set(None);
            }
        }),
        Slider(at as f32, (0.0, total_ms as f32), None, {
            let s = session.clone();
            move |v| s.playhead_ms.set(v as u64)
        }, SliderConfig::default()),
        Text(format!("{:.2}s / {:.2}s", at as f32 / 1000.0, total_ms as f32 / 1000.0)).size(Sp(11.0)),
    ])
}

fn color_swatches(colors: &[Rgba], selected: Rgba, apply: Rc<dyn Fn(Rgba)>) -> View {
    let views: Vec<View> = colors
        .iter()
        .copied()
        .map(|c| {
            let a = apply.clone();
            Box(Modifier::new()
                .size(Dp(22.0), Dp(22.0))
                .background(Color::from_rgba(c.r, c.g, c.b, 255))
                .border(Dp(if c == selected { 2.0 } else { 1.0 }), theme().on_surface, Dp(11.0))
                .clip_rounded(Dp(11.0))
                .on_pointer_down(move |_| {
                    a(c.clone());
                    request_frame();
                }))
        })
        .collect();
    Row(Modifier::new().gap(Dp(4.0)).align_items(AlignItems::CENTER)).with_children(views)
}

fn preview_controls(session: &Rc<Session>, settings: &Settings) -> View {
    let theme_colors = [
        Rgba::BLACK,
        Rgba::WHITE,
        Rgba { r: 103, g: 80, b: 164, a: 255 },
        Rgba { r: 25, g: 118, b: 210, a: 255 },
        Rgba { r: 198, g: 40, b: 40, a: 255 },
    ];
    let mono_colors = [
        Rgba::BLACK,
        Rgba::WHITE,
        Rgba { r: 103, g: 80, b: 164, a: 255 },
        Rgba { r: 66, g: 66, b: 66, a: 255 },
    ];
    let theme_swatches = color_swatches(&theme_colors, settings.theme_color, Rc::new({
        let s = session.clone();
        move |c| s.settings.set(s.settings.get().with(|v| v.theme_color = c))
    }));
    let mono_swatches = color_swatches(&mono_colors, settings.monochrome_color, Rc::new({
        let s = session.clone();
        move |c| s.settings.set(s.settings.get().with(|v| v.monochrome_color = c))
    }));

    Column(Modifier::new().fill_max_width().gap(Dp(6.0))).child((
        Row(Modifier::new().fill_max_width().gap(Dp(8.0)).align_items(AlignItems::CENTER)).child((
            icon_button(Symbols::ZOOM_OUT, {
                let s = session.clone();
                move || s.settings.set(s.settings.get().with(|v| v.zoom = (v.zoom * 0.8).max(0.1)))
            }),
            Slider(settings.zoom, (0.1, 8.0), Some(0.05), {
                let s = session.clone();
                move |v| s.settings.set(s.settings.get().with(|x| x.zoom = v))
            }, SliderConfig::default()),
            icon_button(Symbols::ZOOM_IN, {
                let s = session.clone();
                move || s.settings.set(s.settings.get().with(|v| v.zoom = (v.zoom * 1.25).min(8.0)))
            }),
            Button(Modifier::new().height(Dp(30.0)), {
                let s = session.clone();
                move || s.settings.set(s.settings.get().with(|v| {
                    v.zoom = 1.0;
                    v.pan_x = 0.0;
                    v.pan_y = 0.0;
                }))
            }, ButtonConfig::default(), || Text("Fit").size(Sp(11.0))),
            Text(format!("{}%", (settings.zoom * 100.0) as i32)).size(Sp(11.0)),
        )),
        Row(Modifier::new().fill_max_width().gap(Dp(8.0)).align_items(AlignItems::CENTER)).with_children(vec![
            section_title("Theme color"),
            theme_swatches,
            Spacer(),
            Switch(settings.mirrored, {
                let s = session.clone();
                move |on| s.settings.set(s.settings.get().with(|v| v.mirrored = on))
            }, SwitchConfig::default()),
            Text("RTL mirror").size(Sp(11.0)),
            Switch(settings.tint, {
                let s = session.clone();
                move |on| s.settings.set(s.settings.get().with(|v| v.tint = on))
            }, SwitchConfig::default()),
            Text("Tint").size(Sp(11.0)),
            Switch(settings.show_grid, {
                let s = session.clone();
                move |on| s.settings.set(s.settings.get().with(|v| v.show_grid = on))
            }, SwitchConfig::default()),
            Text("Grid").size(Sp(11.0)),
        ]),
        Row(Modifier::new().fill_max_width().gap(Dp(8.0)).align_items(AlignItems::CENTER)).with_children(vec![
            Switch(settings.monochrome, {
                let s = session.clone();
                move |on| s.settings.set(s.settings.get().with(|v| v.monochrome = on))
            }, SwitchConfig::default()),
            Text("Monochrome").size(Sp(11.0)),
            mono_swatches,
            Spacer(),
            Text("Flattens all fills and strokes to one color").size(Sp(10.0)).color(theme().on_surface_variant),
        ]),
    ))
}

fn preview_canvas(session: &Rc<Session>, drag: Rc<Cell<Option<(f32, f32)>>>) -> View {
    let draw = session.clone();
    let mut modi = Modifier::new()
        .fill_max_size()
        .clip_rounded(Dp(8.0))
        .on_scroll({
            let s = session.clone();
            move |delta: Vec2| {
                let factor = (1.0 - delta.y * 0.002).clamp(0.5, 2.0);
                s.settings.set(s.settings.get().with(|v| v.zoom = (v.zoom * factor).clamp(0.1, 8.0)));
                request_frame();
                Vec2::ZERO
            }
        });
    modi = modi.on_pointer_down({
        let d = drag.clone();
        move |pe: PointerEvent| {
            if matches!(pe.event, PointerEventKind::Down(PointerButton::Primary)) {
                d.set(Some((pe.position.x, pe.position.y)));
                pe.consume();
            }
        }
    });
    modi = modi.on_pointer_move({
        let s = session.clone();
        let d = drag.clone();
        move |pe: PointerEvent| {
            if let Some((px, py)) = d.get() {
                let dx = pe.position.x - px;
                let dy = pe.position.y - py;
                d.set(Some((pe.position.x, pe.position.y)));
                s.settings.set(s.settings.get().with(|v| {
                    v.pan_x += dx;
                    v.pan_y += dy;
                }));
                request_frame();
                pe.consume();
            }
        }
    });
    modi = modi.on_pointer_up(move |_| drag.set(None));

    Canvas(modi, move |scope: &mut repose_canvas::DrawScope| {
        let s = &draw;
        let (result, ops) = s.render();
        let Some(doc) = result.doc.as_ref() else { return };
        let settings = s.settings.get();
        let w = scope.size.width;
        let h = scope.size.height;
        let base = match settings.background {
            Background::Light => Color::WHITE,
            Background::Dark => Color::from_hex("#18181C"),
            Background::Checker => Color::from_hex("#F5F5F5"),
        };
        scope.draw_rect(Rect { x: 0.0, y: 0.0, w, h }, base, Px(0.0));
        if settings.background == Background::Checker {
            let cell = 18.0_f32;
            let mut row = 0;
            let mut y = 0.0;
            while y < h {
                let mut col = 0;
                let mut x = 0.0;
                while x < w {
                    if (row + col) % 2 == 0 {
                        scope.draw_rect(
                            Rect { x, y, w: cell.min(w - x), h: cell.min(h - y) },
                            Color::from_hex("#E3E3E8"),
                            Px(0.0),
                        );
                    }
                    x += cell;
                    col += 1;
                }
                y += cell;
                row += 1;
            }
        }
        let pad = 64.0;
        let fit = ((w - pad) / doc.width.max(1.0) as f32).min((h - pad) / doc.height.max(1.0) as f32).max(0.01);
        let scale = fit * settings.zoom;
        let tx = (w - doc.width as f32 * scale) * 0.5 + settings.pan_x;
        let ty = (h - doc.height as f32 * scale) * 0.5 + settings.pan_y;
        if settings.show_grid {
            let step = doc.viewport_width.max(1.0) as f32 * scale / 8.0;
            if step > 3.0 {
                let mut x = tx;
                while x < tx + doc.width as f32 * scale {
                    scope.draw_rect(Rect { x, y: ty, w: 1.0, h: doc.height as f32 * scale }, Color::from_hex("#405080").with_alpha_f32(0.35), Px(0.0));
                    x += step;
                }
                let mut y = ty;
                while y < ty + doc.height as f32 * scale {
                    scope.draw_rect(Rect { x: tx, y, w: doc.width as f32 * scale, h: 1.0 }, Color::from_hex("#405080").with_alpha_f32(0.35), Px(0.0));
                    y += step;
                }
            }
        }
        let transformed = vector_render::with_outer(&ops, [scale, 0.0, 0.0, scale, tx, ty]);
        vector_render::paint(&transformed, scope);
    })
}
