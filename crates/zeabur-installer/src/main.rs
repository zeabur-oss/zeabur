//! `zeabur-installer`: RKE2, Zeabur Catalog components, Zeabur Identity and Zeabur on one server.

mod apply;
mod doctor;
mod gui;
mod join;
mod load;
mod node;
mod out;
mod render;
mod uninstall;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use zeabur_installer_host::{HostError, facts, token};
use zeabur_installer_plan::config::Role;
use zeabur_installer_plan::{PlanError, plan};

/// Install RKE2 hardened with the CIS profile, the Zeabur Catalog's components, Zeabur Identity
/// and Zeabur on one Linux server, from one configuration file.
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
    /// Serve a page that walks through the configuration and runs `apply`, on 127.0.0.1 by
    /// default; reach it with `ssh -L`.
    Gui {
        /// Where to listen.
        #[arg(long, default_value = "127.0.0.1:8085")]
        listen: SocketAddr,
    },
    /// Check the install.
    ///
    /// Exit codes, as monitoring plugins (Nagios) and `zeaburctl doctor` use them: 0 when all is
    /// well, 1 for a warning, 2 for something critical, 3 when the checks cannot run (not root).
    /// An invalid command line also exits 2.
    Doctor,
    /// Remove Zeabur, or Zeabur and Zeabur Identity. RKE2 itself is removed with
    /// `/usr/local/bin/rke2-uninstall.sh`. On a node that joined a cluster, prints how to take
    /// the node out. Says which of the installer's files stay after that.
    Uninstall {
        /// What to remove; required on a server installed with Zeabur, unless with `--purge`.
        #[arg(value_enum)]
        what: Option<Product>,
        /// Once RKE2 is uninstalled, remove the installer's download cache, local CA copy and
        /// configuration. Volumes stay.
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
    /// the same hosting provider and in the same region; another availability zone is fine.
    Add {
        /// `worker` runs workloads; `server` also runs the control plane and etcd.
        #[arg(long, value_enum, default_value_t = NodeRole::Worker)]
        role: NodeRole,
        /// The address the new VM reaches this server at; by default this node's `InternalIP`.
        #[arg(long)]
        address: Option<String>,
        /// How long a worker's token can join nodes, such as `30m` or `1h`; at most `24h`.
        #[arg(long, default_value = "1h", value_parser = ttl)]
        ttl: Duration,
    },
    /// Let the current nodes' traffic through the network policies the installer made. Run it
    /// after a node joins.
    Sync,
}

fn ttl(text: &str) -> Result<Duration, String> {
    token::parse_ttl(text)
        .ok_or_else(|| "use minutes or hours, such as 30m or 1h, up to 24h".to_owned())
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NodeRole {
    /// An RKE2 agent.
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
    let joining = load::join(&cli.config);
    let result = match (&cli.command, joining) {
        (Command::Plan { json }, Some(config)) => join::show_plan(&config, *json),
        (Command::Apply, Some(config)) => join::apply(&config),
        (Command::Doctor, Some(config)) => return join::doctor(&config),
        (Command::Uninstall { purge, .. }, Some(config)) => {
            join::uninstall(&config, &cli.config, *purge)
        }
        (Command::Gui { .. } | Command::Node { .. }, Some(_)) => {
            Err(InstallError::Message(format!(
                "{} joins this machine to a cluster; run this command on the first server",
                cli.config.display()
            )))
        }
        (Command::Plan { json }, None) => show_plan(&cli, *json),
        (Command::Apply, None) => apply::run(&cli.config),
        (Command::Gui { listen }, None) => gui::run(&cli.config, *listen),
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
            } else if uninstall::rke2_installed() {
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
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            out::err(&format!("error: {error}"));
            ExitCode::FAILURE
        }
    }
}

fn show_plan(cli: &Cli, json: bool) -> Result<(), InstallError> {
    let (config, defaults) = load::config_or_defaults(&cli.config)?;
    let index = load::catalog(&config.sources)?;
    let plan = plan::plan(&index, &config, &facts::read()?)?;
    if defaults && !json {
        out::line(&format!(
            "No configuration at {}; planning with the defaults: a test machine at {} and {}, \
             standard components. Write the file to change them (docs/installer/configuration.md).\n",
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
            apply::UNIT,
            config.host.require_encrypted_disk,
            plan.node_prep.as_ref(),
        ) {
            out::line("apply stops until what failed is fixed.");
        }
    }
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
