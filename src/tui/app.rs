use crate::cli::Args;
use crate::config::Config;
use crate::error::Error;
use crate::input;
use crate::model::{Entry, Level};
use crate::query::{parse, parse_filter};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame, Terminal,
};
use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const HEADER_COLOR: Color = Color::Cyan;
const HEADER_ACCENT: Color = Color::Magenta;
const BORDER_ACTIVE: Color = Color::Cyan;
const LEVEL_TRACE: Color = Color::DarkGray;
const LEVEL_DEBUG: Color = Color::Cyan;
const LEVEL_INFO: Color = Color::Green;
const LEVEL_WARN: Color = Color::Yellow;
const LEVEL_ERROR: Color = Color::Red;
const LEVEL_FATAL: Color = Color::LightRed;
const LEVEL_PANIC: Color = Color::LightRed;
const TEXT_DIM: Color = Color::DarkGray;
const TEXT_BRIGHT: Color = Color::White;
const STATUS_OK: Color = Color::Green;
const STATUS_WARN: Color = Color::Yellow;
const STATUS_ERROR: Color = Color::Red;
const KEY_COLOR: Color = Color::Green;
const HINT_COLOR: Color = Color::Gray;
const VALUE_STRING: Color = Color::Cyan;
const VALUE_NUMBER: Color = Color::Yellow;
const VALUE_BOOL: Color = Color::Magenta;
const SELECTED_BG: Color = Color::Indexed(237);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Browsing,
    Filtering,
    Details,
    Help,
    FilePicker,
    Histogram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Time,
    Level,
}

pub struct TuiApp {
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    list_state: TableState,
    mode: TuiMode,
    filter_input: String,
    show_details: bool,
    details_scroll: u16,
    status_message: String,
    status_color: Color,
    follow_enabled: bool,
    follow_paths: Vec<PathBuf>,
    sort_mode: Option<SortMode>,
    args: Args,
    file_picker_input: String,
    config: Option<Config>,
}

impl TuiApp {
    pub fn new(entries: Vec<Entry>, args: Args, config: Option<Config>) -> Self {
        let mut list_state = TableState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }

