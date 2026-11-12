//! Builds the Tauri context (config, icons, assets) here and not with `tauri::generate_context!`,
//! so cargo builds the app again when an icon changes. On Windows this also builds the `.exe`
//! resource with `icons/icon.ico` again.

fn main() {
    let attributes = tauri_build::Attributes::new().codegen(tauri_build::CodegenContext::new());
    if let Err(error) = tauri_build::try_build(attributes) {
        println!("cargo::error=tauri-build failed: {error:#}");
    }
}
