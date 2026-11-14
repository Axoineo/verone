//! The `verone` command-line tool.

use clap::Parser;

/// A virtual Android phone that runs on your computer.
#[derive(Debug, Parser)]
#[command(name = "verone", version, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
