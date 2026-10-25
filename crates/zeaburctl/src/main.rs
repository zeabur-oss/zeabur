//! `zeaburctl`: the command-line tool for people using a cluster.

#![forbid(unsafe_code)]

mod doctor;
mod in_process;
mod output;
mod pack;
mod ui;
mod up;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Kubernetes made simple and secure by default.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open the web interface on this machine, with your current kubeconfig context.
    Ui(ui::UiArgs),
    /// Check the cluster version, the Zeabur Identity setup and the components Zeabur Catalog
    /// recommends.
    ///
    /// Exit codes, as monitoring plugins (Nagios) and `zeabur-installer doctor` use them: 0 when
    /// all is well, 1 for a warning, 2 for something critical, 3 when the cluster cannot be
    /// reached or a check cannot run. An invalid command line also exits 2.
    Doctor(doctor::DoctorArgs),
    /// Upload a folder to a service that builds one, and build it.
    ///
    /// The files `.gitignore` and `.dockerignore` leave out are not sent. The service is made with
    /// "Upload a folder" in the web interface.
    Up(up::UpArgs),
}

fn main() -> ExitCode {
    let args = Args::parse();
    // Every command holds the kubeconfig's credential in memory.
    if let Err(error) = zeabur_core::secret::forbid_core_dumps() {
        output::error(&format!("core dumps stay possible: {error}"));
    }
    match args.command {
        Command::Ui(ui_args) => ui::run(&ui_args),
        Command::Doctor(doctor_args) => doctor::run(&doctor_args),
        Command::Up(up_args) => up::run(&up_args),
    }
}
