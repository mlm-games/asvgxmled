use std::rc::Rc;
use std::time::Instant;

use repose_core::{Dp, Modifier, Sp, Theme, View, WidthClass, remember_auto, request_frame, theme, window_size_class};
use repose_material::{Icon, Symbol};
use repose_material::material3::{
    DropdownMenu, DropdownMenuConfig, DropdownMenuEntry, DropdownMenuItem, IconButton,
    IconButtonConfig, MaterialTheme, MenuState, NavItem, NavigationBar, NavigationBarConfig,
    Scaffold, ScaffoldConfig, TopAppBar, TopAppBarConfig, show_simple_snackbar,
};
use repose_platform::RenderContext;
use repose_ui::overlay::SnackbarController;
use repose_ui::{Box, Column, Row, Text, TextStyle as _, ViewExt};

use crate::icons::Symbols;
use crate::inspector::{docs_pane, inspector_pane};
use crate::panels::{editor_pane, preview_pane};
use crate::session::{ANIMATED_SAMPLE, Pane, SAMPLE, Session};
use crate::{edit, io};

pub fn app(_s: &mut repose_core::Scheduler, _rc: &RenderContext) -> View {
    let session = remember_auto("asvgxmled/session", Session::new);
    tick_animation(&session);
    let snackbar = remember_auto("snackbar", SnackbarController::ambient);
    let status = session.status.get();
    if !status.is_empty() {
        show_simple_snackbar(&snackbar, status.clone(), None, None, 2600);
        session.status.set(String::new());
    }
    drain_files(&session);
    let settings = session.settings.get();
    let dark = settings.app_dark;
    let pane = session.pane.get();

    MaterialTheme(if dark { Theme::dark() } else { Theme::light() }, || {
        let top = top_bar(&session);
        let nav = NavigationBar(selected_pane_index(pane), nav_items(&session), NavigationBarConfig::default());
        let show_nav = matches!(window_size_class().width, WidthClass::Compact);
        let body = body(&session, pane);

        Scaffold(
            move |_| {
                if show_nav {
                    Column(Modifier::new().fill_max_size()).child((
                        Box(Modifier::new().fill_max_size().weight(1.0)).child(body.clone()),
                        nav.clone(),
                    ))
                } else {
                    body.clone()
                }
            },
            ScaffoldConfig {
                modifier: Modifier::new().fill_max_size(),
                top_bar: Some(top),
                ..Default::default()
            },
        )
    })
}

fn tick_animation(session: &Rc<Session>) {
    if !session.playing.get() {
        session.last_tick.set(None);
        return;
    }
    let now = Instant::now();
    let dt = session.last_tick.get().map(|t| now.duration_since(t).as_millis() as u64).unwrap_or(0);
    session.last_tick.set(Some(now));
    let total = session.render().0.doc.as_ref().map(vector_model::total_duration).unwrap_or(0);
    if total > 0 {
        let next = (session.playhead_ms.get() + dt) % total;
        session.playhead_ms.set(next);
        request_frame();
    } else {
        session.playing.set(false);
    }
}

fn export_doc(session: &Rc<Session>) -> Option<vector_model::VectorDoc> {
    let (result, _) = session.render();
    let mut doc = result.doc?;
    if !doc.animations.is_empty() {
        doc = vector_model::apply_animations(&doc, session.playhead_ms.get());
    }
    let settings = session.settings.get();
    if settings.monochrome {
        doc.tint = Some(vector_model::ColorSpec::Literal(settings.monochrome_color));
    }
    Some(doc)
}

fn body(session: &Rc<Session>, pane: Pane) -> View {
    match window_size_class().width {
        WidthClass::Compact => match pane {
            Pane::Editor => editor_pane(session),
            Pane::Preview => preview_pane(session, &session.settings.get()),
            Pane::Inspector => inspector_pane(session),
            Pane::Docs => docs_pane(),
        },
        WidthClass::Medium => match pane {
            Pane::Inspector | Pane::Docs => {
                if pane == Pane::Docs { docs_pane() } else { inspector_pane(session) }
            }
            _ => Row(Modifier::new().fill_max_size()).child((
                Box(Modifier::new().width(Dp(440.0)).fill_max_height()).child(editor_pane(session)),
                Box(Modifier::new().fill_max_size()).child(preview_pane(session, &session.settings.get())),
            )),
        },
        WidthClass::Expanded => Row(Modifier::new().fill_max_size()).child((
            Box(Modifier::new().width(Dp(460.0)).fill_max_height()).child(editor_pane(session)),
            Box(Modifier::new().fill_max_size()).child(preview_pane(session, &session.settings.get())),
            Box(Modifier::new().width(Dp(320.0)).fill_max_height()).child(inspector_pane(session)),
        )),
    }
}

