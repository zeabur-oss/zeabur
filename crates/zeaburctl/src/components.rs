//! `zeaburctl components export`: Zeabur Catalog components' manifests or Helm commands, to
//! install them on a cluster. The text comes from the catalogue in this binary
//! ([`zeabur_catalog::install`]), followed by the objects Zeabur adds to the component
//! ([`zeabur_catalog::zeabur`]); no cluster is read.

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
    /// Values the component asks for show as `<name>` placeholders; replace them first. Zeabur's
    /// own objects for the component (its alert rules, Zeabur Proxy's access to the logs) come
    /// last; apply them once the component is installed.
    Export(ExportArgs),
}

#[derive(Debug, clap::Args)]
struct ExportArgs {
    /// The components, by id, such as `cert-manager`, printed one after another.
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
    /// The namespace Zeabur runs in, which Zeabur's objects admit.
    #[arg(long, default_value = "zeabur")]
    zeabur_namespace: String,
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
        let text = text + &zeaburs(component, args)?;
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

/// The objects Zeabur adds to `component`, in the export's format.
fn zeaburs(component: &str, args: &ExportArgs) -> Result<String, InstallError> {
    let yaml = zeabur_catalog::zeabur::yaml(component, &args.zeabur_namespace)
        .map_err(|e| InstallError::Catalogue(component.to_owned(), e))?;
    if yaml.is_empty() {
        return Ok(String::new());
    }
    let comment = format!("# Zeabur's objects for {component}: apply them once it is installed.\n");
    Ok(if args.format == Format::Helm {
        format!(
            "\n{comment}kubectl apply --server-side --field-manager zeabur -f - <<'EOF'\n{yaml}EOF\n"
        )
    } else {
        format!("---\n{comment}{yaml}")
    })
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
        let charts = text(&export(&["cert-manager"])).unwrap();
        assert!(charts.contains("kind: HelmChart"), "{charts}");
        let commands = text(&export(&["cert-manager", "--format", "helm"])).unwrap();
        assert!(commands.contains("helm upgrade --install"), "{commands}");
    }

    #[test]
    fn export_prints_each_component_named() {
        let both = text(&export(&["cert-manager", "keel"])).unwrap();
        let one = |c: &str| text(&export(&[c])).unwrap();
        assert!(both.contains(&one("cert-manager")), "{both}");
        assert!(both.contains(&one("keel")), "{both}");
        assert!(
            text(&export(&["cert-manager", "nope"])).is_err(),
            "an unknown one fails the whole export"
        );
    }

    #[test]
    fn export_adds_zeaburs_objects_in_each_format() {
        let rules = text(&export(&["victoria-metrics"])).unwrap();
        assert!(rules.contains("kind: VMRule"), "{rules}");
        let policy = text(&export(&[
            "victoria-logs",
            "--format",
            "helm",
            "--zeabur-namespace",
            "platform",
        ]))
        .unwrap();
        assert!(policy.contains("kubectl apply --server-side"), "{policy}");
        assert!(
            policy.contains("kubernetes.io/metadata.name: platform"),
            "{policy}"
        );
        let none = text(&export(&["cert-manager"])).unwrap();
        assert!(!none.contains("Zeabur's objects"), "{none}");
    }

    #[test]
    fn export_refuses_an_unknown_component_or_profile() {
        assert!(matches!(
            text(&export(&["nope"])),
            Err(InstallError::UnknownComponent(_))
        ));
        assert!(
            Cli::try_parse_from(["components", "export", "cert-manager", "--profile", "huge"])
                .is_err()
        );
    }
}
