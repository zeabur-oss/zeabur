//! `zeaburctl`: the command-line tool for people using a cluster.

#![forbid(unsafe_code)]

mod doctor;
mod in_process;
mod mcp;
mod output;
mod ui;

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
    /// Serve a read-only Model Context Protocol server over stdio.
    Mcp(mcp::McpArgs),
    /// Check the cluster version, the Zeabur Identity setup and the recommended components.
    Doctor(doctor::DoctorArgs),
}

fn main() -> ExitCode {
    let args = Args::parse();
    match args.command {
        Command::Ui(ui_args) => ui::run(&ui_args),
        Command::Mcp(mcp_args) => mcp::run(&mcp_args),
        Command::Doctor(doctor_args) => doctor::run(&doctor_args),
    }
}
