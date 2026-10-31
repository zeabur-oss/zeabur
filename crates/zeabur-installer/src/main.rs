//! `zeabur-installer`: K3s or RKE2, Zeabur Catalog components, Zeabur Identity and Zeabur on one
//! server, or `KubeSolo` and the components for one person.

mod apply;
mod doctor;
mod gui;
mod join;
mod load;
mod node;
mod out;
mod personal;
mod remove;
mod render;
mod telemetry;
mod uninstall;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use zeabur_installer_host::{HostError, facts, token};
use zeabur_installer_plan::config::{AccessMode, Role};
use zeabur_installer_plan::{PlanError, plan};

/// Install K3s or RKE2 hardened to the CIS benchmark, the Zeabur Catalog's components, Zeabur
/// Identity and Zeabur on one Linux server, from one configuration file. With `use: personal`,
/// install `KubeSolo` and the components for one person instead.
#[derive(Debug, Parser)]
#[command(name = "zeabur-installer", version, about)]
struct Cli {
    /// The configuration file.
    #[arg(
        long,
        short,
        global = true,
        default_value = "/etc/zeabur-installer/config.yaml"
    )]
    config: PathBuf,
    /// Do not join the experience program for this run (<https://oss.zeabur.com/docs/zeabur/reference/telemetry/>).
    #[arg(long, global = true)]
    no_telemetry: bool,
    /// The command to run.
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show what `apply` would install and run its host checks, without changing anything.
    Plan {
        /// Print the plan as JSON, without the host checks.
        #[arg(long)]
        json: bool,
    },
    /// Install, or bring an existing install in line with the configuration. Safe to run again.
    Apply,
    /// Print Zeabur's namespace, Zeabur Identity's Ingress and Zeabur's `HelmChart` as `apply`
    /// makes them, as a JSON `List`, for a K3s/RKE2 cluster that has its other parts from another
    /// source (`scripts/dev-cluster.sh`). Reads no host and no catalog.
    Manifests {
        /// The cluster's name, which `apply` takes from the host name.
        #[arg(long)]
        cluster_name: String,
    },
    /// Serve a page that walks through the configuration and runs `apply`, on 127.0.0.1 by
    /// default; reach it with `ssh -L`.
    Gui {
        /// Where to listen.
        #[arg(long, default_value = "127.0.0.1:8085")]
        listen: SocketAddr,
    },
    /// Print a new one-time link for a user to register a passkey, as `zeabur-identityctl
    /// setup-link` does, with this server's cluster and issuer. For the first admin when the link
    /// `apply` printed has expired. Run it on a server, as root.
    SetupLink {
        /// The user the link is for.
        #[arg(long, default_value = "admin")]
        username: String,
        /// How long the link works, such as `30m` or `24h`.
        #[arg(long, default_value = "24h")]
        valid_for: String,
        /// Delete the user's passkeys and sessions first, for a user who lost them.
        #[arg(long)]
        reset: bool,
    },
    /// Check the install.
    ///
    /// Exit codes, as monitoring plugins (Nagios) and `zeaburctl doctor` use them: 0 when all is
    /// well, 1 for a warning, 2 for something critical, 3 when the checks cannot run (not root).
    /// An invalid command line also exits 2.
    Doctor,
    /// Remove Zeabur, or Zeabur and Zeabur Identity. K3s itself is removed with
    /// `/usr/local/bin/k3s-uninstall.sh`, RKE2 with `/usr/local/bin/rke2-uninstall.sh`. On a node
    /// that joined a cluster, prints how to take
    /// the node out. Says which of the installer's files stay after that. After Zeabur Identity
    /// is removed, the other servers stop trusting it once `apply` runs on each of them.
    Uninstall {
        /// What to remove; required on a server installed with Zeabur, unless with `--purge`.
        #[arg(value_enum)]
        what: Option<Product>,
        /// Once K3s or RKE2 is uninstalled, remove the installer's download cache, local CA copy
        /// and configuration. Volumes stay.
        #[arg(long, conflicts_with = "what")]
        purge: bool,
    },
    /// Add nodes to this cluster; run on a server.
    Node {
        /// What to do.
        #[command(subcommand)]
        command: NodeCommand,
    },
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    /// Print the configuration and commands that join a new VM to this cluster. The VM must be at
    /// the same hosting provider and in the same region; another availability zone is fine. After
    /// a worker joins, run `node sync` on a server.
    Add {
        /// `worker` runs workloads; `server`, on RKE2 only, also runs the control plane and etcd.
        #[arg(long, value_enum, default_value_t = NodeRole::Worker)]
        role: NodeRole,
        /// The address the new VM reaches this server at; by default this node's `InternalIP`.
        #[arg(long)]
        address: Option<String>,
        /// How long a worker's token can join nodes, such as `30m` or `1h`; at most `24h`.
        #[arg(long, default_value = "1h", value_parser = ttl)]
        ttl: Duration,
    },
    /// Let the current nodes' traffic through the network policies the installer made. Run it on
    /// a server after a worker joins and after a node is removed.
    Sync,
}

