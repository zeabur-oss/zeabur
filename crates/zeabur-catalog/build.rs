//! Embeds the vendored Zeabur Catalog (`bundle/capabilities/<capability>/<id>.yaml`)
//! as a sorted table of `include_str!` entries, so the binary carries the catalogue it was built
//! with and reads nothing at run time.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let root = manifest.join("bundle/capabilities");
    println!("cargo::rerun-if-changed={}", root.display());
    let mut entries = Vec::new();
    for capability in fs::read_dir(&root).into_iter().flatten().flatten() {
        println!("cargo::rerun-if-changed={}", capability.path().display());
        for file in fs::read_dir(capability.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "yaml") {
                let capability = capability.file_name().to_string_lossy().into_owned();
                let id = path
                    .file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                entries.push((capability, id, path));
            }
        }
    }
    entries.sort();
    let mut code = String::from("pub(crate) static FILES: &[(&str, &str, &str)] = &[\n");
    for (capability, id, path) in &entries {
        let _ignored = writeln!(
            code,
            "    ({capability:?}, {id:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    code.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_default()).join("bundle.rs");
    if let Err(error) = fs::write(&out, code) {
        println!("cargo::error=cannot write {}: {error}", out.display());
    }
}
