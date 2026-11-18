//! `zeaburctl components export`: Zeabur Catalog components' manifests or Helm commands, to
//! install them on a cluster. The text comes from the catalogue in this binary
//! ([`zeabur_catalog::install`]); no cluster is read.

use std::process::ExitCode;

use clap::{Subcommand, ValueEnum};
use zeabur_catalog::install::{InstallError, InstallFormat, PROFILES, guide};

use crate::output;

/// What to do with the catalogue's components.
#[derive(Debug, clap::Args)]
pub(crate) struct ComponentsArgs {
    #[command(subcommand)]
    command: ComponentsCommand,
}

#[derive(Debug, Subcommand)]
enum ComponentsCommand {
    /// Print what installs one or more components, with the catalogue's pinned versions and
    /// values. Apply it with kubectl, or commit it to the repository a GitOps tool watches.
    ///
    /// Values the component asks for show as `<name>` placeholders; replace them first.
    Export(ExportArgs),
}

#[derive(Debug, clap::Args)]
struct ExportArgs {
    /// The components, `<capability>/<id>`, such as `certificates/cert-manager`, printed one after
    /// another.
    #[arg(required = true)]
    components: Vec<String>,
    /// `helmchart`: K3s/RKE2 `HelmChart` objects, which helm-controller installs. `helm`: shell
    /// commands with `helm upgrade --install`. `flux`: Flux sources and `HelmRelease` objects.
    /// `argocd`: Argo CD `Application` objects.
    #[arg(long, value_enum, default_value_t = Format::Helmchart)]
    format: Format,
    /// The sizing: `single-node` or `multi-node`.
    #[arg(long, default_value = PROFILES[0], value_parser = PROFILES)]
    profile: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Helmchart,
    Helm,
    Flux,
    Argocd,
}

impl Format {
    const fn install(self) -> InstallFormat {
        match self {
            Self::Helmchart => InstallFormat::K3s,
            Self::Helm => InstallFormat::Helm,
            Self::Flux => InstallFormat::Flux,
            Self::Argocd => InstallFormat::ArgoCd,
        }
    }
}

pub(crate) fn run(args: &ComponentsArgs) -> ExitCode {
    match &args.command {
        ComponentsCommand::Export(export) => match text(export) {
            Ok(text) => {
                output::report(&text);
                ExitCode::SUCCESS
            }
            Err(error) => {
                output::error(&error.to_string());
                ExitCode::FAILURE
            }
        },
    }
}

fn text(args: &ExportArgs) -> Result<String, InstallError> {
    let format = args.format.install();
    let separator = if args.format == Format::Helm {
        "\n"
    } else {
        "---\n"
    };
    let mut out = String::new();
    for component in &args.components {
        let text = guide(component, &args.profile)?
            .formats
            .into_iter()
            .find(|f| f.format == format)
            .map_or_default(|f| f.text);
        if !out.is_empty() && !out.ends_with(separator) {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(separator);
        }
        out.push_str(&text);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(subcommand)]
        command: ComponentsCommand,
    }

    fn export(args: &[&str]) -> ExportArgs {
        let cli = Cli::try_parse_from(["components", "export"].iter().chain(args)).unwrap();
        let ComponentsCommand::Export(export) = cli.command;
        export
    }

    #[test]
    fn export_prints_helm_charts_by_default_and_helm_commands_on_request() {
        let charts = text(&export(&["certificates/cert-manager"])).unwrap();
        assert!(charts.contains("kind: HelmChart"), "{charts}");
        let commands = text(&export(&["certificates/cert-manager", "--format", "helm"])).unwrap();
        assert!(commands.contains("helm upgrade --install"), "{commands}");
    }

    #[test]
    fn export_prints_each_component_named() {
        let both = text(&export(&[
            "certificates/cert-manager",
            "image-updates/keel",
        ]))
        .unwrap();
        let one = |c: &str| text(&export(&[c])).unwrap();
        assert!(both.contains(&one("certificates/cert-manager")), "{both}");
        assert!(both.contains(&one("image-updates/keel")), "{both}");
        assert!(
            text(&export(&["certificates/cert-manager", "certificates/nope"])).is_err(),
            "an unknown one fails the whole export"
        );
    }

    #[test]
    fn export_refuses_an_unknown_component_or_profile() {
        assert!(matches!(
            text(&export(&["certificates/nope"])),
            Err(InstallError::UnknownComponent(_))
        ));
        assert!(
            Cli::try_parse_from([
                "components",
                "export",
                "certificates/cert-manager",
                "--profile",
                "huge"
            ])
            .is_err()
        );
    }
}
