mod config;
mod daemon;
mod setup;
mod shell;

use clap::{Parser, Subcommand};
use config::{Paths, SottoConfig};

#[derive(Parser)]
#[command(name = "sotto", about = "generated commit messages")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Setup wizard
    Setup,

    /// File watcher daemon
    Daemon,

    /// Print cached commit message
    /// This is called by the shell widget
    Complete,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Setup => run_setup(),
        Command::Daemon => run_daemon(),
        Command::Complete => {
            run_complete();
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("sotto {e:#}");
        std::process::exit(1);
    }
}

fn run_setup() -> anyhow::Result<()> {
    let paths = Paths::resolve()?;
    setup::run(&paths)
}

fn run_daemon() -> anyhow::Result<()> {
    let paths = Paths::resolve()?;
    let config = SottoConfig::load(&paths)?;
    daemon::start(&config, &paths)
}

fn run_complete() {
    // we should never error out here
    let Ok(paths) = Paths::resolve() else { return };
    shell::complete::run(&paths);
}
