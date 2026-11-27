//! Embeds Zeabur Catalog's `bundle.tar.gz` from `ZEABUR_CATALOG_RELEASE`, which the Nix dev shell
//! sets to the catalog's `release` output at the commit in flake.lock. The components
//! (`components/<id>/component.yaml`), the templates (`templates/<id>/template.yaml` and
//! `manifests.yaml`) and the measurements (`measurements/templates/<id>.json`,
//! `measurements/components/<id>.json`) become
//! sorted tables of `include_str!` entries, so the binary carries the catalogue it was built with.
//! `crosscheck.json`, the catalogue's own render of each template, is unpacked for the tests only.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::{env, io, process};

fn main() {
    println!("cargo::rerun-if-env-changed=ZEABUR_CATALOG_RELEASE");
    println!("cargo::rerun-if-env-changed=ZEABUR_CATALOG_COMMIT");
    let (Some(release), Some(_)) = (
        env::var_os("ZEABUR_CATALOG_RELEASE"),
        env::var_os("ZEABUR_CATALOG_COMMIT"),
    ) else {
        println!(
            "cargo::error=ZEABUR_CATALOG_RELEASE and ZEABUR_CATALOG_COMMIT are not set: build inside `nix develop`"
        );
        return;
    };
    let archive = Path::new(&release).join("bundle.tar.gz");
    println!("cargo::rerun-if-changed={}", archive.display());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_default());
    let (components, templates, measurements, component_measurements) =
        match unpack(&archive, &out.join("bundle")) {
            Ok(tables) => tables,
            Err(error) => {
                println!("cargo::error=cannot read {}: {error}", archive.display());
                process::exit(1);
            }
        };
    let mut code = String::from("pub(crate) static FILES: &[(&str, &str)] = &[\n");
    for (id, path) in &components {
        let _ignored = writeln!(
            code,
            "    ({id:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    code.push_str("];\n");
    code.push_str("pub(crate) static TEMPLATES: &[(&str, &str, &str)] = &[\n");
    for (id, dir) in &templates {
        let file = |name: &str| dir.join(name).display().to_string();
        let _ignored = writeln!(
            code,
            "    ({id:?}, include_str!({:?}), include_str!({:?})),",
            file("template.yaml"),
            file("manifests.yaml")
        );
    }
    code.push_str("];\n");
    code.push_str("pub(crate) static TEMPLATE_MEASUREMENTS: &[(&str, &str)] = &[\n");
    for (id, path) in &measurements {
        let _ignored = writeln!(
            code,
            "    ({id:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    code.push_str("];\n");
    code.push_str("pub(crate) static COMPONENT_MEASUREMENTS: &[(&str, &str)] = &[\n");
    for (id, path) in &component_measurements {
        let _ignored = writeln!(
            code,
            "    ({id:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    code.push_str("];\n");
    if let Err(error) = fs::write(out.join("bundle.rs"), code) {
        println!("cargo::error=cannot write bundle.rs: {error}");
    }
}

type Components = Vec<(String, PathBuf)>;
type Templates = Vec<(String, PathBuf)>;
type Measurements = Vec<(String, PathBuf)>;

/// Writes the files Zeabur reads from the archive to `dir` and returns the components, the
/// template directories, the template measurements and the component measurements, sorted.
fn unpack(
    archive: &Path,
    dir: &Path,
) -> io::Result<(Components, Templates, Measurements, Measurements)> {
    let _ignored = fs::remove_dir_all(dir);
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?));
    let (mut components, mut templates) = (Vec::new(), Vec::new());
    let (mut measurements, mut component_measurements) = (Vec::new(), Vec::new());
    for entry in tar.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let path = entry.path()?.into_owned();
        let parts: Vec<&str> = path.iter().filter_map(|part| part.to_str()).collect();
        let dest = dir.join(parts.join("/"));
        match parts.as_slice() {
            ["components", id, "component.yaml"] => {
                components.push(((*id).to_owned(), dest.clone()));
            }
            ["components", _, file] if file.starts_with("README") => continue,
            // A layout this build does not know would leave the catalogue silently empty.
            ["components", ..] => {
                return Err(io::Error::other(format!(
                    "{} is not components/<id>/component.yaml",
                    path.display()
                )));
            }
            ["measurements", "templates", file] => match file.strip_suffix(".json") {
                Some(id) => measurements.push((id.to_owned(), dest.clone())),
                None => continue,
            },
            ["measurements", "components", file] => match file.strip_suffix(".json") {
                Some(id) => component_measurements.push((id.to_owned(), dest.clone())),
                None => continue,
            },
            ["templates", id, "template.yaml"] => {
                templates.push(((*id).to_owned(), dir.join("templates").join(id)));
            }
            ["templates", _, "manifests.yaml"] | ["crosscheck.json"] => {}
            _ => continue,
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut File::create(dest)?)?;
    }
    components.sort();
    templates.sort();
    measurements.sort();
    component_measurements.sort();
    Ok((components, templates, measurements, component_measurements))
}