fn ttl(text: &str) -> Result<Duration, String> {
    token::parse_ttl(text)
        .ok_or_else(|| "use minutes or hours, such as 30m or 1h, up to 24h".to_owned())
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NodeRole {
    /// A K3s or RKE2 agent.
    Worker,
    /// An RKE2 server.
    Server,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Product {
    /// Zeabur only.
    Zeabur,
    /// Zeabur and Zeabur Identity.
    Identity,
}

/// Why the installer stopped.
#[derive(Debug, thiserror::Error)]
enum InstallError {
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error("{0}")]
    Message(String),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // A configuration that does not parse is reported by the command itself.
    if let Some(config) = load::personal(&cli.config) {
        return personal(&cli, &config);
    }
    let joining = load::join(&cli.config);
    let result = match (&cli.command, joining) {
        (Command::Plan { json }, Some(config)) => join::show_plan(&config, *json),
        (Command::Apply, Some(config)) => join::apply(&config),
        (Command::Doctor, Some(config)) => return join::doctor(&config),
        (Command::Uninstall { purge, .. }, Some(config)) => {
            join::uninstall(&config, &cli.config, *purge)
        }
        (
            Command::Gui { .. }
            | Command::Node { .. }
            | Command::Manifests { .. }
            | Command::SetupLink { .. },
            Some(_),
        ) => Err(InstallError::Message(format!(
            "{} joins this machine to a cluster; run this command on the first server",
            cli.config.display()
        ))),
        (Command::Plan { json }, None) => show_plan(&cli, *json),
        (Command::Apply, None) => apply::run(&cli.config, cli.no_telemetry),
        (Command::Manifests { cluster_name }, None) => manifests(&cli, cluster_name),
        (
            Command::SetupLink {
                username,
                valid_for,
                reset,
            },
            None,
        ) => apply::new_setup_link(&cli.config, username, valid_for, *reset),
        (Command::Gui { listen }, None) => gui::run(&cli.config, *listen, cli.no_telemetry),
        (Command::Doctor, None) => return doctor::run(),
        (
            Command::Uninstall {
                what: Some(what), ..
            },
            None,
        ) => uninstall::run(&cli.config, *what),
        (Command::Uninstall { what: None, purge }, None) => {
            let left = uninstall::leftovers(&cli.config, None);
            if *purge {
                uninstall::purge(&left, &cli.config)
            } else if uninstall::installed().is_some() {
                Err(InstallError::Message(
                    "name what to remove: zeabur-installer uninstall zeabur|identity".to_owned(),
                ))
            } else {
                out::line(&uninstall::leftovers_text(&left));
                Ok(())
            }
        }
        (
            Command::Node {
                command: NodeCommand::Add { role, address, ttl },
            },
            None,
        ) => node::add(
            &cli.config,
            &node::AddOptions {
                role: match role {
                    NodeRole::Worker => Role::Worker,
                    NodeRole::Server => Role::Server,
                },
                address: address.clone(),
                ttl: *ttl,
            },
        ),
        (
            Command::Node {
                command: NodeCommand::Sync,
            },
            None,
        ) => node::sync(&cli.config),
    };
    finish(result)
}

fn finish(result: Result<(), InstallError>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            out::err(&format!("error: {error}"));
            ExitCode::FAILURE
        }
    }
}

