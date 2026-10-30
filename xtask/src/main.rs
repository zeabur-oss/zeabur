#![doc = include_str!("../README.md")]

mod checkout;
mod dev_cluster;
mod e2e_proxy;
mod output;
mod process;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

const AFTER_HELP: &str = "\
ZEABUR_IDENTITY_PINNED and ZEABUR_IDENTITY_GIT give the commit Zeabur Identity is built from
without --identity-src; ZEABUR_BUILDER_PINNED and ZEABUR_BUILDER_GIT likewise for Zeabur Builder.
ZEABUR_REGISTRY_MIRRORS (`docker.io=http://mirror:5001,ghcr.io=...`) makes the node pull through
those mirrors; K3S_IMAGE picks the k3s release; CATALOG_STOP_AT leaves out the catalog's releases
from that one on. State is in target/dev-cluster (kubeconfig, CA, zeabur-identityctl).";

/// Development tasks for this repository.
#[derive(Debug, Parser)]
#[command(name = "xtask", bin_name = "cargo xtask", after_help = AFTER_HELP)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
enum Task {
    /// The k3d cluster with Zeabur Identity and Zeabur at <https://zeabur.localhost>.
    #[command(subcommand)]
    DevCluster(DevCluster),
    /// Zeabur Proxy's canary: no person reads another namespace's log lines or series. Needs a
    /// cluster where the chart runs with `proxy.enabled` and the catalog's logs capability, and
    /// KUBECONFIG pointing at it. `ZEABUR_NAMESPACE`, `ZEABUR_PROXY_SERVICE` and `ZEABUR_LOGS_URL`
    /// say where they run; `ZEABUR_PROXY_E2E_KEEP=1` keeps what it made.
    E2eProxy,
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
enum DevCluster {
    /// Create the cluster, installed with zeabur-installer's own objects and values. Runs on Linux
    /// with Docker and Nix inside `nix develop`; uses this machine's port 443. A second `up` on a
    /// running cluster brings it in line with the checkout.
    Up(dev_cluster::Up),
    /// Delete the cluster, its volume and its images.
    Down,
    /// Print cluster-scoped objects and namespace metadata.
    Snapshot,
    /// Check out a pinned commit or release tag from `URL` in `DIR`.
    Checkout {
        dir: PathBuf,
        pin: String,
        url: String,
    },
}

impl Task {
    fn run(self) -> Result<(), String> {
        match self {
            Self::DevCluster(DevCluster::Up(options)) => dev_cluster::up(&options),
            Self::DevCluster(DevCluster::Down) => dev_cluster::down(),
            Self::DevCluster(DevCluster::Snapshot) => dev_cluster::snapshot(),
            Self::DevCluster(DevCluster::Checkout { dir, pin, url }) => {
                checkout::Pin::parse(&pin).and_then(|pin| checkout::checkout(&dir, &pin, &url))
            }
            Self::E2eProxy => e2e_proxy::run(),
        }
    }
}

fn main() -> ExitCode {
    match Cli::parse().task.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, DevCluster, Task};
    use crate::dev_cluster::Profile;

    fn parse(args: &[&str]) -> Result<Task, clap::Error> {
        Cli::try_parse_from(std::iter::once("xtask").chain(args.iter().copied()))
            .map(|cli| cli.task)
    }

    #[test]
    fn up_takes_its_options() {
        match parse(&["dev-cluster", "up", "--profile", "bare"]) {
            Ok(Task::DevCluster(DevCluster::Up(up))) => assert_eq!(up.profile, Profile::Bare),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn down_and_snapshot_take_no_arguments() {
        assert_eq!(
            parse(&["dev-cluster", "down"]).ok(),
            Some(Task::DevCluster(DevCluster::Down))
        );
        assert_eq!(
            parse(&["dev-cluster", "snapshot"]).ok(),
            Some(Task::DevCluster(DevCluster::Snapshot))
        );
        assert!(parse(&["dev-cluster", "down", "now"]).is_err());
    }

    #[test]
    fn an_unknown_task_is_a_usage_error() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["dev-cluster"]).is_err());
        assert!(parse(&["dev-cluster", "start"]).is_err());
    }
}