fn selected_pane_index(pane: Pane) -> usize {
    match pane {
        Pane::Editor => 0,
        Pane::Preview => 1,
        Pane::Inspector => 2,
        Pane::Docs => 3,
    }
}

fn nav_items(session: &Rc<Session>) -> Vec<NavItem> {
    let mk = |icon: Symbol, label: &'static str, pane: Pane, s: Rc<Session>| NavItem {
        icon: Icon(icon).size(Sp(20.0)),
        label: label.into(),
        on_click: Rc::new(move || s.pane.set(pane)),
        enabled: true,
        interaction_source: None,
    };
    vec![
        mk(Symbols::CODE, "Source", Pane::Editor, session.clone()),
        mk(Symbols::VISIBILITY, "Preview", Pane::Preview, session.clone()),
        mk(Symbols::INFO, "Inspect", Pane::Inspector, session.clone()),
        mk(Symbols::MENU_BOOK, "Docs", Pane::Docs, session.clone()),
    ]
}

fn menu_item(text: &'static str, action: Rc<dyn Fn()>) -> DropdownMenuEntry {
    DropdownMenuEntry::Item(DropdownMenuItem {
        text: text.into(),
        leading_icon: None,
        trailing_icon: None,
        on_click: action,
        enabled: true,
    })
}

