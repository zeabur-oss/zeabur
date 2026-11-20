//! Prints the chart version the catalogue pins and tests for a component. The development cluster
//! (`cargo xtask dev-cluster`, `xtask/src/dev_cluster/catalog.rs`) builds Zeabur Builder from that
//! release:
//!
//! ```sh
//! cargo run -q -p zeabur-catalog --example tested_version -- zeabur-builder
//! ```

use std::error::Error;
use std::io::Write as _;

use zeabur_catalog::catalogue;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [component] = args.as_slice() else {
        return Err("usage: tested_version <id>".into());
    };
    let version = catalogue()?
        .into_iter()
        .find(|c| &c.id == component)
        .ok_or_else(|| format!("the catalogue has no component {component}"))?
        .tested_version
        .ok_or_else(|| format!("{component} names no chart version"))?;
    writeln!(std::io::stdout(), "{version}")?;
    Ok(())
}
