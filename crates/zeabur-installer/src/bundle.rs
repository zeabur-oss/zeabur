//! `bundle`: keeps what `apply` downloads in a directory, to carry to a host that cannot reach the
//! sources. `bundle: <directory>` in the configuration makes `plan` and `apply` read it.

use std::fs;
use std::path::Path;

use zeabur_installer_host::{bundle, download};
use zeabur_installer_plan::plan::{Download, ReleasePlan, Source};

use crate::apply::{self, STATE_DIR};
use crate::{InstallError, load, out};

pub(crate) fn run(config_path: &Path, dir: &Path) -> Result<(), InstallError> {
    // Recording is chosen before the configuration is read, which would open the bundle instead.
    bundle::record(dir)?;
    let config = load::config(config_path)?;
    let index = load::catalog(config.sources.catalog.as_deref(), STATE_DIR)?;
    let plan = apply::make_plan(&index, &config)?;

    let work = dir.join("work");
    let agent = download::agent();
    let mut left_out = Vec::new();

    let mut files: Vec<&Download> = plan.cluster.downloads().collect();
    files.extend(&plan.calico);
    if let Some(identity) = &plan.identity
        && let Source::Pinned(d) = &identity.identityctl
    {
        files.push(d);
    }
    if let Some(zeabur) = &plan.zeabur
        && let Source::Pinned(d) = &zeabur.chart
    {
        files.push(d);
    }
    for d in files {
        out::err(&format!("    {}", d.url));
        download::fetch(&agent, &d.url, &work.join(&d.file), d.sha256.as_deref())?;
    }

    for component in &plan.components {
        for release in &component.releases {
            match release {
                ReleasePlan::HelmChart {
                    name,
                    object,
                    digest,
                } => match digest {
                    Some(digest) => {
                        out::err(&format!("    {name}"));
                        apply::checked_chart(object, digest)?;
                    }
                    None => left_out.push(name.clone()),
                },
                ReleasePlan::Urls { urls, digests, .. } => {
                    for (i, url) in urls.iter().enumerate() {
                        out::err(&format!("    {url}"));
                        download::fetch_text(&agent, url, digests.get(i).map(String::as_str))?;
                    }
                }
                ReleasePlan::Manifests { .. } => {}
            }
        }
    }

    bundle::finish()?;
    fs::remove_dir_all(&work)
        .map_err(|e| InstallError::Message(format!("{}: {e}", work.display())))?;
    for item in &left_out {
        out::err(&format!(
            "not kept, it has no digest to check it against: {item}"
        ));
    }
    for url in bundle::recorded_urls() {
        out::err(&format!(
            "kept with the SHA-256 it has now, not checked against a source: {url}"
        ));
    }
    out::line(&format!(
        "Kept in {}. Copy it to the host and set `bundle: <path>` in its configuration.",
        dir.display()
    ));
    Ok(())
}
