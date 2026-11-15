//! The `verone` command-line tool.

mod output;

use std::process::ExitCode;

use clap::Parser;
use verone_core::Result;
use verone_core::process::Runner;

/// A virtual Android phone that runs on your computer.
#[derive(Debug, Parser)]
#[command(name = "verone", version, about, arg_required_else_help = true)]
struct Cli {
    /// Print results as JSON, for use by other programs.
    #[arg(long, global = true)]
    json: bool,

    /// Show each external command before it runs.
    #[arg(short, long, global = true)]
    verbose: bool,
}

/// What every command needs: how to print and how to run programs.
#[derive(Debug, Clone, Copy)]
struct Context {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "first read by the `doctor` command in P1-15")
    )]
    output: output::Mode,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "first read by the engine commands in P1-08")
    )]
    runner: Runner,
}

impl Context {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            output: output::Mode::from_json_flag(cli.json),
            runner: Runner::new(cli.verbose),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::print_error(&error);
            ExitCode::from(error.exit_code())
        }
    }
}

/// Runs the requested command. Commands are added by later Phase 1 tasks.
fn run(cli: &Cli) -> Result<()> {
    let _context = Context::from_cli(cli);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_set_the_context() {
        let cli = Cli::parse_from(["verone", "--json", "-v"]);
        assert!(cli.json);
        assert!(cli.verbose);
        let context = Context::from_cli(&cli);
        assert_eq!(context.output, output::Mode::Json);
        let _runner = context.runner;
    }

    #[test]
    fn the_command_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
