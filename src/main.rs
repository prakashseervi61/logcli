mod app;
mod cli;
mod error;
mod filter;
mod input;
mod model;
mod output;
mod parser;
mod query;
mod tui;

use clap::Parser;
use std::io::IsTerminal;

fn main() {
    let mut args = cli::Args::parse();

    // Auto-launch the interactive TUI when no files are given and stdin is a TTY.
    // When stdin is piped, batch mode reads from stdin as before.
    if !args.tui && args.files.is_empty() && std::io::stdin().is_terminal() {
        args.tui = true;
    }

    match app::run(args) {
        Ok(()) => {}
        Err(error::Error::BrokenPipe) => std::process::exit(3),
        Err(error::Error::Interrupted) => std::process::exit(130),
        Err(err) => {
            eprintln!("logcli: error: {err}");
            std::process::exit(err.exit_code());
        }
    }
}