/// The commands of a personal install.
fn personal(cli: &Cli, config: &zeabur_installer_plan::config::PersonalConfig) -> ExitCode {
    finish(match &cli.command {
        Command::Doctor => return personal::doctor(config),
        Command::Plan { json } => personal::show_plan(config, *json),
        Command::Apply => personal::apply(config, &cli.config, cli.no_telemetry),
        Command::Gui { listen } => gui::run(&cli.config, *listen, cli.no_telemetry),
        Command::Uninstall { .. } => {
            out::line(&personal::uninstall_text());
            Ok(())
        }
        Command::Node { .. } | Command::Manifests { .. } | Command::SetupLink { .. } => {
            Err(InstallError::Message(format!(
                "{} is a personal install: one node and no Zeabur panel",
                cli.config.display()
            )))
        }
    })
}

fn show_plan(cli: &Cli, json: bool) -> Result<(), InstallError> {
    let (config, defaults) = load::config_or_defaults(&cli.config)?;
    let index = load::catalog(config.sources.catalog.as_deref(), apply::STATE_DIR)?;
    let plan = apply::make_plan(&index, &config)?;
    if defaults && !json {
        let kind = match config.access.mode {
            AccessMode::Domain => "with Let's Encrypt certificates",
            AccessMode::Local => "on a test machine with a local CA",
        };
        out::line(&format!(
            "No configuration at {}; planning with the defaults: {} and {} {kind}, \
             standard components. Write the file to change them (https://oss.zeabur.com/docs/zeabur/reference/installer-configuration/).\n",
            cli.config.display(),
            config.access.zeabur,
            config.access.identity
        ));
    }
    if json {
        let text = serde_json::to_string_pretty(&plan)
            .map_err(|e| InstallError::Message(e.to_string()))?;
        out::line(&text);
    } else {
        out::line(render::plan(&plan).trim_end());
        if apply::show_checks(
            zeabur_installer_host::preflight::Node::Server(config.distribution),
            config.host.require_encrypted_disk,
            plan.node_prep.as_ref(),
        ) {
            out::line("apply stops until what failed is fixed.");
        }
    }
    Ok(())
}

fn manifests(cli: &Cli, cluster_name: &str) -> Result<(), InstallError> {
    let config = load::config(&cli.config)?;
    let facts = plan::Facts {
        arch: facts::arch(std::env::consts::ARCH).to_owned(),
        hostname: cluster_name.to_owned(),
        node_name: None,
        host_hex: String::new(),
        memory_mib: 0,
        node_ips: Vec::new(),
        record: None,
        installer: None,
    };
    let (identity, zeabur) = plan::products_only(&config, &facts)?.ok_or_else(|| {
        InstallError::Message(format!("{} says `zeabur: false`", cli.config.display()))
    })?;
    let namespace = plan::namespace_object(&plan::Namespace {
        name: plan::ZEABUR_NAMESPACE.to_owned(),
        privileged: None,
        baseline: None,
        public: false,
    });
    let list = serde_json::json!({
        "apiVersion": "v1",
        "kind": "List",
        "items": [namespace, identity.ingress, apply::zeabur_helm_chart(&zeabur, None)?],
    });
    out::line(&list.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstall_takes_no_argument_on_a_joined_node() {
        let cli = Cli::try_parse_from(["zeabur-installer", "uninstall"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Uninstall {
                what: None,
                purge: false
            }
        ));
        let cli = Cli::try_parse_from(["zeabur-installer", "uninstall", "zeabur"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Uninstall {
                what: Some(Product::Zeabur),
                ..
            }
        ));
        let cli = Cli::try_parse_from(["zeabur-installer", "uninstall", "--purge"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Uninstall { purge: true, .. }
        ));
        assert!(
            Cli::try_parse_from(["zeabur-installer", "uninstall", "zeabur", "--purge"]).is_err()
        );
    }
}
