use crate::cli::Args;
use crate::error::Error;
use crate::input;
use crate::model::{Entry, Level};
use crate::query::{parse, parse_filter};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Color theme for the TUI. Inspired by terminal log viewers (lnav, toolong, gonzo).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ColorTheme {
    pub header: Color,
    pub header_accent: Color,
    pub border: Color,
    pub border_active: Color,
    pub level_trace: Color,
    pub level_debug: Color,
    pub level_info: Color,
    pub level_warn: Color,
    pub level_error: Color,
    pub level_fatal: Color,
    pub level_panic: Color,
    pub text_dim: Color,
    pub text_bright: Color,
    pub status_ok: Color,
    pub status_warn: Color,
    pub status_error: Color,
    pub highlight: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            header: Color::Cyan,
            header_accent: Color::Magenta,
            border: Color::DarkGray,
            border_active: Color::Cyan,
            level_trace: Color::DarkGray,
            level_debug: Color::Cyan,
            level_info: Color::Green,
            level_warn: Color::Yellow,
            level_error: Color::Red,
            level_fatal: Color::LightRed,
            level_panic: Color::LightRed,
            text_dim: Color::DarkGray,
            text_bright: Color::White,
            status_ok: Color::Green,
            status_warn: Color::Yellow,
            status_error: Color::Red,
            highlight: Color::LightCyan,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Browsing,
    Filtering,
    Details,
    Help,
    FilePicker,
}

#[derive(Debug, Clone)]
pub struct FollowState {
    enabled: bool,
    file_paths: Vec<PathBuf>,
}

impl FollowState {
    pub fn new() -> Self {
        Self {
            enabled: false,
            file_paths: Vec::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    None,
    Time,
    Level,
}

pub struct TuiApp {
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    list_state: ListState,
    mode: TuiMode,
    filter_input: String,
    show_details: bool,
    details_scroll: u16,
    status_message: String,
    status_color: Color,
    follow: FollowState,
    sort_mode: SortMode,
    args: Args,
    file_picker_input: String,
    theme: ColorTheme,
}

impl TuiApp {
    pub fn new(entries: Vec<Entry>, args: Args) -> Self {
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }

        let mut filtered: Vec<usize> = (0..entries.len()).collect();
        TuiApp::sort_filtered(&mut filtered[..], &entries, SortMode::Time);

        let filtered_len = filtered.len();

        Self {
            entries,
            filtered,
            list_state,
            mode: TuiMode::Browsing,
            filter_input: String::new(),
            show_details: false,
            details_scroll: 0,
            status_message: format!("{} entries loaded", filtered_len),
            status_color: Color::Green,
            follow: FollowState::new(),
            sort_mode: SortMode::Time,
            args,
            file_picker_input: String::new(),
            theme: ColorTheme::default(),
        }
    }

    fn sort_filtered(filtered: &mut [usize], entries: &[Entry], mode: SortMode) {
        match mode {
            SortMode::Time => {
                filtered.sort_by(
                    |&a, &b| match (&entries[a].timestamp, &entries[b].timestamp) {
                        (Some(ta), Some(tb)) => ta.cmp(tb),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    },
                );
            }
            SortMode::Level => {
                filtered.sort_by(|&a, &b| entries[b].level.cmp(&entries[a].level));
            }
            SortMode::None => {}
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<impl ratatui::backend::Backend>,
    ) -> io::Result<()> {
        // Set up file watcher if follow mode is requested
        if self.args.follow && !self.args.files.is_empty() {
            self.follow.enabled = true;
            self.follow.file_paths = self.args.files.clone();
        }

        let mut last_tick = Instant::now();
        let tick_rate = Duration::from_millis(200);

        loop {
            let now = Instant::now();
            if now.duration_since(last_tick) >= tick_rate {
                self.handle_tick()?;
                last_tick = now;
            }

            if event::poll(tick_rate)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    match self.mode {
                        TuiMode::Browsing => {
                            if self.handle_browsing(key.code, key.modifiers)? {
                                return Ok(());
                            }
                        }
                        TuiMode::Filtering => {
                            if self.handle_filtering(key.code)? {
                                return Ok(());
                            }
                        }
                        TuiMode::Details => {
                            if self.handle_details(key.code)? {
                                return Ok(());
                            }
                        }
                        TuiMode::Help => {
                            if self.handle_help(key.code)? {
                                return Ok(());
                            }
                        }
                        TuiMode::FilePicker => {
                            if self.handle_file_picker(key.code)? {
                                return Ok(());
                            }
                        }
                    }
                }
            }

            terminal.draw(|f| self.draw(f))?;
        }
    }

