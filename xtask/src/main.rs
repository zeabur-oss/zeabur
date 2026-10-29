#![doc = include_str!("../README.md")]

mod checkout;
mod dev_cluster;
mod e2e_proxy;
mod output;
mod process;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <task>

  dev-cluster up [options]  A k3d cluster with Zeabur Identity and Zeabur at
                            https://zeabur.localhost, installed with zeabur-installer's own
                            objects and values. Runs on Linux with Docker and Nix inside
                            `nix develop`; uses this machine's port 443. A second `up` on a
                            running cluster brings it in line with the checkout.
    --profile bare          only the cluster, for the cluster tests of crates/*/tests and
                            zeaburctl in local mode
    --profile full          also CloudNativePG with the Barman Cloud plugin, the Percona
                            Operator for MongoDB, Keel, zot, a test S3 and Zeabur Builder
    --identity-src <dir>    build Zeabur Identity from this working tree, as it is
    --builder-src <dir>     build Zeabur Builder from this working tree, as it is (full profile)
    --zeabur-image <image>  run this Zeabur image (a local image is imported, any other is
                            pulled) instead of one built from this checkout; Zeabur Proxy
                            stays off
    --zeabur-chart <chart>  install this chart (directory, .tgz or oci://) instead of
                            deploy/chart/zeabur
  dev-cluster down          delete the cluster, its volume and its images
  dev-cluster snapshot      print cluster-scoped objects and namespace metadata
  dev-cluster checkout <dir> <pin> <url>
                            check out a pinned commit or release tag from <url> in <dir>
  e2e-proxy                 Zeabur Proxy's canary: no person reads another namespace's log
                            lines or series. Needs a cluster where the chart runs with
                            `proxy.enabled` and the catalog's logs capability, and KUBECONFIG
                            pointing at it. ZEABUR_NAMESPACE, ZEABUR_PROXY_SERVICE and
                            ZEABUR_LOGS_URL say where they run; ZEABUR_PROXY_E2E_KEEP=1 keeps
                            what it made.

Without --identity-src, Zeabur Identity is built from the commit in ZEABUR_IDENTITY_PINNED,
fetched from ZEABUR_IDENTITY_GIT; ZEABUR_IDENTITY_SRC stands for --identity-src. Zeabur Builder
likewise comes from ZEABUR_BUILDER_PINNED, ZEABUR_BUILDER_GIT and ZEABUR_BUILDER_SRC.
ZEABUR_REGISTRY_MIRRORS (`docker.io=http://mirror:5001,ghcr.io=...`) makes the node pull through
those mirrors; K3S_IMAGE picks the k3s release; CATALOG_STOP_AT leaves out the catalog's releases
from that one on. State is in target/dev-cluster (kubeconfig, CA, zeabur-identityctl).
";

/// What `cargo xtask` was asked to do.
#[derive(Debug, PartialEq, Eq)]
enum Task {
    Up(dev_cluster::Up),
    Down,
    Snapshot,
    E2eProxy,
    Checkout {
        dir: PathBuf,
        pin: String,
        url: String,
    },
}

impl Task {
    fn parse(args: &[&str], env: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        match args {
            ["dev-cluster", "up", options @ ..] => {
                dev_cluster::Up::parse(options, env).map(Self::Up)
            }
            ["dev-cluster", "down"] => Ok(Self::Down),
            ["dev-cluster", "snapshot"] => Ok(Self::Snapshot),
            ["e2e-proxy"] => Ok(Self::E2eProxy),
            ["dev-cluster", "checkout", dir, pin, url] => Ok(Self::Checkout {
                dir: PathBuf::from(dir),
                pin: (*pin).to_owned(),
                url: (*url).to_owned(),
            }),
            _ => Err(String::new()),
        }
    }

    fn run(self) -> Result<(), String> {
        match self {
            Self::Up(options) => dev_cluster::up(&options),
            Self::Down => dev_cluster::down(),
            Self::Snapshot => dev_cluster::snapshot(),
            Self::E2eProxy => e2e_proxy::run(),
            Self::Checkout { dir, pin, url } => {
                checkout::Pin::parse(&pin).and_then(|pin| checkout::checkout(&dir, &pin, &url))
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let task = match Task::parse(&args, |name| std::env::var(name).ok()) {
        Ok(task) => task,
        Err(message) => {
            if !message.is_empty() {
                output::error(&message);
            }
            output::usage(USAGE);
            return ExitCode::from(2);
        }
    };
    match task.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Task;
    use crate::dev_cluster::Profile;

    fn parse(args: &[&str]) -> Result<Task, String> {
        Task::parse(args, |_| None)
    }

    #[test]
    fn up_takes_its_options() {
        match parse(&["dev-cluster", "up", "--profile", "bare"]) {
            Ok(Task::Up(up)) => assert_eq!(up.profile, Profile::Bare),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn down_and_snapshot_take_no_arguments() {
        assert_eq!(parse(&["dev-cluster", "down"]), Ok(Task::Down));
        assert_eq!(parse(&["dev-cluster", "snapshot"]), Ok(Task::Snapshot));
        assert!(parse(&["dev-cluster", "down", "now"]).is_err());
    }

    #[test]
    fn an_unknown_task_is_a_usage_error() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["dev-cluster"]).is_err());
        assert!(parse(&["dev-cluster", "start"]).is_err());
    }
}
