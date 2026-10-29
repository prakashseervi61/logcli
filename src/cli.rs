use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "logcli",
    version,
    about = "Fast, local-first log viewer and processor for JSON and logfmt logs",
    disable_help_flag = true
)]
#[allow(dead_code)]
pub struct Args {
    /// Files to process (omit or use '-' for stdin)
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Print help
    #[arg(long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,

    /// Minimum level to display (trace, debug, info, warn, error, fatal)
    #[arg(short = 'l', long = "level", value_name = "LEVEL")]
    pub level: Option<String>,

    /// Show entries at or after this time
    #[arg(long = "since", value_name = "TIME", allow_hyphen_values = true)]
    pub since: Option<String>,

    /// Show entries at or before this time
    #[arg(long = "until", value_name = "TIME", allow_hyphen_values = true)]
    pub until: Option<String>,

    /// Filter by field value: key=value, key!=value, key~=text, key~~=regex
    #[arg(short = 'f', long = "filter", value_name = "EXPR")]
    pub filters: Vec<String>,

    /// Filter using a boolean query expression (repeatable; multiple are ANDed)
    #[arg(short = 'q', long = "query", value_name = "EXPR")]
    pub queries: Vec<String>,

    /// Input format: auto, json, logfmt, plain
    #[arg(long = "input-format", value_name = "FORMAT", default_value = "auto")]
    pub input_format: String,

    /// Allow non-JSON prefixes before JSON log entries
    #[arg(long = "allow-prefix")]
    pub allow_prefix: bool,

    /// Entry delimiter: auto, lf, crlf, cr, nul
    #[arg(long = "delimiter", value_name = "DELIM", default_value = "auto")]
    pub delimiter: String,

    /// Color output: auto, always, never
    #[arg(
        long = "color",
        value_name = "WHEN",
        num_args = 0..=1,
        default_missing_value = "always",
        default_value = "auto"
    )]
    pub color: String,

    /// Alias for --color=always
    #[arg(short = 'c')]
    pub color_always: bool,

    /// Disable color output
    #[arg(long = "no-color")]
    pub no_color: bool,

    /// Color theme (accepted; themes are not implemented yet)
    #[arg(long = "theme", value_name = "THEME", default_value = "uni")]
    pub theme: String,

    /// Hide fields; prefix with '!' to reveal, '*' for all
    #[arg(short = 'h', long = "hide", value_name = "KEY")]
    pub hide: Vec<String>,

    /// Timestamp format (strftime-style)
    #[arg(
        short = 't',
        long = "time-format",
        value_name = "FORMAT",
        default_value = "%Y-%m-%d %H:%M:%S%.3f"
    )]
    pub time_format: String,

    /// Output time zone (IANA name, e.g. Europe/Berlin)
    #[arg(short = 'Z', long = "time-zone", value_name = "TZ")]
    pub time_zone: Option<String>,

    /// Use the local time zone
    #[arg(short = 'L', long = "local")]
    pub local: bool,

    /// Hide null and empty fields
    #[arg(short = 'e', long = "hide-empty-fields")]
    pub hide_empty: bool,

    /// Show null and empty fields (overrides -e)
    #[arg(short = 'E', long = "show-empty-fields")]
    pub show_empty: bool,

    /// Field expansion: never, inline, auto, always
    #[arg(
        short = 'x',
        long = "expansion",
        value_name = "MODE",
        num_args = 0..=1,
        default_missing_value = "always",
        default_value = "auto"
    )]
    pub expansion: String,

    /// Write output to a file instead of stdout
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Output entry delimiter: newline, nul
    #[arg(
        long = "output-delimiter",
        value_name = "DELIM",
        default_value = "newline"
    )]
    pub output_delimiter: String,

    /// Output raw source entries instead of formatted entries
    #[arg(short = 'r', long = "raw")]
    pub raw: bool,

    /// Pager usage: auto, always, never (pager support is not implemented yet)
    #[arg(long = "paging", value_name = "WHEN", default_value = "auto")]
    pub paging: String,

    /// Alias for --paging=never
    #[arg(short = 'P')]
    pub no_paging: bool,

    /// Sort entries chronologically (in-memory for now)
    #[arg(short = 's', long = "sort")]
    pub sort: bool,

    /// Launch the interactive TUI dashboard
    #[arg(long = "tui")]
    pub tui: bool,

    /// Follow input files (not implemented yet in batch mode; used by TUI)
    #[arg(short = 'F', long = "follow")]
    pub follow: bool,

    /// Number of entries to preload in follow mode
    #[arg(long = "tail", value_name = "N", default_value_t = 10)]
    pub tail: usize,

    /// Number of processing threads (accepted; reserved)
    #[arg(short = 'C', long = "concurrency", value_name = "N")]
    pub concurrency: Option<usize>,

    /// Show diagnostics on stderr
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Suppress non-fatal diagnostics
    #[arg(long = "silent")]
    pub silent: bool,

    /// Maximum accepted entry size (e.g. 64MiB)
    #[arg(long = "max-entry-size", value_name = "SIZE", default_value = "64MiB")]
    pub max_entry_size: String,
}
