use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;
use std::sync::{Arc, Mutex};

use repose_core::signal;
use repose_core::signal::Signal;
use repose_ui::TextFieldState;
use vector_model::{ParseResult, Rgba};
use vector_render::RenderOp;

use crate::io::FileEvent;
use crate::syntax::Palette;

pub const SAMPLE: &str = r##"<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="48dp"
    android:height="48dp"
    android:viewportWidth="24"
    android:viewportHeight="24">
    <!-- After the official battery-charging sample -->
    <group android:rotation="0" android:pivotX="12" android:pivotY="12">
        <clip-path android:pathData="M0,0H24V24H0Z" />
        <path
            android:pathData="M12,2L21,7V17L12,22L3,17V7Z"
            android:fillColor="#FF6750A4"
            android:strokeColor="#FF311B62"
            android:strokeWidth="0.6"
            android:strokeLineJoin="round" />
        <path
            android:pathData="M8,12L11,15L16,9"
            android:strokeColor="#FFFFFFFF"
            android:strokeWidth="2"
            android:strokeLineCap="round"
            android:strokeLineJoin="round" />
    </group>
</vector>
"##;

pub const ANIMATED_SAMPLE: &str = r##"<animated-vector
    xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:aapt="http://schemas.android.com/aapt">
    <aapt:attr name="android:drawable">
        <vector
            android:width="48dp"
            android:height="48dp"
            android:viewportWidth="24"
            android:viewportHeight="24">
            <group android:name="rotationGroup" android:pivotX="12" android:pivotY="12">
                <path
                    android:name="vect"
                    android:pathData="M12,2L21,7V17L12,22L3,17V7Z"
                    android:fillColor="#FF6750A4"
                    android:strokeColor="#FF311B62"
                    android:strokeWidth="0.6" />
            </group>
        </vector>
    </aapt:attr>
    <target android:name="rotationGroup">
        <aapt:attr name="android:animation">
            <objectAnimator
                android:propertyName="rotation"
                android:valueFrom="0"
                android:valueTo="360"
                android:duration="2400"
                android:repeatCount="-1" />
        </aapt:attr>
    </target>
    <target android:name="vect">
        <aapt:attr name="android:animation">
            <objectAnimator
                android:propertyName="pathData"
                android:valueType="pathType"
                android:valueFrom="M12,2L21,7V17L12,22L3,17V7Z"
                android:valueTo="M12,3L20,8V16L12,21L4,16V8Z"
                android:duration="1200"
                android:repeatCount="-1"
                android:repeatMode="reverse" />
        </aapt:attr>
    </target>
</animated-vector>
"##;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    Light,
    Dark,
    Checker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Editor,
    Preview,
    Inspector,
    Docs,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub app_dark: bool,
    pub background: Background,
    pub theme_color: Rgba,
    pub tint: bool,
    pub tint_color: Rgba,
    pub monochrome: bool,
    pub monochrome_color: Rgba,
    pub mirrored: bool,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub word_wrap: bool,
    pub font_size: f32,
    pub show_grid: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            app_dark: true,
            background: Background::Dark,
            theme_color: Rgba::WHITE,
            tint: false,
            tint_color: Rgba { r: 103, g: 80, b: 164, a: 255 },
            monochrome: false,
            monochrome_color: Rgba::BLACK,
            mirrored: false,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            word_wrap: false,
            font_size: 12.0,
            show_grid: false,
        }
    }
}

impl Settings {
    pub fn palette(&self) -> Palette {
        if self.app_dark { Palette::Dark } else { Palette::Light }
    }

    pub fn with(&self, f: impl FnOnce(&mut Self)) -> Self {
        let mut next = self.clone();
        f(&mut next);
        next
    }
}

pub struct Session {
    pub source: Signal<String>,
    pub settings: Signal<Settings>,
    pub pane: Signal<Pane>,
    pub selected: Signal<Option<usize>>,
    pub playhead_ms: Signal<u64>,
    pub playing: Signal<bool>,
    pub status: Signal<String>,
    pub last_tick: Cell<Option<Instant>>,
    pub text: Rc<RefCell<TextFieldState>>,
    pub files: Arc<Mutex<Vec<FileEvent>>>,
    cache: RefCell<Option<Cache>>,
}

#[derive(Clone, PartialEq)]
struct RenderKey {
    source: String,
    theme_color: Rgba,
    tint: bool,
    tint_color: Rgba,
    monochrome: bool,
    monochrome_color: Rgba,
    mirrored: bool,
    playhead_ms: u64,
}

struct Cache {
    key: RenderKey,
    result: ParseResult,
    ops: Vec<RenderOp>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            source: signal(SAMPLE.to_owned()),
            settings: signal(Settings::default()),
            pane: signal(Pane::Editor),
            selected: signal(None),
            playhead_ms: signal(0),
            playing: signal(false),
            status: signal(String::new()),
            last_tick: Cell::new(None),
            text: Rc::new(RefCell::new(TextFieldState::with_text(SAMPLE))),
            files: Arc::new(Mutex::new(Vec::new())),
            cache: RefCell::new(None),
        }
    }

    pub fn render(&self) -> (ParseResult, Vec<RenderOp>) {
        let source = self.source.get();
        let settings = self.settings.get();
        let key = RenderKey {
            source: source.clone(),
            theme_color: settings.theme_color,
            tint: settings.tint,
            tint_color: settings.tint_color,
            monochrome: settings.monochrome,
            monochrome_color: settings.monochrome_color,
            mirrored: settings.mirrored,
            playhead_ms: self.playhead_ms.get(),
        };
        if let Some(c) = self.cache.borrow().as_ref() {
            if c.key == key {
                return (c.result.clone(), c.ops.clone());
            }
        }
        let result = vector_model::parse(&source);
        let ops = match &result.doc {
            Some(doc) => {
                let animated = vector_model::apply_animations(doc, key.playhead_ms);
                vector_render::build(&animated, &self.options(&settings), vector_render::IDENTITY)
            }
            None => Vec::new(),
        };
        *self.cache.borrow_mut() = Some(Cache { key, result: result.clone(), ops: ops.clone() });
        (result, ops)
    }

    pub fn options(&self, settings: &Settings) -> vector_render::RenderOptions {
        vector_render::RenderOptions {
            theme_color: settings.theme_color,
            tint: if settings.monochrome {
                Some(vector_model::ColorSpec::Literal(settings.monochrome_color))
            } else if settings.tint {
                Some(vector_model::ColorSpec::Literal(settings.tint_color))
            } else {
                None
            },
            mirrored: settings.mirrored,
        }
    }

    pub fn set_source(&self, text: String) {
        self.text.borrow_mut().apply_controlled_value(&text);
        self.selected.set(None);
        self.source.set(text);
    }
}