    fn handle_tick(&mut self) -> io::Result<()> {
        if !self.follow.is_enabled() {
            return Ok(());
        }

        let mut new_entries = Vec::new();

        for path in &self.follow.file_paths {
            if let Ok(file) = File::open(path) {
                let reader = io::BufReader::new(file);
                let delimiter = b'\n';

                for bytes in reader.split(delimiter).filter_map(|r| r.ok()) {
                    if bytes.is_empty() {
                        continue;
                    }
                    let line_str = String::from_utf8_lossy(&bytes);
                    if line_str.trim().is_empty() {
                        continue;
                    }
                    if let Some(entry) = crate::parser::parse_entry(
                        &line_str,
                        &path.display().to_string(),
                        0,
                        &self.args.input_format,
                        self.args.allow_prefix,
                    ) {
                        let already_exists = self.entries.iter().any(|e| e.raw == entry.raw);
                        if !already_exists {
                            new_entries.push(entry);
                        }
                    }
                }
            }
        }

        if !new_entries.is_empty() {
            let old_len = self.entries.len();
            let new_count = new_entries.len();
            self.entries.extend(new_entries);

            // Rebuild filtered list from scratch with all entries
            self.rebuild_filtered();

            // Auto-scroll to bottom if we were at the bottom before
            if let Some(sel) = self.list_state.selected() {
                if sel >= old_len.saturating_sub(1) {
                    self.list_state
                        .select(Some(self.filtered.len().saturating_sub(1)));
                }
            }

            self.status_message = format!(
                "{} entries loaded (+{} new, follow: ON)",
                self.entries.len(),
                new_count
            );
            self.status_color = self.theme.status_ok;
        }

        Ok(())
    }

