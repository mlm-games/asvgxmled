fn main() -> anyhow::Result<()> {
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        asvgxmled::desktop_main()
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = asvgxmled::wasm_start();
        Ok(())
    }

    #[cfg(target_os = "android")]
    {
        Ok(())
    }
}
