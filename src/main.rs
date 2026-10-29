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

fn main() {
    let args = cli::Args::parse();

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
