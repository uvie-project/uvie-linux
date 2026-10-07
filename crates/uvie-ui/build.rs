fn main() {
    // The UI is dark-only (Theme palette in app.slint) — pin the fluent
    // dark widget style so std-widgets don't render light on light
    // desktops. SLINT_STYLE can still override it at runtime.
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent-dark".into()),
    )
    .expect("slint compile failed");
}
