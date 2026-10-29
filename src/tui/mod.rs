mod app;

pub use app::TuiApp;

use crate::app::load_entries;
use crate::cli::Args;
use crate::error::Error;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

pub fn run(args: Args) -> Result<(), Error> {
    let entries = if args.files.is_empty() {
        // No files given: start with an empty TUI. The user can still filter,
        // but there's nothing to display until files are loaded (future feature).
        Vec::new()
    } else {
        load_entries(&args)?
    };

    let mut app = TuiApp::new(entries, args);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result.map_err(|err| Error::usage(format!("tui error: {err}")))
}
