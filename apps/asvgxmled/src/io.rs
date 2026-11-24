use std::io::Cursor;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use std::thread;

use anyhow::{Context, Result};
use image::ImageFormat;
use repose_core::{BlendMode, ClipOp, Color, PaintDesc, Scene, SceneNode};
use repose_render_wgpu::offscreen::OffscreenRenderer;
use vector_model::{Rgba, VectorDoc};
use vector_render::{RenderOptions, RenderOp};

pub enum FileEvent {
    Loaded { name: String, text: String },
    Saved(String),
    Failed(String),
}

pub type FileQueue = Arc<Mutex<Vec<FileEvent>>>;

fn post(queue: &FileQueue, event: FileEvent) {
    queue.lock().unwrap().push(event);
    repose_core::request_frame();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn queue_open(queue: FileQueue) {
    thread::spawn(move || {
        let Some(path) = rlobkit_dialogs::blocking_open_file("Open VectorDrawable XML", &["xml"]) else { return };
        match std::fs::read_to_string(&path) {
            Ok(text) => post(&queue, FileEvent::Loaded { name: path.display().to_string(), text }),
            Err(e) => post(&queue, FileEvent::Failed(format!("Open failed: {e}"))),
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub fn queue_open(queue: FileQueue) {
    use rlobkit_dialogs::picker::{OpenFileOptions, RlobKit};
    use rlobkit_dialogs::{RlobKitMode, RlobKitType};

    wasm_bindgen_futures::spawn_local(async move {
        let opts = OpenFileOptions {
            file_type: RlobKitType::Custom { extensions: vec!["xml".into()], mime_types: vec![] },
            mode: RlobKitMode::Single,
            title: Some("Open VectorDrawable XML".into()),
            initial_directory: None,
        };
        let result = match RlobKit::open_file_picker(opts).await {
            Ok(Some(files)) => match files.into_iter().next() {
                Some(file) => match file.read_bytes_async().await {
                    Ok(bytes) => String::from_utf8(bytes.to_vec())
                        .map(|text| FileEvent::Loaded { name: file.name().to_owned(), text })
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                },
                None => return,
            },
            Ok(None) => return,
            Err(e) => Err(e.to_string()),
        };
        post(&queue, result.unwrap_or_else(|e| FileEvent::Failed(format!("Open failed: {e}"))));
    });
}

pub fn svg_text(doc: &VectorDoc, theme: Rgba) -> String {
    vector_model::format::to_svg(doc, theme)
}

#[cfg(not(target_arch = "wasm32"))]
fn save_text(queue: FileQueue, name: String, extension: &str, text: String) {
    let extension = extension.to_owned();
    thread::spawn(move || {
        let Some(path) = rlobkit_dialogs::blocking_save_file("Save file", &name, &extension) else { return };
        match std::fs::write(&path, text) {
            Ok(()) => post(&queue, FileEvent::Saved(format!("Saved {}", path.display()))),
            Err(e) => post(&queue, FileEvent::Failed(format!("Save failed: {e}"))),
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn save_text(queue: FileQueue, name: String, extension: &str, text: String) {
    use rlobkit_dialogs::picker::{RlobKit, SaveFileOptions};

    let opts = SaveFileOptions {
        suggested_name: Some(format!("{name}.{extension}")),
        extension: Some(extension.to_owned()),
        title: Some("Save file".into()),
        initial_directory: None,
        file_type: None,
        data: Some(text.into_bytes()),
    };
    wasm_bindgen_futures::spawn_local(async move {
        match RlobKit::save_bytes(opts, &[]).await {
            Ok(Some(_)) => post(&queue, FileEvent::Saved("Download started".into())),
            Ok(None) => {}
            Err(e) => post(&queue, FileEvent::Failed(format!("Save failed: {e}"))),
        }
    });
}

pub fn queue_save_xml(queue: FileQueue, name: String, xml: String) {
    save_text(queue, name, "xml", xml);
}

pub fn queue_save_svg(queue: FileQueue, name: String, svg: String) {
    save_text(queue, name, "svg", svg);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn queue_save_png(queue: FileQueue, name: String, bytes: Vec<u8>) {
    thread::spawn(move || {
        let Some(path) = rlobkit_dialogs::blocking_save_file("Save PNG", &name, "png") else { return };
        match std::fs::write(&path, bytes) {
            Ok(()) => post(&queue, FileEvent::Saved(format!("Saved {}", path.display()))),
            Err(e) => post(&queue, FileEvent::Failed(format!("Save failed: {e}"))),
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub fn queue_save_png(queue: FileQueue, name: String, bytes: Vec<u8>) {
    use rlobkit_dialogs::picker::{RlobKit, SaveFileOptions};

    let opts = SaveFileOptions {
        suggested_name: Some(format!("{name}.png")),
        extension: Some("png".into()),
        title: Some("Save PNG".into()),
        initial_directory: None,
        file_type: None,
        data: Some(bytes),
    };
    wasm_bindgen_futures::spawn_local(async move {
        match RlobKit::save_bytes(opts, &[]).await {
            Ok(Some(_)) => post(&queue, FileEvent::Saved("Download started".into())),
            Ok(None) => {}
            Err(e) => post(&queue, FileEvent::Failed(format!("Save failed: {e}"))),
        }
    });
}

fn build_scene(ops: &[RenderOp], scale: f32, background: Rgba) -> Scene {
    let mut scene = Scene::default();
    scene.clear_color = Color::from_rgba(background.r, background.g, background.b, background.a);
    for op in ops {
        match op {
            RenderOp::Mesh(m) => scene.nodes.push(SceneNode::VectorMesh {
                mesh: m.mesh.clone(),
                transform: vector_render::compose([scale, 0.0, 0.0, scale, 0.0, 0.0], m.transform),
                paint: PaintDesc::Solid,
                clip: None,
                blend: BlendMode::Alpha,
            }),
            RenderOp::PushClip(mesh, transform) => scene.nodes.push(SceneNode::PushVectorClip {
                mesh: vector_render::transform_mesh(mesh.clone(), vector_render::compose([scale, 0.0, 0.0, scale, 0.0, 0.0], *transform)),
                op: ClipOp::Intersect,
            }),
            RenderOp::PopClip => scene.nodes.push(SceneNode::PopVectorClip),
        }
    }
    scene
}

fn encode_png(rgba: Vec<u8>, w: u32, h: u32) -> Result<Vec<u8>> {
    let image = image::RgbaImage::from_raw(w, h, rgba).context("invalid RGBA buffer")?;
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).context("PNG encode failed")?;
    Ok(bytes.into_inner())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn queue_png_render(
    queue: FileQueue,
    name: String,
    doc: &VectorDoc,
    ops: &[RenderOp],
    scale: f32,
    background: Rgba,
) {
    let w = (doc.width as f32 * scale).round().max(1.0) as u32;
    let h = (doc.height as f32 * scale).round().max(1.0) as u32;
    let scene = build_scene(ops, scale, background);
    thread::spawn(move || {
        let result = (|| -> Result<Vec<u8>> {
            let mut renderer = OffscreenRenderer::new_blocking(w, h, 1).context("no GPU adapter for PNG export")?;
            let rgba = renderer.render_rgba(&scene, None).context("offscreen render failed")?;
            encode_png(rgba, w, h)
        })();
        match result {
            Ok(bytes) => queue_save_png(queue, name, bytes),
            Err(e) => post(&queue, FileEvent::Failed(format!("PNG failed: {e}"))),
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub fn queue_png_render(
    queue: FileQueue,
    name: String,
    doc: &VectorDoc,
    ops: &[RenderOp],
    scale: f32,
    background: Rgba,
) {
    let w = (doc.width as f32 * scale).round().max(1.0) as u32;
    let h = (doc.height as f32 * scale).round().max(1.0) as u32;
    let scene = build_scene(ops, scale, background);
    wasm_bindgen_futures::spawn_local(async move {
        let result = async {
            let mut renderer = OffscreenRenderer::new(w, h, 1).await.context("no WebGPU adapter for PNG export")?;
            let rgba = renderer.render_rgba_async(&scene, None).await.context("offscreen render failed")?;
            encode_png(rgba, w, h)
        }
        .await;
        match result {
            Ok(bytes) => queue_save_png(queue, name, bytes),
            Err(e) => post(&queue, FileEvent::Failed(format!("PNG failed: {e}"))),
        }
    });
}
