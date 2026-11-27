//! `zeaburctl templates export`: what deploying a Zeabur Catalog template into a project writes,
//! the same objects the web interface's plan writes ([`zeabur_catalog::Catalog::export`]), from the
//! templates in this binary. The cluster is read only to say which components the template needs
//! and the cluster lacks.

use std::collections::BTreeMap;
use std::process::ExitCode;

use clap::Subcommand;
use serde::Deserialize;
use zeabur_api::cluster::templates::{Requirement, RequirementState};
use zeabur_catalog::{Catalog, RenderError, RenderOptions};

use crate::in_process::InProcess;
use crate::output;

/// What to do with the catalogue's templates.
#[derive(Debug, clap::Args)]
pub(crate) struct TemplatesArgs {
    #[command(subcommand)]
    command: TemplatesCommand,
}

#[derive(Debug, Subcommand)]
enum TemplatesCommand {
    /// Print the objects that deploying a template into a project writes, the same ones the web
    /// interface writes, as YAML for `kubectl apply --server-side`.
    ///
    /// Required inputs that are not given show as `<name>` placeholders; replace them first. Each
    /// run generates new values for the template's generated Secrets, so save one export and
    /// apply that file.
    Export(ExportArgs),
}

#[derive(Debug, clap::Args)]
struct ExportArgs {
    /// The template, by id, such as `miniflux`.
    template: String,
    /// The project to deploy into: its namespace.
    #[arg(long)]
    project: String,
    /// The domain, for a template that routes one.
    #[arg(long)]
    domain: Option<String>,
    /// The cert-manager `ClusterIssuer` Zeabur uses (the chart's `domains.clusterIssuer`), for a
    /// template that asks for certificates.
    #[arg(long)]
    cluster_issuer: Option<String>,
    /// An input's value, as `<name>=<value>`; repeat it for each input.
    #[arg(long = "set", value_parser = input)]
    inputs: Vec<(String, String)>,
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
}

fn input(text: &str) -> Result<(String, String), String> {
    text.split_once('=')
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .ok_or_else(|| format!("{text} is not <name>=<value>"))
}

/// The part of `GET /api/v1/cluster/templates/{id}` this command reads.
#[derive(Debug, Deserialize)]
struct Requires {
    requires: Vec<Requirement>,
}

pub(crate) async fn run(args: &TemplatesArgs) -> ExitCode {
    let TemplatesCommand::Export(export) = &args.command;
    let text = match text(export) {
        Ok(text) => text,
        Err(error) => {
            output::error(&error.to_string());
            return ExitCode::FAILURE;
        }
    };
    warn_missing(export).await;
    output::report(&text);
    ExitCode::SUCCESS
}

fn text(args: &ExportArgs) -> Result<String, RenderError> {
    let inputs: BTreeMap<String, String> = args.inputs.iter().cloned().collect();
    let objects = Catalog::embedded().export(
        &args.template,
        &RenderOptions {
            namespace: &args.project,
            domain: args.domain.as_deref(),
            cluster_issuer: args.cluster_issuer.as_deref(),
            inputs: &inputs,
        },
    )?;
    objects
        .iter()
        .map(serde_saphyr::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map(|documents| documents.join("---\n"))
        .map_err(|e| RenderError::Broken {
            template: args.template.clone(),
            detail: e.to_string(),
        })
}

/// Says on stderr which components the template needs that the cluster does not serve, with the
/// command that prints what installs each.
async fn warn_missing(args: &ExportArgs) {
    let read = async {
        let api =
            InProcess::from_kubeconfig(args.context.as_deref(), zeabur_api::Settings::default())
                .await?;
        api.get::<Requires>(&format!("/api/v1/cluster/templates/{}", args.template))
            .await
            .map_err(|problem| problem.detail.clone())
    };
    match read.await {
        Ok(view) => {
            for line in missing(&view.requires) {
                output::info(&line);
            }
        }
        Err(error) => output::info(&format!(
            "zeaburctl: could not check the components {} needs: {error}",
            args.template
        )),
    }
}

fn missing(requires: &[Requirement]) -> Vec<String> {
    requires
        .iter()
        .filter(|r| r.state == RequirementState::Missing)
        .map(|r| {
            let install = r.component.as_ref().map_or_else(
                || {
                    format!(
                        "a component of {} (zeaburctl doctor lists them)",
                        r.capability
                    )
                },
                |id| format!("zeaburctl components export {id}"),
            );
            format!(
                "zeaburctl: this template needs {}, which the cluster does not have. Install it \
                 first: {install}",
                r.name
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(subcommand)]
        command: TemplatesCommand,
    }

    fn export(args: &[&str]) -> ExportArgs {
        let cli = Cli::try_parse_from(["templates", "export"].iter().chain(args)).unwrap();
        let TemplatesCommand::Export(export) = cli.command;
        export
    }

    #[test]
    fn export_prints_placeholders_for_inputs_not_given() {
        let missing = text(&export(&["wordpress", "--project", "blog"])).unwrap();
        assert!(missing.contains("<adminEmail>"), "{missing}");
        assert!(missing.contains("namespace: blog"), "{missing}");
        let given = text(&export(&[
            "wordpress",
            "--project",
            "blog",
            "--domain",
            "blog.example.com",
            "--cluster-issuer",
            "letsencrypt",
            "--set",
            "adminEmail=me@example.com",
        ]))
        .unwrap();
        assert!(!given.contains("<adminEmail>"), "{given}");
        assert!(given.contains("blog.example.com"), "{given}");
    }

    #[test]
    fn export_refuses_an_unknown_template_or_a_bad_input() {
        assert!(matches!(
            text(&export(&["nope", "--project", "blog"])),
            Err(RenderError::UnknownTemplate(_))
        ));
        assert!(
            Cli::try_parse_from(["templates", "export", "wordpress", "--set", "adminEmail"])
                .is_err()
        );
    }

    #[test]
    fn a_missing_component_names_the_command_that_installs_it() {
        let requires = [
            Requirement {
                capability: "certificates".to_owned(),
                component: Some("cert-manager".to_owned()),
                name: "cert-manager".to_owned(),
                state: RequirementState::Missing,
            },
            Requirement {
                capability: "ingress".to_owned(),
                component: Some("traefik".to_owned()),
                name: "Traefik".to_owned(),
                state: RequirementState::Installed,
            },
        ];
        let lines = missing(&requires);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("zeaburctl components export cert-manager"),
            "{lines:?}"
        );
    }
}