        let mut filtered: Vec<usize> = (0..entries.len()).collect();
        let initial_sort = config
            .as_ref()
            .and_then(|c| c.tui.as_ref())
            .and_then(|t| t.sort.as_deref())
            .unwrap_or("time");
        let sort_mode = match initial_sort {
            "time" => Some(SortMode::Time),
            "level" => Some(SortMode::Level),
            _ => None,
        };
        Self::sort_filtered(&mut filtered[..], &entries, sort_mode);

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
            status_color: STATUS_OK,
            follow_enabled: config
                .as_ref()
                .and_then(|c| c.tui.as_ref())
                .and_then(|t| t.follow)
                .unwrap_or(false),
            follow_paths: args.files.clone(),
            sort_mode,
            args,
            file_picker_input: String::new(),
            config,
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<impl ratatui::backend::Backend>,
    ) -> io::Result<()> {
        // Set up file watcher if follow mode is requested (CLI flag or config).
        let config_follow = self
            .config
            .as_ref()
            .and_then(|c| c.tui.as_ref())
            .and_then(|t| t.follow)
            .unwrap_or(false);
        if (self.args.follow || config_follow) && !self.args.files.is_empty() {
            self.follow_enabled = true;
            self.follow_paths = self.args.files.clone();
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
                        TuiMode::Histogram => {
                            if key.code == KeyCode::Char('h') {
                                self.mode = TuiMode::Browsing;
                            }
                        }
                    }
                }
            }

            terminal.draw(|f| self.draw(f))?;
        }
    }

    fn handle_tick(&mut self) -> io::Result<()> {
        if !self.follow_enabled {
            return Ok(());
        }

        let mut new_entries = Vec::new();

        for path in &self.follow_paths {
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
            self.status_color = STATUS_OK;
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

    fn sort_filtered(filtered: &mut [usize], entries: &[Entry], mode: Option<SortMode>) {
        match mode {
            Some(SortMode::Time) => {
                filtered.sort_by(
                    |&a, &b| match (&entries[a].timestamp, &entries[b].timestamp) {
                        (Some(ta), Some(tb)) => ta.cmp(tb),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    },
                );
            }
            Some(SortMode::Level) => {
                filtered.sort_by(|&a, &b| entries[b].level.cmp(&entries[a].level));
            }
            None => {}
        }
    }

    /// Compute histogram data: count entries per time bucket per level.
    /// Returns (bucket_labels, level_counts) where level_counts maps level -> per-bucket counts.
    fn histogram_data(&self) -> (Vec<String>, Vec<(Level, Vec<u64>)>) {
        if self.filtered.is_empty() {
            return (vec![], vec![]);
        }

        // Collect timestamps that have values
        let timestamps: Vec<Option<chrono::DateTime<chrono::Utc>>> = self
            .filtered
            .iter()
            .map(|&idx| self.entries[idx].timestamp)
            .collect();

        let with_ts: Vec<_> = timestamps.iter().filter_map(|t| *t).collect();
        if with_ts.is_empty() {
            return (vec![], vec![]);
        }

        let min_ts = with_ts.iter().min().unwrap();
        let max_ts = with_ts.iter().max().unwrap();
        let num_buckets = 10usize.min(with_ts.len());
        let range_secs = (max_ts.timestamp() - min_ts.timestamp()).max(1) as f64;
        let bucket_secs = range_secs / num_buckets as f64;

        // Initialize bucket counts per level
        let levels = [
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Warn,
            Level::Error,
            Level::Fatal,
            Level::Panic,
        ];
        let mut level_counts: Vec<(Level, Vec<u64>)> = levels
            .iter()
            .copied()
            .map(|l| (l, vec![0u64; num_buckets]))
            .collect();

        for (i, &ts) in timestamps.iter().enumerate() {
            if let Some(ts) = ts {
                let entry_level = self.entries[self.filtered[i]].level.unwrap_or(Level::Info);
                let diff_secs = (ts.timestamp() - min_ts.timestamp()) as f64;
                let bucket = (diff_secs / bucket_secs).floor() as usize;
                let bucket = bucket.min(num_buckets - 1);
                if let Some((_, counts)) = level_counts.iter_mut().find(|(l, _)| *l == entry_level)
                {
                    counts[bucket] += 1;
                }
            }
        }

        // Bucket labels
        let mut labels = Vec::with_capacity(num_buckets);
        for b in 0..num_buckets {
            let ts = (*min_ts) + chrono::Duration::seconds((b as f64 * bucket_secs) as i64);
            labels.push(ts.format("%H:%M").to_string());
        }

        (labels, level_counts)
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
                self.toggle_follow();
            }
            KeyCode::Esc => {
                if !self.filter_input.is_empty() {
                    self.filter_input.clear();
                    self.rebuild_filtered();
                    self.status_message = "filter cleared".to_string();
                    self.status_color = TEXT_DIM;
                }
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
            KeyCode::Char('h') => {
                if self.mode == TuiMode::Histogram {
                    self.mode = TuiMode::Browsing;
                } else {
                    self.mode = TuiMode::Histogram;
                }
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
            None => Some(SortMode::Time),
            Some(SortMode::Time) => Some(SortMode::Level),
            Some(SortMode::Level) => None,
        };
        self.rebuild_filtered();
        let sort_name = match self.sort_mode {
            None => "none",
            Some(SortMode::Time) => "time",
            Some(SortMode::Level) => "level",
        };
        self.status_message = format!("sort: {}", sort_name);
        self.status_color = TEXT_DIM;
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
            self.status_color = STATUS_ERROR;
            return Ok(());
        }

        // Check if already in follow list
        if self.follow_paths.iter().any(|p| p == path) {
            self.status_message = format!("already watching: {}", path_str);
            self.status_color = STATUS_WARN;
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
        self.follow_paths.push(path.to_path_buf());
        self.status_message = format!("added {} ({} entries total)", path_str, self.entries.len());
        self.status_color = STATUS_OK;
        Ok(())
    }

    fn toggle_follow(&mut self) {
        if self.follow_paths.is_empty() {
            self.status_message = "no files to follow".to_string();
            self.status_color = STATUS_WARN;
            return;
        }
        self.follow_enabled = !self.follow_enabled;
        if self.follow_enabled {
            self.status_message = "follow mode: ON (watch for new lines)".to_string();
            self.status_color = STATUS_OK;
        } else {
            self.status_message = "follow mode: OFF".to_string();
            self.status_color = STATUS_WARN;
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

    /// Draw the histogram view: stacked bar chart of entries by level over time.
    fn draw_histogram(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let (labels, level_counts) = self.histogram_data();

        if labels.is_empty() {
            let empty = ratatui::widgets::Paragraph::new("no entries with timestamps").block(
                ratatui::widgets::Block::default()
                    .borders(ratatui::widgets::Borders::ALL)
                    .title(ratatui::text::Span::styled(
                        " histogram ",
                        ratatui::style::Style::default().fg(HEADER_COLOR),
                    )),
            );
            f.render_widget(empty, area);
            return;
        }

        let total_count: u64 = level_counts
            .iter()
            .map(|(_, c)| c.iter().sum::<u64>())
            .sum();
        let max_total: u64 = labels
            .iter()
            .enumerate()
            .map(|(i, _)| level_counts.iter().map(|(_, c)| c[i]).sum::<u64>())
            .max()
            .unwrap_or(1);
        let chart_height = area.height.saturating_sub(2) as usize;

        // Build rows from bottom to top
        let mut rows: Vec<String> = Vec::with_capacity(chart_height);
        for row in (0..chart_height).rev() {
            let threshold = (max_total as f64 * row as f64 / chart_height as f64) as u64;
            let mut line = String::with_capacity(labels.len() * 2);
            for b in 0..labels.len() {
                let bucket_total: u64 = level_counts.iter().map(|(_, c)| c[b]).sum();
                if bucket_total >= threshold {
                    // Find the topmost level present in this bucket at this row
                    let mut ch = ' ';
                    for (l, counts) in &level_counts {
                        if counts[b] >= threshold {
                            ch = match l {
                                Level::Error | Level::Fatal | Level::Panic => '█',
                                Level::Warn => '▓',
                                Level::Info => '▒',
                                Level::Debug => '░',
                                Level::Trace => '▱',
                            };
                            break;
                        }
                    }
                    line.push(ch);
                } else {
                    line.push(' ');
                }
                if b < labels.len() - 1 {
                    line.push(' ');
                }
            }
            rows.push(line);
        }

        // Build X-axis labels
        let mut xaxis = String::with_capacity(labels.len() * 2);
        for (i, label) in labels.iter().enumerate() {
            if i > 0 {
                xaxis.push(' ');
            }
            xaxis.push_str(label);
        }

        // Legend
        let mut legend = String::new();
        for (l, _) in &level_counts {
            let sym = match l {
                Level::Error | Level::Fatal | Level::Panic => "█",
                Level::Warn => "▓",
                Level::Info => "▒",
                Level::Debug => "░",
                Level::Trace => "▱",
            };
            if !legend.is_empty() {
                legend.push_str("  ");
            }
            legend.push_str(&format!("{} {}", sym, l.label()));
        }

        let info = format!("total={} buckets={}", total_count, labels.len());

        let text: Vec<ratatui::text::Line> = rows
            .into_iter()
            .map(ratatui::text::Line::from)
            .chain(std::iter::once(ratatui::text::Line::from(xaxis)))
            .chain(std::iter::once(ratatui::text::Line::from(info)))
            .chain(std::iter::once(ratatui::text::Line::from(legend)))
            .collect();

        let hist = ratatui::widgets::Paragraph::new(text).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title(ratatui::text::Span::styled(
                    " histogram (press h to close) ",
                    ratatui::style::Style::default().fg(HEADER_COLOR),
                )),
        );
        f.render_widget(hist, area);
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

    fn draw(&mut self, f: &mut Frame) {
        let size = f.size();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(5),    // entries + details
                Constraint::Length(3), // filter / status
                Constraint::Length(1), // key hints
            ])
            .split(size);

        self.draw_header(f, chunks[0]);

        let body_width = chunks[1].width;
        let entries_width = (body_width.saturating_mul(68) / 100).max(20);
        let details_width = body_width
            .saturating_sub(entries_width)
            .saturating_sub(1)
            .max(10);
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(entries_width),
                Constraint::Length(1),
                Constraint::Length(details_width),
            ])
            .split(chunks[1]);

        match self.mode {
            TuiMode::Histogram => self.draw_histogram(f, chunks[1]),
            _ => {
                self.draw_entries(f, body_chunks[0]);
                self.draw_details(f, body_chunks[2]);
            }
        }

        // Filter box sits under the entries pane only, like the reference UI.
        let filter_area = Rect {
            width: entries_width.min(chunks[2].width),
            ..chunks[2]
        };
        self.draw_filter_bar(f, filter_area);
        self.draw_keybar(f, chunks[3]);

        match self.mode {
            TuiMode::Help => self.draw_help_popup(f, chunks[1]),
            TuiMode::FilePicker => self.draw_file_popup(f, chunks[1]),
            _ => {}
        }
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_ACTIVE));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let right_width = 44u16.min(inner.width.saturating_mul(2) / 3);
        let sections = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(18u16.min(inner.width)),
                Constraint::Min(0),
                Constraint::Length(right_width),
            ])
            .split(inner);

        let title = Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "logcli ",
                Style::default()
                    .fg(HEADER_ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(TEXT_BRIGHT),
            ),
        ]));
        f.render_widget(title, sections[0]);

        let source_line = Paragraph::new(Line::from(vec![
            Span::styled("[ Source: ", Style::default().fg(HEADER_COLOR)),
            Span::styled(self.source_label(), Style::default().fg(TEXT_BRIGHT)),
            Span::styled(" ]", Style::default().fg(HEADER_COLOR)),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(source_line, sections[1]);

        let (errors, warns, infos) = self.header_counts();
        let counts = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("[ ERRORS: {} ]", errors),
                Style::default().fg(LEVEL_ERROR),
            ),
            Span::raw(" "),
            Span::styled(
                format!("[ WARNs: {} ]", warns),
                Style::default().fg(LEVEL_WARN),
            ),
            Span::raw(" "),
            Span::styled(
                format!("[ INFO: {} ]", infos),
                Style::default().fg(LEVEL_INFO),
            ),
            Span::raw(" "),
        ]))
        .alignment(Alignment::Right);
        f.render_widget(counts, sections[2]);
    }

    fn draw_entries(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_ACTIVE))
            .title(Span::styled(
                format!(" entries [{}] ", self.entries.len()),
                Style::default().fg(HEADER_COLOR),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.filtered.is_empty() {
            let message = if self.filter_input.trim().is_empty() {
                "no entries loaded".to_string()
            } else {
                format!("no entries match \"{}\"", self.filter_input)
            };
            let empty = Paragraph::new(message)
                .style(Style::default().fg(TEXT_DIM))
                .alignment(Alignment::Center);
            f.render_widget(empty, inner);
            return;
        }

        let header = Row::new(vec![
            Cell::from(""),
            Cell::from(Span::styled(
                "TIMESTAMP",
                Style::default()
                    .fg(HEADER_ACCENT)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "LEVEL",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(Span::styled(
                "MESSAGE",
                Style::default()
                    .fg(HEADER_COLOR)
                    .add_modifier(Modifier::BOLD),
            )),
        ]);

        let selected = self.list_state.selected();
        let rows: Vec<Row> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(position, &index)| {
                let entry = &self.entries[index];
                let marker = if selected == Some(position) {
                    Span::styled(
                        ">",
                        Style::default().fg(KEY_COLOR).add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw(" ")
                };
                let time = entry
                    .timestamp
                    .map(|t| t.format("%H:%M:%S%.3f").to_string())
                    .unwrap_or_else(|| "--:--:--.---".to_string());
                let level = entry.level;
                let level_text = level.map(level_short).unwrap_or("-").to_string();
                let level_style = Style::default()
                    .fg(level_color(level.unwrap_or(Level::Trace)))
                    .add_modifier(Modifier::BOLD);
                let message = entry.message.as_deref().unwrap_or("").to_string();

                Row::new(vec![
                    Cell::from(marker),
                    Cell::from(Span::styled(time, Style::default().fg(TEXT_DIM))),
                    Cell::from(Span::styled(level_text, level_style)),
                    Cell::from(Span::styled(message, Style::default().fg(Color::Gray))),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Length(12),
                Constraint::Length(5),
                Constraint::Min(10),
            ],
        )
        .header(header)
        .column_spacing(1)
        .highlight_style(Style::default().bg(SELECTED_BG));

        f.render_stateful_widget(table, inner, &mut self.list_state);
    }

    fn draw_details(&self, f: &mut Frame, area: Rect) {
        let active = self.show_details || self.mode == TuiMode::Details;
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_ACTIVE))
            .title(Span::styled(" details ", Style::default().fg(HEADER_COLOR)));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.selected_entry().is_none() {
            let empty = Paragraph::new("no entry selected").style(Style::default().fg(TEXT_DIM));
            f.render_widget(empty, inner);
            return;
        }

        let lines = self.details_lines();
        let start = if active {
            self.details_scroll as usize
        } else {
            0
        };
        let visible: Vec<Line> = lines.into_iter().skip(start).collect();
        let details = Paragraph::new(visible).wrap(Wrap { trim: false });
        f.render_widget(details, inner);
    }

    fn draw_filter_bar(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_ACTIVE))
            .title(Span::styled(
                " filter / status ",
                Style::default().fg(HEADER_COLOR),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let filtering = self.mode == TuiMode::Filtering;
        let mut spans = vec![
            Span::styled("Filter: ", Style::default().fg(HINT_COLOR)),
            Span::styled("[ ", Style::default().fg(BORDER_ACTIVE)),
            Span::styled(self.filter_input.clone(), Style::default().fg(TEXT_BRIGHT)),
            Span::styled("]", Style::default().fg(BORDER_ACTIVE)),
        ];
        if !filtering && !self.status_message.is_empty() {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(
                self.status_message.clone(),
                Style::default().fg(self.status_color),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), inner);

        if filtering {
            let x = inner.x + 10 + self.filter_input.chars().count() as u16;
            f.set_cursor(x.min(inner.x + inner.width.saturating_sub(1)), inner.y);
        }
    }

    fn draw_keybar(&self, f: &mut Frame, area: Rect) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let matches_width = 17u16.min(area.width);
        let parts = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(matches_width)])
            .split(area);

        f.render_widget(Paragraph::new(self.keybar_line(area.width < 120)), parts[0]);

        let matches = Paragraph::new(Line::from(vec![
            Span::styled("Matches: ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                format!("{}/{}", self.filtered.len(), self.entries.len()),
                Style::default().fg(TEXT_BRIGHT),
            ),
        ]))
        .alignment(Alignment::Right);
        f.render_widget(matches, parts[1]);
    }

    fn keybar_line(&self, compact: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if compact {
            push_key(&mut spans, "↑/↓", "Nav");
            push_key(&mut spans, "d", "Details");
            push_key(&mut spans, "/", "Filter");
        } else {
            push_key(&mut spans, "↑/↓", "Navigate");
            push_key(&mut spans, "Enter/d", "Details");
            push_key(&mut spans, "/", "Search");
            push_key(&mut spans, "Esc", "Clear");
        }

        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "[f]",
            Style::default().fg(KEY_COLOR).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled("Follow ", Style::default().fg(HINT_COLOR)));
        if self.follow_enabled {
            spans.push(Span::styled("(ON)", Style::default().fg(KEY_COLOR)));
        } else {
            spans.push(Span::styled("(OFF)", Style::default().fg(TEXT_DIM)));
        }

        push_key(&mut spans, "?", "Help");
        push_key(&mut spans, "q", "Quit");
        Line::from(spans)
    }

    fn draw_help_popup(&self, f: &mut Frame, area: Rect) {
        let popup = centered_rect(64, 82, area);
        f.render_widget(Clear, popup);
        let help = Paragraph::new(help_text())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BORDER_ACTIVE))
                    .title(Span::styled(" help ", Style::default().fg(HEADER_COLOR))),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(help, popup);
    }

    fn draw_file_popup(&self, f: &mut Frame, area: Rect) {
        let mut popup = centered_rect(60, 20, area);
        popup.height = popup.height.max(3).min(area.height);
        f.render_widget(Clear, popup);
        let input = Paragraph::new(Line::from(vec![
            Span::styled("add file: ", Style::default().fg(HINT_COLOR)),
            Span::styled(
                self.file_picker_input.clone(),
                Style::default().fg(TEXT_BRIGHT),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER_ACTIVE))
                .title(Span::styled(
                    " add file ",
                    Style::default().fg(HEADER_COLOR),
                )),
        );
        f.render_widget(input, popup);
    }

    fn source_label(&self) -> String {
        let files = if self.follow_paths.is_empty() {
            &self.args.files
        } else {
            &self.follow_paths
        };
        if files.is_empty() {
            return "stdin".to_string();
        }
        let first = files[0]
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| files[0].to_str().unwrap_or("?"));
        if files.len() == 1 {
            first.to_string()
        } else {
            format!("{} +{}", first, files.len() - 1)
        }
    }

    fn header_counts(&self) -> (usize, usize, usize) {
        let mut errors = 0;
        let mut warns = 0;
        let mut infos = 0;
        for entry in &self.entries {
            match entry.level {
                Some(Level::Error | Level::Fatal | Level::Panic) => errors += 1,
                Some(Level::Warn) => warns += 1,
                Some(Level::Info) => infos += 1,
                _ => {}
            }
        }
        (errors, warns, infos)
    }

    fn details_lines(&self) -> Vec<Line<'static>> {
        let Some(entry) = self.selected_entry() else {
            return vec![Line::from("no entry selected")];
        };
        let value = entry_details_json(entry);
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| entry.raw.clone());
        let level = entry.level.map(level_color);
        pretty
            .lines()
            .map(|line| colorize_json_line(line, level))
            .collect()
    }
}