    fn rebuild_filtered(&mut self) {
        let input = self.filter_input.trim();
        self.filtered = if input.is_empty() {
            (0..self.entries.len()).collect()
        } else {
            match self.try_parse_query(input) {
                Ok(expr) => self
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, entry)| if expr.matches(entry) { Some(i) } else { None })
                    .collect(),
                Err(_) => (0..self.entries.len()).collect(),
            }
        };
        Self::sort_filtered(&mut self.filtered[..], &self.entries, self.sort_mode);
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    fn handle_browsing(&mut self, code: KeyCode, modifiers: KeyModifiers) -> io::Result<bool> {
        match code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
            KeyCode::Char('G') => self.scroll_to_bottom(),
            KeyCode::Char('g') => self.scroll_to_top(),
            KeyCode::Char('/') => {
                self.mode = TuiMode::Filtering;
                self.filter_input.clear();
            }
            KeyCode::Char('f') => {
                self.mode = TuiMode::Filtering;
                self.filter_input.clear();
            }
            KeyCode::Char('d') | KeyCode::Enter => self.open_details(),
            KeyCode::Char('F') => {
                self.toggle_follow();
            }
            KeyCode::Char('?') => {
                self.mode = TuiMode::Help;
            }
            KeyCode::Char('o') => {
                self.mode = TuiMode::FilePicker;
                self.file_picker_input.clear();
                self.status_message = "Enter file path to add:".to_string();
            }
            KeyCode::Char('s') => {
                self.cycle_sort();
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    fn cycle_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::None => SortMode::Time,
            SortMode::Time => SortMode::Level,
            SortMode::Level => SortMode::None,
        };
        self.rebuild_filtered();
        let sort_name = match self.sort_mode {
            SortMode::None => "none",
            SortMode::Time => "time",
            SortMode::Level => "level",
        };
        self.status_message = format!("sort: {}", sort_name);
        self.status_color = self.theme.text_dim;
    }

    fn handle_filtering(&mut self, code: KeyCode) -> io::Result<bool> {
        match code {
            KeyCode::Enter => {
                self.rebuild_filtered();
                self.mode = TuiMode::Browsing;
            }
            KeyCode::Esc => {
                self.mode = TuiMode::Browsing;
                self.filter_input.clear();
                self.rebuild_filtered();
            }
            KeyCode::Backspace => {
                self.filter_input.pop();
            }
            KeyCode::Char(c) => {
                self.filter_input.push(c);
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_details(&mut self, code: KeyCode) -> io::Result<bool> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('d') => {
                self.show_details = false;
                self.mode = TuiMode::Browsing;
                self.details_scroll = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => self.scroll_details_down(),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_details_up(),
            KeyCode::Char('G') => self.scroll_details_to_bottom(),
            KeyCode::Char('g') => self.scroll_details_to_top(),
            _ => {}
        }
        Ok(false)
    }

    fn handle_help(&mut self, code: KeyCode) -> io::Result<bool> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                self.mode = TuiMode::Browsing;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_file_picker(&mut self, code: KeyCode) -> io::Result<bool> {
        match code {
            KeyCode::Enter => {
                let path_str = self.file_picker_input.trim().to_string();
                if !path_str.is_empty() {
                    if let Err(err) = self.add_file(&path_str) {
                        self.status_message = format!("error adding file: {err}");
                    }
                }
                self.mode = TuiMode::Browsing;
            }
            KeyCode::Esc => {
                self.mode = TuiMode::Browsing;
                self.file_picker_input.clear();
            }
            KeyCode::Backspace => {
                self.file_picker_input.pop();
            }
            KeyCode::Char(c) => {
                self.file_picker_input.push(c);
            }
            _ => {}
        }
        Ok(false)
    }

    fn add_file(&mut self, path_str: &str) -> Result<(), Error> {
        let path = Path::new(path_str);
        if !path.exists() {
            self.status_message = format!("file not found: {}", path_str);
            self.status_color = self.theme.status_error;
            return Ok(());
        }

        // Check if already in follow list
        if self.follow.file_paths.iter().any(|p| p == path) {
            self.status_message = format!("already watching: {}", path_str);
            self.status_color = self.theme.status_warn;
            return Ok(());
        }

        // Load entries from the file
        let sources = input::open_sources(&[path.to_path_buf()])?;
        if let Some(mut source) = sources.into_iter().next() {
            let delimiter = b'\n';
            let mut buffer = Vec::new();
            let mut line_number = 0u64;

            loop {
                buffer.clear();
                let read = source.reader.read_until(delimiter, &mut buffer);
                match read {
                    Ok(0) => break,
                    Ok(_) => {
                        line_number += 1;
                        if buffer.last() == Some(&delimiter) {
                            buffer.pop();
                        }
                        if delimiter != b'\r' && buffer.last() == Some(&b'\r') {
                            buffer.pop();
                        }
                        if buffer.is_empty() {
                            continue;
                        }
                        let raw = String::from_utf8_lossy(&buffer);
                        if let Some(entry) = crate::parser::parse_entry(
                            &raw,
                            &source.name,
                            line_number,
                            &self.args.input_format,
                            self.args.allow_prefix,
                        ) {
                            self.entries.push(entry);
                        }
                    }
                    Err(_) => break,
                }
            }
        }

        self.rebuild_filtered();
        self.follow.file_paths.push(path.to_path_buf());
        self.status_message = format!("added {} ({} entries total)", path_str, self.entries.len());
        self.status_color = self.theme.status_ok;
        Ok(())
    }

    fn toggle_follow(&mut self) {
        if self.follow.file_paths.is_empty() {
            self.status_message = "no files to follow".to_string();
            self.status_color = self.theme.status_warn;
            return;
        }
        self.follow.enabled = !self.follow.enabled;
        if self.follow.enabled {
            self.status_message = "follow mode: ON (watch for new lines)".to_string();
            self.status_color = self.theme.status_ok;
        } else {
            self.status_message = "follow mode: OFF".to_string();
            self.status_color = self.theme.status_warn;
        }
    }

    fn scroll_down(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered.len().saturating_sub(1) {
                    i
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn scroll_up(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn scroll_to_top(&mut self) {
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn scroll_to_bottom(&mut self) {
        if !self.filtered.is_empty() {
            self.list_state.select(Some(self.filtered.len() - 1));
        }
    }

    fn open_details(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if let Some(&_entry_idx) = self.filtered.get(i) {
                self.show_details = true;
                self.mode = TuiMode::Details;
                self.details_scroll = 0;
                self.status_message = format!("entry {} of {}", i + 1, self.filtered.len());
            }
        }
    }

    fn try_parse_query(&self, input: &str) -> Result<crate::query::Expr, String> {
        parse(input)
            .or_else(|_| parse_filter(input))
            .map_err(|e| e.to_string())
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered.get(i).and_then(|&idx| self.entries.get(idx)))
    }

    fn scroll_details_down(&mut self) {
        self.details_scroll = self.details_scroll.saturating_add(1);
    }

    fn scroll_details_up(&mut self) {
        self.details_scroll = self.details_scroll.saturating_sub(1);
    }

    fn scroll_details_to_bottom(&mut self) {
        self.details_scroll = u16::MAX;
    }

    fn scroll_details_to_top(&mut self) {
        self.details_scroll = 0;
    }

    fn draw(&self, f: &mut Frame) {
        let theme = self.theme;
        let size = f.size();
        let header_height = if size.height > 10 { 7 } else { 4 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height), // header with banner
                Constraint::Min(0),                // body
                Constraint::Length(3),             // filter bar
            ])
            .split(size);

        // Header with logcli banner
        let follow_indicator = if self.follow.is_enabled() {
            " [FOLLOW ON]"
        } else {
            ""
        };
        let sort_indicator = match self.sort_mode {
            SortMode::Time => " [sort: time]",
            SortMode::Level => " [sort: level]",
            SortMode::None => " [sort: none]",
        };

        // Banner lines - each on its own Line to prevent overlap
        let banner_lines: Vec<ratatui::text::Line> = logcli_banner()
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let color = if i % 2 == 0 {
                    theme.header
                } else {
                    theme.header_accent
                };
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    line.to_string(),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ))
            })
            .collect();

        // Subtitle line - truncated to fit terminal width
        let subtitle = format!(
            "{} entries  {} files  {}",
            self.entries.len(),
            self.follow.file_paths.len(),
            if self.mode == TuiMode::Filtering {
                "[FILTERING]"
            } else {
                ""
            }
        );
        let help_hint = "(q=quit, /=filter, o=add file, s=sort, d=details, F=follow, ?=help)";
        let header_line = format!(
            "logcli TUI{}{}  {}  {}",
            follow_indicator, sort_indicator, subtitle, help_hint
        );
        // Truncate header line to fit within terminal width minus borders
        let max_width = size.width.saturating_sub(4);
        let header_line = if header_line.len() > max_width as usize {
            let keep = max_width.saturating_sub(3);
            format!("{}...", &header_line[..keep as usize])
        } else {
            header_line
        };

        let header_text: Vec<ratatui::text::Line> = std::iter::once(ratatui::text::Line::from(""))
            .chain(banner_lines)
            .chain(std::iter::once(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled(
                    header_line,
                    Style::default()
                        .fg(theme.text_bright)
                        .add_modifier(Modifier::BOLD),
                ),
            ])))
            .collect();

        let header = Paragraph::new(header_text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_active))
                .title(ratatui::text::Span::styled(
                    " logcli ",
                    Style::default()
                        .fg(theme.header_accent)
                        .add_modifier(Modifier::BOLD),
                )),
        );
        f.render_widget(header, chunks[0]);

        // Body: split into list and details
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[1]);

        // List of entries
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .map(|&idx| {
                let entry = &self.entries[idx];
                let level_color = level_color(entry.level.unwrap_or(Level::Info));
                let level_str = entry
                    .level
                    .map(|l| l.label().to_string())
                    .unwrap_or_else(|| "-".to_string());
                let time_str = entry
                    .timestamp
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
                    .unwrap_or_else(|| "-".to_string());
                let source = Path::new(&entry.source)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&entry.source);
                let message = entry.message.as_deref().unwrap_or("");
                let content = format!(
                    "{}  {:<5}  {:<20}  {}",
                    time_str, level_str, source, message
                );
                ListItem::new(content).style(Style::default().fg(level_color).bg(Color::Reset))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(ratatui::text::Span::styled(
                        " entries ",
                        Style::default().fg(theme.header),
                    )),
            )
            .highlight_style(
                Style::default()
                    .bg(theme.highlight)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, body_chunks[0], &mut self.list_state.clone());

        // Details panel
        if self.show_details {
            if let Some(entry) = self.selected_entry() {
                let details = format_entry_details(entry);
                let lines: Vec<&str> = details.lines().collect();
                let start = self.details_scroll as usize;
                let visible: Vec<&str> = lines.into_iter().skip(start).collect();
                let details_text = visible.join("\n");
                let details_paragraph = Paragraph::new(details_text).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title(ratatui::text::Span::styled(
                            " details ",
                            Style::default().fg(theme.header),
                        )),
                );
                f.render_widget(details_paragraph, body_chunks[1]);
            }
        } else {
            let empty = Paragraph::new("press 'd' or Enter to view details").block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border)),
            );
            f.render_widget(empty, body_chunks[1]);
        }

        // Filter / status bar
        let (bar_text, bar_color) = match self.mode {
            TuiMode::Filtering => (format!("/{}", self.filter_input), theme.text_bright),
            TuiMode::Help => (help_text(), theme.text_bright),
            TuiMode::FilePicker => (
                format!("add file: {}", self.file_picker_input),
                theme.text_bright,
            ),
            _ => (self.status_message.clone(), self.status_color),
        };
        let bar = Paragraph::new(ratatui::text::Span::styled(
            bar_text,
            Style::default().fg(bar_color),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_active))
                .title(ratatui::text::Span::styled(
                    " filter / status ",
                    Style::default().fg(theme.header),
                )),
        );
        f.render_widget(bar, chunks[2]);
    }
}

