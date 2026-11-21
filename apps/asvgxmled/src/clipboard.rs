pub fn write(text: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = clipawl::blocking::write(text);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let value = text.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(clipboard) = clipawl::Clipboard::new() {
                let _ = clipboard.write(&value).await;
            }
        });
    }
}