/// Short three/four-character level label used by the entries table.
fn level_short(level: Level) -> &'static str {
    match level {
        Level::Trace => "TRC",
        Level::Debug => "DBG",
        Level::Info => "INF",
        Level::Warn => "WRN",
        Level::Error => "ERR",
        Level::Fatal => "FTL",
        Level::Panic => "PNC",
    }
}

/// Append a `[key] description` pair to a key-hint line.
fn push_key(spans: &mut Vec<Span<'static>>, key: &str, description: &str) {
    if !spans.is_empty() {
        spans.push(Span::raw("  "));
    }
    spans.push(Span::styled(
        format!("[{}]", key),
        Style::default().fg(KEY_COLOR).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        description.to_string(),
        Style::default().fg(HINT_COLOR),
    ));
}

/// Center a rectangle within `area` using percentages of width and height.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// Flatten an entry into an ordered JSON object for the details pane.
fn entry_details_json(entry: &Entry) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "timestamp".to_string(),
        entry
            .timestamp
            .map(|ts| serde_json::Value::String(ts.to_rfc3339()))
            .unwrap_or(serde_json::Value::Null),
    );
    map.insert(
        "level".to_string(),
        entry
            .level
            .map(|level| serde_json::Value::String(level.label().to_string()))
            .unwrap_or(serde_json::Value::Null),
    );
    map.insert(
        "source".to_string(),
        serde_json::Value::String(entry.source.clone()),
    );
    map.insert("line".to_string(), serde_json::Value::from(entry.line));
    if let Some(message) = &entry.message {
        map.insert(
            "message".to_string(),
            serde_json::Value::String(message.clone()),
        );
    }
    if let Some(logger) = &entry.logger {
        map.insert(
            "logger".to_string(),
            serde_json::Value::String(logger.clone()),
        );
    }
    if let Some(caller) = &entry.caller {
        map.insert(
            "caller".to_string(),
            serde_json::Value::String(caller.clone()),
        );
    }
    for (key, value) in &entry.fields {
        map.insert(key.clone(), value.clone());
    }
    serde_json::Value::Object(map)
}

