//! Writes the `crds/` files `chart_crds` selects: `chart_crds <chart.tgz> <values.json>`.
//! `scripts/check-chart-crds.sh` compares them with what Helm installs.

use std::io::Write as _;

use zeabur_installer_host::archive::chart_crds;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(chart), Some(values)) = (args.next(), args.next()) else {
        return Err("usage: chart_crds <chart.tgz> <values.json>".to_owned());
    };
    let archive = std::fs::read(&chart).map_err(|e| format!("{chart}: {e}"))?;
    let text = std::fs::read(&values).map_err(|e| format!("{values}: {e}"))?;
    let values = serde_json::from_slice(&text).map_err(|e| format!("{values}: {e}"))?;
    let crds = chart_crds(&archive, &values).map_err(|e| e.to_string())?;
    std::io::stdout()
        .write_all(&crds)
        .map_err(|e| e.to_string())
}
