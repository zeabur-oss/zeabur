// The context (config, icons, assets) is built here and not by `tauri::generate_context!`, so cargo
// builds the app again when an icon changes. On Windows this also builds the `.exe` resource with
// `icons/icon.ico` again.
fn main() {
    let attributes = tauri_build::Attributes::new().codegen(tauri_build::CodegenContext::new());
    tauri_build::try_build(attributes).expect("tauri-build failed");
}
