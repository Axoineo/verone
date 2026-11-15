//! The `verone` command-line tool.

mod commands;
mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use verone_core::Result;
use verone_core::process::Runner;
use verone_core::profile::DEFAULT_ANDROID;
use verone_core::storage::Storage;
use verone_engine_container::ContainerEngine;

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

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check that this computer can run phones, and say how to fix what is missing.
    Doctor,
    /// Download or list Android images.
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ImageCommand {
    /// Download the image for an Android version, if it is not already here.
    Pull {
        /// Android version.
        #[arg(long, default_value = DEFAULT_ANDROID)]
        android: String,
    },
    /// Show the supported images and whether each is downloaded.
    List,
}

/// What every command needs: how to print, where phones live and the engine.
struct Context {
    output: output::Mode,
    engine: ContainerEngine,
}

impl Context {
    fn from_cli(cli: &Cli) -> Result<Self> {
        let storage = Storage::from_env()?;
        let runner = Runner::new(cli.verbose);
        Ok(Self {
            output: output::Mode::from_json_flag(cli.json),
            engine: ContainerEngine::new(runner, storage.home()),
        })
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

/// Runs the requested command.
fn run(cli: &Cli) -> Result<()> {
    let Some(command) = &cli.command else {
        return Ok(());
    };
    let context = Context::from_cli(cli)?;
    let mut out = std::io::stdout().lock();
    match command {
        Command::Doctor => commands::doctor(&context.engine, context.output, &mut out),
        Command::Image { command } => match command {
            ImageCommand::Pull { android } => {
                commands::image_pull(&context.engine, android, context.output, &mut out)
            }
            ImageCommand::List => commands::image_list(&context.engine, context.output, &mut out),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_options_work_before_and_after_the_command() {
        let before = Cli::parse_from(["verone", "--json", "-v", "doctor"]);
        assert!(before.json && before.verbose);
        let after = Cli::parse_from(["verone", "image", "list", "--json"]);
        assert!(after.json);
    }

    #[test]
    fn image_pull_defaults_to_the_default_android_version() {
        let cli = Cli::parse_from(["verone", "image", "pull"]);
        match cli.command {
            Some(Command::Image {
                command: ImageCommand::Pull { android },
            }) => assert_eq!(android, "14"),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_command_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