/// Colorize one line of pretty-printed JSON for the details pane.
fn colorize_json_line(line: &str, level_color: Option<Color>) -> Line<'static> {
    let trimmed = line.trim_start();
    let indent_length = line.len() - trimmed.len();
    let indent = &line[..indent_length];

    if trimmed == "{" || trimmed == "}" || trimmed == "}," {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(TEXT_DIM),
        ));
    }

    if let Some((key, value)) = trimmed.split_once(": ") {
        let is_level = key.trim() == "\"level\"";
        let value_style = if is_level {
            level_color.unwrap_or(VALUE_STRING)
        } else {
            json_value_color(value)
        };
        return Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(
                key.to_string(),
                Style::default()
                    .fg(TEXT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": "),
            Span::styled(value.to_string(), Style::default().fg(value_style)),
        ]);
    }

    Line::from(Span::styled(
        line.to_string(),
        Style::default().fg(TEXT_DIM),
    ))
}

fn json_value_color(value: &str) -> Color {
    let value = value.trim_end_matches(',');
    if value.starts_with('"') {
        VALUE_STRING
    } else if value == "true" || value == "false" {
        VALUE_BOOL
    } else if value == "null" || value.starts_with('{') || value.starts_with('[') {
        TEXT_DIM
    } else if value.parse::<f64>().is_ok() {
        VALUE_NUMBER
    } else {
        TEXT_BRIGHT
    }
}

