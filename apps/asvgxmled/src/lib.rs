pub mod clipboard;
pub mod edit;
pub mod icons;
pub mod inspector;
pub mod io;
pub mod panels;
pub mod session;
pub mod syntax;
pub mod ui;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_start() -> Result<(), wasm_bindgen::JsValue> {
    let mut options = repose_platform::web::WebOptions::new(None);
    options.set_prevent_default(true);
    repose_platform::web::run_web_app(ui::app, options)
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn desktop_main() -> anyhow::Result<()> {
    repose_platform::run_desktop_app_with_config(
        ui::app,
        repose_platform::AppConfig {
            window_title: "asvgxmled — VectorDrawable Editor".into(),
            window_size: (1600, 980),
            ..Default::default()
        },
    )
}
