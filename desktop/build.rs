//! Builds the Tauri context (config, icons, assets) here and not with `tauri::generate_context!`,
//! so cargo builds the app again when an icon changes. On Windows this also builds the `.exe`
//! resource with `icons/icon.ico` again. The app manifest makes every command need a permission
//! (`allow-<command>`), so only the windows that `capabilities/` names can call it.

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "snapshot",
        "add_paths",
        "add_pasted",
        "remove_source",
        "toggle_favourite",
        "probe",
        "open_cluster",
        "start_cluster",
        "back_to_chooser",
        "set_menu",
    ]);
    let attributes = tauri_build::Attributes::new()
        .codegen(tauri_build::CodegenContext::new())
        .app_manifest(manifest);
    if let Err(error) = tauri_build::try_build(attributes) {
        println!("cargo::error=tauri-build failed: {error:#}");
    }
}