fn level_color(level: Level) -> Color {
    match level {
        Level::Trace => LEVEL_TRACE,
        Level::Debug => LEVEL_DEBUG,
        Level::Info => LEVEL_INFO,
        Level::Warn => LEVEL_WARN,
        Level::Error => LEVEL_ERROR,
        Level::Fatal => LEVEL_FATAL,
        Level::Panic => LEVEL_PANIC,
    }
}

fn help_text() -> String {
    let lines = vec![
        "logcli TUI — Key Bindings".to_string(),
        String::new(),
        "  q        Quit".to_string(),
        "  j/k      Scroll up/down".to_string(),
        "  g/G      Go to top/bottom".to_string(),
        "  /        Open filter prompt".to_string(),
        "  Esc      Clear the active filter".to_string(),
        "  d/Enter  Focus the details pane".to_string(),
        "  o        Add a log file".to_string(),
        "  s        Cycle sort (none/time/level)".to_string(),
        "  f/F      Toggle follow mode".to_string(),
        "  h        Toggle histogram view".to_string(),
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
            redacted: false,
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
        let app = TuiApp::new(entries.clone(), make_args(), None);
        assert_eq!(app.list_state.selected(), Some(0));
        assert_eq!(app.filtered.len(), 3);
        assert_eq!(app.mode, TuiMode::Browsing);
    }

    #[test]
    fn new_app_with_no_entries_has_no_selection() {
        let app = TuiApp::new(vec![], make_args(), None);
        assert_eq!(app.list_state.selected(), None);
        assert_eq!(app.filtered.len(), 0);
    }

    #[test]
    fn scroll_down_moves_selection() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args(), None);
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
        let mut app = TuiApp::new(entries, make_args(), None);
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
        let mut app = TuiApp::new(entries, make_args(), None);
        app.filter_input = "level = error".to_string();
        app.rebuild_filtered();
        assert_eq!(app.mode, TuiMode::Browsing);
        assert_eq!(app.filtered.len(), 2);
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn apply_filter_no_matches() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args(), None);
        app.filter_input = "level = panic".to_string();
        app.rebuild_filtered();
        assert_eq!(app.filtered.len(), 0);
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn cycle_sort_changes_mode() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args(), None);
        assert_eq!(app.sort_mode, Some(SortMode::Time));
        app.cycle_sort();
        assert_eq!(app.sort_mode, Some(SortMode::Level));
        app.cycle_sort();
        assert_eq!(app.sort_mode, None);
        app.cycle_sort();
        assert_eq!(app.sort_mode, Some(SortMode::Time));
    }

    #[test]
    fn toggle_follow_no_files() {
        let entries = make_entries();
        let args = make_args();
        let mut app = TuiApp::new(entries, args, None);
        assert!(!app.follow_enabled);
        app.toggle_follow();
        assert!(!app.follow_enabled); // no files, so stays off
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
    fn level_short_labels() {
        assert_eq!(level_short(Level::Error), "ERR");
        assert_eq!(level_short(Level::Warn), "WRN");
        assert_eq!(level_short(Level::Info), "INF");
        assert_eq!(level_short(Level::Debug), "DBG");
        assert_eq!(level_short(Level::Trace), "TRC");
    }

    #[test]
    fn header_counts_only_levels_shown_in_header() {
        let mut entries = make_entries();
        entries[0].level = Some(Level::Fatal);
        entries[1].level = Some(Level::Warn);
        entries[2].level = Some(Level::Debug);
        let app = TuiApp::new(entries, make_args(), None);
        assert_eq!(app.header_counts(), (1, 1, 0));
    }

    #[test]
    fn details_json_flattens_fields() {
        let entry = &make_entries()[0];
        let value = entry_details_json(entry);
        assert_eq!(value["message"], "connection refused");
        assert_eq!(value["status"], 500);
        assert!(value["timestamp"].is_string());
    }

    #[test]
    fn draw_renders_reference_layout() {
        let entries = make_entries();
        let mut app = TuiApp::new(entries, make_args(), None);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();

        let buffer = terminal.backend().buffer();
        let width = buffer.area.width as usize;
        let mut screen = String::new();
        for row in buffer.content.chunks(width) {
            for cell in row {
                screen.push_str(cell.symbol());
            }
            screen.push('\n');
        }
        for expected in [
            "logcli v",
            "Source:",
            "ERRORS:",
            "WARNs:",
            "INFO:",
            "entries [3]",
            "TIMESTAMP",
            "LEVEL",
            "MESSAGE",
            "details",
            "filter / status",
            "Filter:",
            "Matches:",
            "[q]",
            "Quit",
        ] {
            assert!(
                screen.contains(expected),
                "missing {expected:?} in rendered screen:\n{screen}"
            );
        }
    }

    #[test]
    fn sort_filtered_by_time() {
        let mut entries = make_entries();
        // Give different timestamps
        entries[0].timestamp = Some(Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 2).unwrap());
        entries[1].timestamp = Some(Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap());
        entries[2].timestamp = Some(Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 1).unwrap());

        let mut filtered = vec![0, 1, 2];
        TuiApp::sort_filtered(&mut filtered[..], &entries, Some(SortMode::Time));
        // Should be sorted: entries[1] (10:00:00), entries[2] (10:00:01), entries[0] (10:00:02)
        assert_eq!(filtered, vec![1, 2, 0]);
    }
}
