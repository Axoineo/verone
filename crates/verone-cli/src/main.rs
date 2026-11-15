//! The `verone` command-line tool.

mod commands;
mod output;
mod phone;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use verone_core::Result;
use verone_core::process::Runner;
use verone_core::profile::{DEFAULT_ANDROID, GraphicsMode};
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
    /// Create a new phone. It is not started.
    Create {
        /// A short name for the phone: lowercase letters, digits and hyphens.
        id: String,
        /// The name shown in windows and lists. Defaults to the id.
        #[arg(long)]
        name: Option<String>,
        /// Android version.
        #[arg(long, default_value = DEFAULT_ANDROID)]
        android: String,
        /// Screen size as WIDTHxHEIGHT. Defaults to 720x1280.
        #[arg(long)]
        size: Option<String>,
        /// Screen density. Defaults to 320.
        #[arg(long)]
        dpi: Option<u32>,
        /// How the screen is drawn.
        #[arg(long, value_enum, default_value_t = GraphicsChoice::Auto)]
        graphics: GraphicsChoice,
        /// Publish adb on this port of 127.0.0.1, for development tools.
        #[arg(long, default_value_t = 0)]
        adb_port: u16,
    },
    /// Start a phone and wait until Android is ready.
    Start { id: String },
    /// Shut a phone down cleanly.
    Stop { id: String },
    /// Stop a phone, then start it again.
    Restart { id: String },
    /// List all phones.
    List,
    /// Show everything about one phone.
    Info { id: String },
    /// Show a phone in a window, starting it first if needed.
    Open {
        id: String,
        /// Options passed on to scrcpy, after `--`.
        #[arg(last = true)]
        scrcpy_options: Vec<String>,
    },
    /// Run an adb command against a running phone. Example: verone adb work -- shell
    Adb {
        id: String,
        /// The adb command and its arguments, after `--`.
        #[arg(last = true)]
        arguments: Vec<String>,
    },
    /// Install an APK file on a running phone.
    Install { id: String, apk: PathBuf },
    /// Erase everything on a phone and keep its settings.
    Reset {
        id: String,
        /// Do not ask for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Remove a phone and everything on it.
    Delete {
        id: String,
        /// Do not ask for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Print a shell completion script. Example: verone completions bash
    Completions {
        /// The shell to generate the script for.
        shell: clap_complete::Shell,
    },
}

/// The graphics modes offered on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum GraphicsChoice {
    /// Use the GPU if a usable one exists, otherwise software.
    Auto,
    /// Always draw in software.
    Software,
    /// Always use the GPU.
    Gpu,
}

impl From<GraphicsChoice> for GraphicsMode {
    fn from(choice: GraphicsChoice) -> Self {
        match choice {
            GraphicsChoice::Auto => GraphicsMode::Auto,
            GraphicsChoice::Software => GraphicsMode::Software,
            GraphicsChoice::Gpu => GraphicsMode::Gpu,
        }
    }
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
    runner: Runner,
    storage: Storage,
    engine: ContainerEngine,
}

impl Context {
    fn from_cli(cli: &Cli) -> Result<Self> {
        let storage = Storage::from_env()?;
        let runner = Runner::new(cli.verbose);
        Ok(Self {
            output: output::Mode::from_json_flag(cli.json),
            runner,
            engine: ContainerEngine::new(runner, storage.home()),
            storage,
        })
    }

    fn env(&self) -> phone::Env<'_, ContainerEngine> {
        phone::Env {
            storage: &self.storage,
            engine: &self.engine,
            mode: self.output,
        }
    }

    fn tools(&self) -> phone::Tools<'_> {
        phone::Tools {
            adb: self.engine.adb(),
            runner: self.runner,
            scrcpy: vec![phone::SCRCPY_PROGRAM.to_string()],
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

/// Runs the requested command.
fn run(cli: &Cli) -> Result<()> {
    let Some(command) = &cli.command else {
        return Ok(());
    };
    // Completions need no data home, runtime or engine.
    if let Command::Completions { shell } = command {
        use clap::CommandFactory;
        use std::io::Write;
        // Generate into memory first: writing straight to a closed pipe (for
        // example `verone completions bash | head`) would otherwise panic.
        let mut script = Vec::new();
        clap_complete::generate(*shell, &mut Cli::command(), "verone", &mut script);
        let _ = std::io::stdout().write_all(&script);
        return Ok(());
    }
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
        Command::Create {
            id,
            name,
            android,
            size,
            dpi,
            graphics,
            adb_port,
        } => {
            let options = phone::CreateOptions {
                id: id.clone(),
                name: name.clone(),
                android: android.clone(),
                size: size.clone(),
                dpi: *dpi,
                graphics: (*graphics).into(),
                adb_port: *adb_port,
            };
            phone::create(&context.env(), &options, &mut out)
        }
        Command::Start { id } => phone::start(&context.env(), id, &mut out),
        Command::Stop { id } => phone::stop(&context.env(), id, &mut out),
        Command::Restart { id } => phone::restart(&context.env(), id, &mut out),
        Command::List => phone::list(&context.env(), &mut out),
        Command::Info { id } => phone::info(&context.env(), id, &mut out),
        Command::Open { id, scrcpy_options } => {
            drop(out);
            phone::open(&context.env(), &context.tools(), id, scrcpy_options)
        }
        Command::Adb { id, arguments } => {
            drop(out);
            phone::adb(&context.env(), &context.tools(), id, arguments)
        }
        Command::Install { id, apk } => {
            phone::install(&context.env(), &context.tools(), id, apk, &mut out)
        }
        Command::Reset { id, yes } => phone::reset(
            &context.env(),
            id,
            *yes,
            &mut std::io::stdin().lock(),
            &mut out,
        ),
        Command::Delete { id, yes } => phone::delete(
            &context.env(),
            id,
            *yes,
            &mut std::io::stdin().lock(),
            &mut out,
        ),
        Command::Completions { .. } => Ok(()),
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
    fn create_options_are_read_from_the_command_line() {
        let cli = Cli::parse_from([
            "verone",
            "create",
            "work",
            "--name",
            "Work phone",
            "--size",
            "1080x2400",
            "--dpi",
            "420",
            "--graphics",
            "software",
            "--adb-port",
            "5599",
        ]);
        match cli.command {
            Some(Command::Create {
                id,
                name,
                android,
                size,
                dpi,
                graphics,
                adb_port,
            }) => {
                assert_eq!(id, "work");
                assert_eq!(name.as_deref(), Some("Work phone"));
                assert_eq!(android, "14");
                assert_eq!(size.as_deref(), Some("1080x2400"));
                assert_eq!(dpi, Some(420));
                assert_eq!(GraphicsMode::from(graphics), GraphicsMode::Software);
                assert_eq!(adb_port, 5599);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn arguments_after_two_dashes_are_passed_on_untouched() {
        let cli = Cli::parse_from(["verone", "adb", "work", "--", "shell", "ls", "-la"]);
        match cli.command {
            Some(Command::Adb { id, arguments }) => {
                assert_eq!(id, "work");
                assert_eq!(arguments, ["shell", "ls", "-la"]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
        let cli = Cli::parse_from(["verone", "open", "work", "--", "--no-window"]);
        match cli.command {
            Some(Command::Open { scrcpy_options, .. }) => {
                assert_eq!(scrcpy_options, ["--no-window"]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_command_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