fn top_bar(session: &Rc<Session>) -> View {
    let file_menu = remember_auto("file_menu", MenuState::new);
    let export_menu = remember_auto("export_menu", MenuState::new);
    let th = theme();

    let file_items = vec![
        menu_item("Sample", {
            let s = session.clone();
            Rc::new(move || {
                s.set_source(SAMPLE.to_owned());
                s.status.set("Sample loaded".into());
            })
        }),
        menu_item("Animated sample", {
            let s = session.clone();
            Rc::new(move || {
                s.set_source(ANIMATED_SAMPLE.to_owned());
                s.playhead_ms.set(0);
                s.playing.set(true);
                s.status.set("AnimatedVector sample loaded".into());
            })
        }),
        menu_item("Open XML…", {
            let s = session.clone();
            Rc::new(move || io::queue_open(s.files.clone()))
        }),
        menu_item("Save XML…", {
            let s = session.clone();
            Rc::new(move || io::queue_save_xml(s.files.clone(), "vector".into(), s.source.get()))
        }),
        DropdownMenuEntry::Divider,
        menu_item("Clear", {
            let s = session.clone();
            Rc::new(move || s.set_source(String::new()))
        }),
    ];

    let export_items = vec![
        menu_item("Copy SVG", {
            let s = session.clone();
            Rc::new(move || {
                let text = export_doc(&s).map(|d| io::svg_text(&d, s.settings.get().theme_color));
                match text {
                    Some(svg) => {
                        let _ = crate::clipboard::write(&svg);
                        s.status.set("SVG copied".into());
                    }
                    None => s.status.set("Fix XML before export".into()),
                }
            })
        }),
        menu_item("Copy XML", {
            let s = session.clone();
            Rc::new(move || {
                let _ = crate::clipboard::write(&s.source.get());
                s.status.set("XML copied".into());
            })
        }),
        DropdownMenuEntry::Divider,
        menu_item("Save SVG…", {
            let s = session.clone();
            Rc::new(move || {
                let svg = export_doc(&s).map(|d| io::svg_text(&d, s.settings.get().theme_color));
                match svg {
                    Some(svg) => io::queue_save_svg(s.files.clone(), "vector".into(), svg),
                    None => s.status.set("Fix XML before export".into()),
                }
            })
        }),
        menu_item("Save PNG 4×…", {
            let s = session.clone();
            Rc::new(move || {
                let Some(doc) = export_doc(&s) else {
                    s.status.set("Fix XML before export".into());
                    return;
                };
                let opts = s.options(&s.settings.get());
                let ops = vector_render::build(&doc, &opts, vector_render::IDENTITY);
                let bg = if s.settings.get().app_dark { vector_model::Rgba { r: 24, g: 24, b: 28, a: 255 } } else { vector_model::Rgba::WHITE };
                match io::png_bytes(&doc, &opts, &ops, 4.0, bg) {
                    Ok(bytes) => io::queue_save_png(s.files.clone(), "vector".into(), bytes),
                    Err(e) => s.status.set(format!("PNG failed: {e}")),
                }
            })
        }),
    ];

    let file_trigger = IconButton(
        Icon(Symbols::MENU).size(Sp(20.0)).color(th.on_surface),
        { let m = file_menu.clone(); move || m.open() },
        IconButtonConfig::default(),
    );
    let export_trigger = IconButton(
        Icon(Symbols::DOWNLOAD).size(Sp(20.0)).color(th.on_surface),
        { let m = export_menu.clone(); move || m.open() },
        IconButtonConfig::default(),
    );

    let mut actions: Vec<View> = vec![
        transform_button(Symbols::ROTATE_LEFT, "Rotate −90°", session, -90.0),
        transform_button(Symbols::ROTATE_RIGHT, "Rotate +90°", session, 90.0),
        transform_button(Symbols::FLIP, "Flip horizontal", session, -1.0),
        DropdownMenu(export_menu, Modifier::new(), export_trigger, export_items, DropdownMenuConfig {
            min_width: Dp(220.0), ..Default::default()
        }),
        IconButton(
            Icon(if th.is_dark() { Symbols::LIGHT_MODE } else { Symbols::DARK_MODE }).size(Sp(20.0)),
            {
                let s = session.clone();
                move || s.settings.set(s.settings.get().with(|v| v.app_dark = !v.app_dark))
            },
            IconButtonConfig::default(),
        ),
        IconButton(
            Icon(Symbols::MENU_BOOK).size(Sp(20.0)),
            { let s = session.clone(); move || s.pane.set(Pane::Docs) },
            IconButtonConfig::default(),
        ),
    ];
    actions.reverse();

    TopAppBar(
        Row(Modifier::new().gap(Dp(8.0))).child((
            Icon(Symbols::PALETTE).size(Sp(20.0)).color(th.primary),
            Text("asvgxmled").size(Sp(16.0)),
        )),
        None,
        Some(DropdownMenu(file_menu, Modifier::new(), file_trigger, file_items, DropdownMenuConfig {
            min_width: Dp(200.0), ..Default::default()
        })),
        actions,
        TopAppBarConfig::default(),
    )
}

fn transform_button(glyph: Symbol, label: &'static str, session: &Rc<Session>, value: f64) -> View {
    let _ = label;
    IconButton(
        Icon(glyph).size(Sp(19.0)),
        {
            let s = session.clone();
            move || {
                let (result, _) = s.render();
                let Some(doc) = result.doc else { return };
                let cx = doc.viewport_width / 2.0;
                let cy = doc.viewport_height / 2.0;
                let attrs = if value == -1.0 {
                    format!(" android:scaleX=\"-1\" android:pivotX=\"{cx}\"")
                } else if value == 90.0 || value == -90.0 {
                    format!(" android:rotation=\"{value}\" android:pivotX=\"{cx}\" android:pivotY=\"{cy}\"")
                } else {
                    format!(" android:rotation=\"180\" android:pivotX=\"{cx}\" android:pivotY=\"{cy}\"")
                };
                let next = edit::add_root_wrapper(&s.source.get(), &attrs);
                s.set_source(next);
                s.status.set("Transformed preview wrapper added".into());
            }
        },
        IconButtonConfig::default(),
    )
}

fn drain_files(session: &Rc<Session>) {
    let events: Vec<io::FileEvent> = {
        let mut q = session.files.lock().unwrap();
        q.drain(..).collect()
    };
    for event in events {
        match event {
            io::FileEvent::Loaded { name, text } => {
                session.set_source(text);
                session.status.set(format!("Opened {name}"));
            }
            io::FileEvent::Saved(message) | io::FileEvent::Failed(message) => session.status.set(message),
        }
    }
}