fn level_color(level: Level) -> Color {
    match level {
        Level::Trace => Color::DarkGray,
        Level::Debug => Color::Cyan,
        Level::Info => Color::Green,
        Level::Warn => Color::Yellow,
        Level::Error => Color::Red,
        Level::Fatal => Color::LightRed,
        Level::Panic => Color::LightRed,
    }
}

/// Simple "logcli" text banner that renders in any terminal.
fn logcli_banner() -> Vec<&'static str> {
    vec![
        "  ____        _     _       ",
        " / ___| _ __ (_) __| | ___  ",
        "| |  _ | '_ \\| |/ _` |/ _ \\ ",
        "| |_| || |_) | | (_| |  __/ ",
        " \\____|| .__/|_|\\__,_|\\___| ",
        "     |_|                    ",
    ]
}

fn format_entry_details(entry: &Entry) -> String {
    let mut lines = Vec::new();
    lines.push(format!("source: {}", entry.source));
    lines.push(format!("line: {}", entry.line));
    lines.push(format!("format: {:?}", entry.format));

    if let Some(ts) = entry.timestamp {
        lines.push(format!("timestamp: {}", ts));
    }
    if let Some(level) = entry.level {
        lines.push(format!("level: {:?} ({})", level, level.label()));
    }
    if let Some(level_raw) = &entry.level_raw {
        lines.push(format!("level_raw: {}", level_raw));
    }
    if let Some(message) = &entry.message {
        lines.push(format!("message: {}", message));
    }
    if let Some(logger) = &entry.logger {
        lines.push(format!("logger: {}", logger));
    }
    if let Some(caller) = &entry.caller {
        lines.push(format!("caller: {}", caller));
    }

    lines.push(String::new());
    lines.push("fields:".to_string());
    if entry.fields.is_empty() {
        lines.push("  (none)".to_string());
    } else {
        for (key, value) in &entry.fields {
            lines.push(format!("  {}: {}", key, value));
        }
    }

    lines.push(String::new());
    lines.push("raw:".to_string());
    lines.push(entry.raw.clone());

    lines.join("\n")
}

