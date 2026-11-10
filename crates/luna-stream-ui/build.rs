fn main() {
    // Enable geometry introspection for headless tests, without release-build metadata.
    let config = slint_build::CompilerConfiguration::new().with_debug_info(cfg!(debug_assertions));
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Falha ao compilar ui/app.slint");
}
