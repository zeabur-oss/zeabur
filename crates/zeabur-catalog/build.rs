//! Embeds Zeabur Catalog's `bundle.tar.gz` from `ZEABUR_CATALOG_RELEASE`, which the Nix dev shell
//! sets to the catalog's `release` output at the commit in flake.lock. The components
//! (`components/<capability>/<id>.yaml`), the `error-pages` template and the template measurements
//! (`measurements/templates/<id>.json`) become sorted tables of `include_str!` entries, so the binary
//! carries the catalogue it was built with and reads nothing at run time.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::{env, process};

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
    let (components, measurements) = match unpack(&archive, &out.join("bundle")) {
        Ok(tables) => tables,
        Err(error) => {
            println!("cargo::error=cannot read {}: {error}", archive.display());
            process::exit(1);
        }
    };
    let mut code = String::from("pub(crate) static FILES: &[(&str, &str, &str)] = &[\n");
    for (capability, id, path) in &components {
        let _ignored = writeln!(
            code,
            "    ({capability:?}, {id:?}, include_str!({:?})),",
            path.display().to_string()
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
    if let Err(error) = fs::write(out.join("bundle.rs"), code) {
        println!("cargo::error=cannot write bundle.rs: {error}");
    }
}

type Components = Vec<(String, String, PathBuf)>;
type Measurements = Vec<(String, PathBuf)>;

/// Writes the files Zeabur reads from the archive to `dir` and returns the components and the
/// template measurements, sorted.
fn unpack(archive: &Path, dir: &Path) -> io::Result<(Components, Measurements)> {
    let _ignored = fs::remove_dir_all(dir);
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?));
    let (mut components, mut measurements) = (Vec::new(), Vec::new());
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let parts: Vec<&str> = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect();
        let dest = dir.join(parts.join("/"));
        match parts.as_slice() {
            ["components", capability, file] => match file.strip_suffix(".yaml") {
                Some(id) => components.push(((*capability).to_owned(), id.to_owned(), dest.clone())),
                None => continue,
            },
            ["measurements", "templates", file] => match file.strip_suffix(".json") {
                Some(id) => measurements.push((id.to_owned(), dest.clone())),
                None => continue,
            },
            ["templates", "error-pages", "manifests.yaml"] => {}
            _ => continue,
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut File::create(dest)?)?;
    }
    components.sort();
    measurements.sort();
    Ok((components, measurements))
}
