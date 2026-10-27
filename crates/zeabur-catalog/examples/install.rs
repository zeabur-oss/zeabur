//! Prints the Helm commands that install a catalogue component, as `zeaburctl components export
//! --format helm` does, with each `<input>=<value>` given filling that input's placeholder.
//! `scripts/dev-cluster.sh` runs them:
//!
//! ```sh
//! cargo run -q -p zeabur-catalog --example install -- image-updates/keel single-node | bash
//! ```

use std::error::Error;
use std::io::Write as _;

use zeabur_catalog::install::{InstallFormat, guide};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [component, profile, inputs @ ..] = args.as_slice() else {
        return Err("usage: install <capability>/<id> <profile> [<input>=<value>...]".into());
    };
    let mut text = guide(component, profile)?
        .formats
        .into_iter()
        .find(|f| f.format == InstallFormat::Helm)
        .map_or_default(|f| f.text);
    for input in inputs {
        let Some((name, value)) = input.split_once('=') else {
            return Err(format!("{input}: not <input>=<value>").into());
        };
        // A shell argument (`'<name>'`), or a YAML value in a values file (`<name>`).
        let argument = format!("'<{name}>'");
        let yaml = format!("<{name}>");
        if !text.contains(&yaml) {
            return Err(format!("{component} takes no input {name}").into());
        }
        text = text.replace(&argument, &format!("'{}'", value.replace('\'', r"'\''")));
        text = text.replace(&yaml, &serde_json::to_string(value)?);
    }
    std::io::stdout().write_all(text.as_bytes())?;
    Ok(())
}
