#![doc = include_str!("../README.md")]

mod checkout;
mod output;

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask dev-cluster checkout <dir> <pin> <url>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["dev-cluster", "checkout", dir, pin, url] => {
            checkout::Pin::parse(pin).and_then(|pin| checkout::checkout(Path::new(dir), &pin, url))
        }
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}