fn help_text() -> String {
    let lines = vec![
        "logcli TUI — Key Bindings".to_string(),
        String::new(),
        "  q        Quit".to_string(),
        "  j/k      Scroll up/down".to_string(),
        "  g/G      Go to top/bottom".to_string(),
        "  / or f   Open filter prompt".to_string(),
        "  d/Enter  Open details view".to_string(),
        "  o        Add a log file".to_string(),
        "  s        Cycle sort (none/time/level)".to_string(),
        "  F        Toggle follow mode".to_string(),
        "  ?        Show this help".to_string(),
        "  Ctrl-C   Force quit".to_string(),
        String::new(),
        "In details view:".to_string(),
        "  j/k      Scroll details".to_string(),
        "  g/G      Go to top/bottom of details".to_string(),
        "  q/Esc/d  Close details".to_string(),
        String::new(),
        "Filter syntax (QUERY_SPEC):".to_string(),
        "  level = error, status >= 500".to_string(),
        "  message ~= \"connection refused\"".to_string(),
        "  exists(request.id)".to_string(),
        "  status in (200, 201, 204)".to_string(),
    ];
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Format, Level};
    use chrono::{TimeZone, Utc};
    use clap::Parser;
    use serde_json::{json, Map, Value};

    fn make_entry(level: Level, message: &str, fields: Map<String, Value>) -> Entry {
        Entry {
            source: "test.log".to_string(),
            line: 1,
            timestamp: Some(Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap()),
            level: Some(level),
            level_raw: Some(level.label().to_string()),
            message: Some(message.to_string()),
            logger: None,
            caller: None,
            fields,
            consumed: Vec::new(),
            raw: format!("{} {}", level, message),
            format: Format::Json,
        }
    }

    fn make_entries() -> Vec<Entry> {
        let mut fields1 = Map::new();
        fields1.insert("status".to_string(), json!(500));
        let e1 = make_entry(Level::Error, "connection refused", fields1);

        let mut fields2 = Map::new();
        fields2.insert("status".to_string(), json!(200));
        let e2 = make_entry(Level::Info, "server started", fields2);

        let mut fields3 = Map::new();
        fields3.insert("status".to_string(), json!(503));
        let e3 = make_entry(Level::Error, "request timeout", fields3);

        vec![e1, e2, e3]
    }

    fn make_args() -> crate::cli::Args {
        crate::cli::Args::parse_from(["logcli"])
    }

    #[test]
    fn new_app_selects_first_entry() {
        let entries = make_entries();
        let app = TuiApp::new(entries.clone(), make_args());
        assert_eq!(app.list_state.selected(), Some(0));
        assert_eq!(app.filtered.len(), 3);
        assert_eq!(app.mode, TuiMode::Browsing);
    }

    #[test]
    fn new_app_with_no_entries_has_no_selection() {
        let app = TuiApp::new(vec![], make_args());
        assert_eq!(app.list_state.selected(), None);
        assert_eq!(app.filtered.len(), 0);
    }

    #[test]
    fn scroll_down_moves_selection() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args());
        app.scroll_down();
        assert_eq!(app.list_state.selected(), Some(1));
        app.scroll_down();
        assert_eq!(app.list_state.selected(), Some(2));
        app.scroll_down();
        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn scroll_up_moves_selection() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args());
        app.scroll_to_bottom();
        assert_eq!(app.list_state.selected(), Some(2));
        app.scroll_up();
        assert_eq!(app.list_state.selected(), Some(1));
        app.scroll_up();
        assert_eq!(app.list_state.selected(), Some(0));
        app.scroll_up();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn apply_filter_matches_entries() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args());
        app.filter_input = "level = error".to_string();
        app.rebuild_filtered();
        assert_eq!(app.mode, TuiMode::Browsing);
        assert_eq!(app.filtered.len(), 2);
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn apply_filter_no_matches() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args());
        app.filter_input = "level = panic".to_string();
        app.rebuild_filtered();
        assert_eq!(app.filtered.len(), 0);
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn cycle_sort_changes_mode() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args());
        assert_eq!(app.sort_mode, SortMode::Time);
        app.cycle_sort();
        assert_eq!(app.sort_mode, SortMode::Level);
        app.cycle_sort();
        assert_eq!(app.sort_mode, SortMode::None);
        app.cycle_sort();
        assert_eq!(app.sort_mode, SortMode::Time);
    }

    #[test]
    fn toggle_follow_no_files() {
        let entries = make_entries();
        let args = make_args();
        let mut app = TuiApp::new(entries, args);
        assert!(!app.follow.is_enabled());
        app.toggle_follow();
        assert!(!app.follow.is_enabled()); // no files, so stays off
    }

    #[test]
    fn help_text_contains_key_bindings() {
        let help = help_text();
        assert!(help.contains("q"));
        assert!(help.contains("Quit"));
        assert!(help.contains("filter"));
        assert!(help.contains("follow"));
        assert!(help.to_lowercase().contains("add a log file"));
    }

    #[test]
    fn level_color_mapping() {
        assert_eq!(level_color(Level::Error), Color::Red);
        assert_eq!(level_color(Level::Info), Color::Green);
        assert_eq!(level_color(Level::Warn), Color::Yellow);
        assert_eq!(level_color(Level::Debug), Color::Cyan);
        assert_eq!(level_color(Level::Fatal), Color::LightRed);
        assert_eq!(level_color(Level::Panic), Color::LightRed);
    }

    #[test]
    fn sort_filtered_by_time() {
        let mut entries = make_entries();
        // Give different timestamps
        entries[0].timestamp = Some(Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 2).unwrap());
        entries[1].timestamp = Some(Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap());
        entries[2].timestamp = Some(Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 1).unwrap());

        let mut filtered = vec![0, 1, 2];
        TuiApp::sort_filtered(&mut filtered[..], &entries, SortMode::Time);
        // Should be sorted: entries[1] (10:00:00), entries[2] (10:00:01), entries[0] (10:00:02)
        assert_eq!(filtered, vec![1, 2, 0]);
    }
}
