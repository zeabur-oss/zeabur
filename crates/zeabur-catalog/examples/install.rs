//! Prints the Helm commands that install a catalogue component, the text Cluster → Components
//! shows. `scripts/dev-cluster.sh` runs them:
//!
//! ```sh
//! cargo run -q -p zeabur-catalog --example install -- databases/cloudnative-pg single-node | bash
//! ```

use std::error::Error;
use std::io::Write as _;

use zeabur_catalog::install::{InstallFormat, guide};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [component, profile] = args.as_slice() else {
        return Err("usage: install <capability>/<id> <profile>".into());
    };
    let text = guide(component, profile)?
        .formats
        .into_iter()
        .find(|f| f.format == InstallFormat::Helm)
        .map_or_default(|f| f.text);
    std::io::stdout().write_all(text.as_bytes())?;
    Ok(())
}
