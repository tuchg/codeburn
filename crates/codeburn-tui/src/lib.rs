use std::collections::HashMap;
use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame, Terminal,
};

use codeburn_core::classifier::category_label;
use codeburn_core::parser::discover_and_parse;
use codeburn_core::stats::build_report;
use codeburn_core::types::{Period, ProviderKind, Report};

// ──────────── Palette (matches TS dashboard) ────────────

const ORANGE: Color = Color::Rgb(255, 140, 66);
const GOLD: Color = Color::Rgb(255, 215, 0);
const BLUE: Color = Color::Rgb(91, 158, 245);
const GREEN: Color = Color::Rgb(91, 245, 160);
const PURPLE: Color = Color::Rgb(224, 91, 245);
const TEAL: Color = Color::Rgb(91, 245, 224);
const AMBER: Color = Color::Rgb(245, 200, 91);
const DIM: Color = Color::Rgb(85, 85, 85);

fn bold(c: Color) -> Style {
    Style::new().fg(c).add_modifier(Modifier::BOLD)
}
fn fg(c: Color) -> Style {
    Style::new().fg(c)
}
fn dim() -> Style {
    Style::new().fg(DIM)
}

// ──────────── Provider list for cycling ────────────

const PROVIDER_CYCLE: &[&str] = &[
    "all", "claude", "codex", "cursor", "opencode", "gemini", "copilot", "pi",
];

fn provider_kind(name: &str) -> Option<ProviderKind> {
    match name {
        "claude" => Some(ProviderKind::Claude),
        "codex" => Some(ProviderKind::Codex),
        "cursor" => Some(ProviderKind::Cursor),
        "opencode" => Some(ProviderKind::Opencode),
        "gemini" => Some(ProviderKind::Gemini),
        "copilot" => Some(ProviderKind::Copilot),
        "pi" => Some(ProviderKind::Pi),
        _ => None,
    }
}

// ──────────── App State ────────────

pub struct App {
    period: Period,
    provider_idx: usize,
    report: Report,
    scroll: u16,
    content_height: u16,
}

impl App {
    fn new(period: Period, provider: Option<&ProviderKind>) -> Self {
        let provider_idx = provider
            .and_then(|pk| PROVIDER_CYCLE.iter().position(|&s| s == pk.as_str()))
            .unwrap_or(0);
        let mut app = Self {
            period,
            provider_idx,
            report: build_report(&[], ""),
            scroll: 0,
            content_height: 0,
        };
        app.reload();
        app
    }

    fn current_provider(&self) -> Option<ProviderKind> {
        provider_kind(PROVIDER_CYCLE[self.provider_idx])
    }

    fn reload(&mut self) {
        let (dr, label) = self.period.date_range();
        let pk = self.current_provider();
        let projects = discover_and_parse(&dr, pk.as_ref());
        self.report = build_report(&projects, &label);
        self.scroll = 0;
    }
}

// ──────────── Entry Point ────────────

/// Launch the interactive TUI. Returns immediately if stdout is not a terminal.
pub fn run_tui(period: Period, provider: Option<&ProviderKind>) -> io::Result<()> {
    if !io::stdout().is_terminal() {
        return Ok(());
    }
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(period, provider);
    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

// ──────────── Event Loop ────────────

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        // Clamp scroll to valid range before drawing.
        let visible_h = terminal.size()?.height.saturating_sub(4); // tabs + status
        app.scroll = app.scroll.min(app.content_height.saturating_sub(visible_h));

        terminal.draw(|f| {
            let total = render(f, app);
            app.content_height = total;
        })?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Left => {
                        app.period = app.period.prev();
                        app.reload();
                    }
                    KeyCode::Right | KeyCode::Tab => {
                        app.period = app.period.next();
                        app.reload();
                    }
                    KeyCode::Char('1') => {
                        app.period = Period::Today;
                        app.reload();
                    }
                    KeyCode::Char('2') => {
                        app.period = Period::Week;
                        app.reload();
                    }
                    KeyCode::Char('3') => {
                        app.period = Period::Days30;
                        app.reload();
                    }
                    KeyCode::Char('4') => {
                        app.period = Period::Month;
                        app.reload();
                    }
                    KeyCode::Char('p') => {
                        app.provider_idx = (app.provider_idx + 1) % PROVIDER_CYCLE.len();
                        app.reload();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.scroll = app.scroll.saturating_add(1);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.scroll = app.scroll.saturating_sub(1);
                    }
                    KeyCode::PageDown => app.scroll = app.scroll.saturating_add(10),
                    KeyCode::PageUp => app.scroll = app.scroll.saturating_sub(10),
                    _ => {}
                }
        }
    }
}

// ──────────── Top-level Render ────────────

/// Renders the full TUI frame and returns the total virtual content height.
fn render(f: &mut Frame, app: &App) -> u16 {
    let [tabs_rect, content_rect, status_rect] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .areas(f.area());

    render_tabs(f, app, tabs_rect);
    let total_h = render_content(f, app, content_rect);
    render_status(f, status_rect);
    total_h
}

// ──────────── Period Tabs ────────────

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let tabs = [Period::Today, Period::Week, Period::Days30, Period::Month];
    let mut spans: Vec<Span> = Vec::new();
    for period in &tabs {
        if *period == app.period {
            spans.push(Span::styled(
                format!(" [ {} ] ", period.label()),
                bold(ORANGE),
            ));
        } else {
            spans.push(Span::styled(
                format!("   {}   ", period.label()),
                dim(),
            ));
        }
    }
    let provider_name = PROVIDER_CYCLE[app.provider_idx];
    if provider_name != "all" {
        spans.push(Span::styled("  |  ", dim()));
        spans.push(Span::styled(format!("[{}]", provider_name), bold(ORANGE)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ──────────── Status Bar ────────────

fn render_status(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("◄►", bold(ORANGE)),
        Span::styled(" period   ", dim()),
        Span::styled("p", bold(ORANGE)),
        Span::styled(" provider   ", dim()),
        Span::styled("↑↓", bold(ORANGE)),
        Span::styled(" scroll   ", dim()),
        Span::styled("1", bold(ORANGE)),
        Span::styled(" today  ", dim()),
        Span::styled("2", bold(ORANGE)),
        Span::styled(" week  ", dim()),
        Span::styled("3", bold(ORANGE)),
        Span::styled(" 30d  ", dim()),
        Span::styled("4", bold(ORANGE)),
        Span::styled(" month   ", dim()),
        Span::styled("q", bold(ORANGE)),
        Span::styled(" quit", dim()),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(dim());
    f.render_widget(Paragraph::new(line).block(block), area);
}

// ──────────── Scrollable Content Area ────────────

/// Row types for the virtual layout.
enum RowKind {
    Overview,
    PairDailyProjects,
    PairActivityModels,
    PairToolsBash,
    SingleTools,
    SingleBash,
    Mcp,
    Empty,
}

struct VRow {
    y: u16,
    h: u16,
    kind: RowKind,
}

fn render_content(f: &mut Frame, app: &App, area: Rect) -> u16 {
    let report = &app.report;
    let wide = area.width >= 90;
    let bar_w: u16 = if wide { 10 } else { 8 };

    let has_tools = !report.tool_breakdown.is_empty();
    let has_bash = !report.bash_breakdown.is_empty();
    let has_mcp = !report.mcp_breakdown.is_empty();

    let n_daily = daily_costs(report).len().min(14) as u16;
    let n_proj = report.projects.len().min(8) as u16;
    let n_act = report.category_breakdown.len().min(8) as u16;
    let n_mod = report.model_breakdown.len().min(8) as u16;
    let n_tools = report.tool_breakdown.len().min(8) as u16;
    let n_bash = report.bash_breakdown.len().min(8) as u16;
    let n_mcp = report.mcp_breakdown.len().min(8) as u16;

    let mut vrows: Vec<VRow> = Vec::new();
    let mut y = 0u16;

    let push = |vrows: &mut Vec<VRow>, y: &mut u16, h: u16, kind: RowKind| {
        vrows.push(VRow { y: *y, h, kind });
        *y += h;
    };

    // Overview (full width, fixed height)
    push(&mut vrows, &mut y, 5, RowKind::Overview);

    if wide {
        // Daily | Projects
        push(&mut vrows, &mut y, n_daily.max(n_proj) + 4, RowKind::PairDailyProjects);
        // Activity | Models
        push(&mut vrows, &mut y, n_act.max(n_mod) + 4, RowKind::PairActivityModels);
        // Tools | Bash (only if either has data)
        if has_tools || has_bash {
            push(&mut vrows, &mut y, n_tools.max(n_bash) + 4, RowKind::PairToolsBash);
        }
    } else {
        push(&mut vrows, &mut y, n_daily + 4, RowKind::PairDailyProjects);
        push(&mut vrows, &mut y, n_proj + 4, RowKind::PairActivityModels);
        push(&mut vrows, &mut y, n_act + 4, RowKind::PairToolsBash);
        if has_tools {
            push(&mut vrows, &mut y, n_tools + 4, RowKind::SingleTools);
        }
        if has_bash {
            push(&mut vrows, &mut y, n_bash + 4, RowKind::SingleBash);
        }
    }

    if has_mcp {
        push(&mut vrows, &mut y, n_mcp + 4, RowKind::Mcp);
    } else if !has_tools && !has_bash && report.projects.is_empty() {
        push(&mut vrows, &mut y, 3, RowKind::Empty);
    }

    let total_h = y;

    for row in &vrows {
        let row_end = row.y + row.h;
        if row_end <= app.scroll {
            continue;
        }
        if row.y >= app.scroll + area.height {
            break;
        }
        let screen_y = row.y.saturating_sub(app.scroll);
        let screen_end = (row_end - app.scroll).min(area.height);
        if screen_y >= screen_end {
            continue;
        }
        let rect = Rect::new(area.x, area.y + screen_y, area.width, screen_end - screen_y);

        match row.kind {
            RowKind::Overview => render_overview(f, report, rect),
            RowKind::PairDailyProjects => {
                if wide {
                    let [left, right] = Layout::horizontal([
                        Constraint::Percentage(50),
                        Constraint::Percentage(50),
                    ])
                    .areas(rect);
                    render_daily_activity(f, report, left, bar_w);
                    render_projects(f, report, right, bar_w);
                } else {
                    render_daily_activity(f, report, rect, bar_w);
                }
            }
            RowKind::PairActivityModels => {
                if wide {
                    let [left, right] = Layout::horizontal([
                        Constraint::Percentage(50),
                        Constraint::Percentage(50),
                    ])
                    .areas(rect);
                    render_activity(f, report, left, bar_w);
                    render_models(f, report, right, bar_w);
                } else {
                    render_projects(f, report, rect, bar_w);
                }
            }
            RowKind::PairToolsBash => {
                if wide {
                    let [left, right] = Layout::horizontal([
                        Constraint::Percentage(50),
                        Constraint::Percentage(50),
                    ])
                    .areas(rect);
                    render_tools(f, report, left, bar_w);
                    render_bash(f, report, right, bar_w);
                } else {
                    render_activity(f, report, rect, bar_w);
                }
            }
            RowKind::SingleTools => render_tools(f, report, rect, bar_w),
            RowKind::SingleBash => render_bash(f, report, rect, bar_w),
            RowKind::Mcp => render_mcp(f, report, rect, bar_w),
            RowKind::Empty => render_empty(f, report, rect),
        }
    }

    total_h
}

// ──────────── Overview Panel ────────────

fn render_overview(f: &mut Frame, report: &Report, area: Rect) {
    let block = panel_block("CodeBurn", ORANGE);
    let inner = block.inner(area);

    let cost_str = fmt_cost(report.total_cost_usd);
    let cache_str = format!("{:.0}%", report.cache_hit_pct);

    let lines = vec![
        Line::from(vec![
            Span::styled(&report.label, dim()),
        ]),
        Line::from(vec![
            Span::styled(cost_str, bold(GOLD)),
            Span::styled(" cost   ", dim()),
            Span::styled(report.total_api_calls.to_string(), bold(Color::White)),
            Span::styled(" calls   ", dim()),
            Span::styled(report.total_sessions.to_string(), bold(Color::White)),
            Span::styled(" sessions   ", dim()),
            Span::styled(cache_str, bold(TEAL)),
            Span::styled(" cache hit", dim()),
        ]),
        Line::from(vec![
            Span::styled(
                format!(
                    "{} in   {} out   {} cached   {} written",
                    fmt_tok(report.total_tokens.input_tokens),
                    fmt_tok(report.total_tokens.output_tokens),
                    fmt_tok(report.total_tokens.cache_read_tokens),
                    fmt_tok(report.total_tokens.cache_creation_tokens),
                ),
                dim(),
            ),
        ]),
    ];

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── Daily Activity Panel ────────────

fn render_daily_activity(f: &mut Frame, report: &Report, area: Rect, bar_w: u16) {
    let block = panel_block("Daily Activity", BLUE);
    let inner = block.inner(area);
    let costs = daily_costs(report);
    let sorted: Vec<(&String, &f64)> = {
        let mut v: Vec<_> = costs.iter().collect();
        v.sort_by_key(|(k, _)| *k);
        v.into_iter().rev().take(inner.height as usize).collect::<Vec<_>>().into_iter().rev().collect()
    };
    let max_cost = sorted.iter().map(|(_, v)| **v).fold(0.0_f64, f64::max);

    let bw = bar_w as usize;
    let header = Line::from(Span::styled(
        format!("{:<5}  {}  {:>8}", "day", " ".repeat(bw), "cost"),
        dim(),
    ));

    let mut lines = vec![header];
    for (day, cost) in &sorted {
        let date_short = if day.len() >= 10 { &day[5..10] } else { day.as_str() };
        lines.push(Line::from(vec![
            Span::styled(format!("{:<5}  ", date_short), dim()),
            Span::styled(make_bar(**cost, max_cost, bar_w as usize), fg(BLUE)),
            Span::styled(format!("  {:>8}", fmt_cost(**cost)), bold(GOLD)),
        ]));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── Projects Panel ────────────

fn render_projects(f: &mut Frame, report: &Report, area: Rect, bar_w: u16) {
    let block = panel_block("Projects", GREEN);
    let inner = block.inner(area);
    let max_cost = report
        .projects
        .iter()
        .map(|p| p.total_cost_usd)
        .fold(0.0_f64, f64::max);

    let name_w = (inner.width as usize).saturating_sub(bar_w as usize + 14).max(4);
    let header = Line::from(Span::styled(
        format!("{}  {:>8}  name", " ".repeat(bar_w as usize), ""),
        dim(),
    ));

    let mut lines = vec![header];
    for p in report.projects.iter().take(inner.height as usize - 1) {
        let name = fit(&p.project_path, name_w);
        lines.push(Line::from(vec![
            Span::styled(make_bar(p.total_cost_usd, max_cost, bar_w as usize), fg(GREEN)),
            Span::styled(format!("  {:>8}  ", fmt_cost(p.total_cost_usd)), bold(GOLD)),
            Span::styled(name, fg(Color::White)),
        ]));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── Activity Breakdown Panel ────────────

fn render_activity(f: &mut Frame, report: &Report, area: Rect, bar_w: u16) {
    let block = panel_block("Activity Breakdown", AMBER);
    let inner = block.inner(area);
    let max_cost = report
        .category_breakdown
        .iter()
        .map(|(_, s)| s.cost_usd)
        .fold(0.0_f64, f64::max);

    let name_w = (inner.width as usize).saturating_sub(bar_w as usize + 14).max(8);
    let header = Line::from(Span::styled(
        format!("{}  {:>8}  activity", " ".repeat(bar_w as usize), ""),
        dim(),
    ));

    let mut lines = vec![header];
    for (cat, stats) in report.category_breakdown.iter().take(inner.height as usize - 1) {
        let label = fit(category_label(cat), name_w);
        lines.push(Line::from(vec![
            Span::styled(make_bar(stats.cost_usd, max_cost, bar_w as usize), fg(AMBER)),
            Span::styled(format!("  {:>8}  ", fmt_cost(stats.cost_usd)), bold(GOLD)),
            Span::styled(label, fg(Color::White)),
        ]));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── Model Breakdown Panel ────────────

fn render_models(f: &mut Frame, report: &Report, area: Rect, bar_w: u16) {
    let block = panel_block("Models", PURPLE);
    let inner = block.inner(area);
    let max_cost = report
        .model_breakdown
        .iter()
        .map(|(_, s)| s.cost_usd)
        .fold(0.0_f64, f64::max);

    let name_w = (inner.width as usize).saturating_sub(bar_w as usize + 14).max(6);
    let header = Line::from(Span::styled(
        format!("{}  {:>8}  model", " ".repeat(bar_w as usize), ""),
        dim(),
    ));

    let mut lines = vec![header];
    for (model, stats) in report.model_breakdown.iter().take(inner.height as usize - 1) {
        let name = fit(model, name_w);
        lines.push(Line::from(vec![
            Span::styled(make_bar(stats.cost_usd, max_cost, bar_w as usize), fg(PURPLE)),
            Span::styled(format!("  {:>8}  ", fmt_cost(stats.cost_usd)), bold(GOLD)),
            Span::styled(name, fg(Color::White)),
        ]));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── Tools Panel ────────────

fn render_tools(f: &mut Frame, report: &Report, area: Rect, bar_w: u16) {
    let block = panel_block("Tools", TEAL);
    let inner = block.inner(area);
    let max_calls = report
        .tool_breakdown
        .iter()
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(1);

    let name_w = (inner.width as usize).saturating_sub(bar_w as usize + 10).max(4);
    let header = Line::from(Span::styled(
        format!("{}  {:>6}  tool", " ".repeat(bar_w as usize), ""),
        dim(),
    ));

    let mut lines = vec![header];
    for (tool, calls) in report.tool_breakdown.iter().take(inner.height as usize - 1) {
        let name = fit(tool, name_w);
        lines.push(Line::from(vec![
            Span::styled(make_bar(*calls as f64, max_calls as f64, bar_w as usize), fg(TEAL)),
            Span::styled(format!("  {:>6}  ", calls), bold(Color::White)),
            Span::styled(name, fg(Color::White)),
        ]));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── Shell Commands Panel ────────────

fn render_bash(f: &mut Frame, report: &Report, area: Rect, bar_w: u16) {
    let block = panel_block("Shell Commands", ORANGE);
    let inner = block.inner(area);
    let max_calls = report
        .bash_breakdown
        .iter()
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(1);

    let name_w = (inner.width as usize).saturating_sub(bar_w as usize + 10).max(4);
    let header = Line::from(Span::styled(
        format!("{}  {:>6}  command", " ".repeat(bar_w as usize), ""),
        dim(),
    ));

    let mut lines = vec![header];
    for (cmd, calls) in report.bash_breakdown.iter().take(inner.height as usize - 1) {
        let name = fit(cmd, name_w);
        lines.push(Line::from(vec![
            Span::styled(make_bar(*calls as f64, max_calls as f64, bar_w as usize), fg(ORANGE)),
            Span::styled(format!("  {:>6}  ", calls), bold(Color::White)),
            Span::styled(name, fg(Color::White)),
        ]));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── MCP Servers Panel ────────────

fn render_mcp(f: &mut Frame, report: &Report, area: Rect, bar_w: u16) {
    let block = panel_block("MCP Servers", PURPLE);
    let inner = block.inner(area);
    let max_calls = report
        .mcp_breakdown
        .iter()
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(1);

    let name_w = (inner.width as usize).saturating_sub(bar_w as usize + 10).max(4);
    let header = Line::from(Span::styled(
        format!("{}  {:>6}  server", " ".repeat(bar_w as usize), ""),
        dim(),
    ));

    let mut lines = vec![header];
    for (srv, calls) in report.mcp_breakdown.iter().take(inner.height as usize - 1) {
        let name = fit(srv, name_w);
        lines.push(Line::from(vec![
            Span::styled(make_bar(*calls as f64, max_calls as f64, bar_w as usize), fg(PURPLE)),
            Span::styled(format!("  {:>6}  ", calls), bold(Color::White)),
            Span::styled(name, fg(Color::White)),
        ]));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

// ──────────── Empty State ────────────

fn render_empty(f: &mut Frame, report: &Report, area: Rect) {
    let block = panel_block("CodeBurn", ORANGE);
    let inner = block.inner(area);
    let line = Line::from(Span::styled(
        format!("No usage data found for {}.", report.label),
        dim(),
    ));
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line]), inner);
}

// ──────────── Helpers ────────────

fn panel_block(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .title(Span::styled(title, bold(color)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(fg(color))
}

fn make_bar(value: f64, max: f64, width: usize) -> String {
    if max <= 0.0 || width == 0 {
        return "░".repeat(width);
    }
    let filled = ((value / max) * width as f64).round() as usize;
    let filled = filled.min(width);
    let mut s = String::with_capacity(width);
    for _ in 0..filled {
        s.push('█');
    }
    for _ in filled..width {
        s.push('░');
    }
    s
}

fn fmt_cost(cost: f64) -> String {
    if cost >= 100.0 {
        format!("${:.0}", cost)
    } else if cost >= 1.0 {
        format!("${:.2}", cost)
    } else if cost >= 0.01 {
        format!("${:.3}", cost)
    } else {
        format!("${:.4}", cost)
    }
}

fn fmt_tok(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn fit(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let byte_end = s
            .char_indices()
            .nth(n)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        s[..byte_end].to_string()
    } else {
        format!("{:<width$}", s, width = n)
    }
}

/// Aggregate cost per calendar day across all project sessions and turns.
fn daily_costs(report: &Report) -> HashMap<String, f64> {
    let mut map: HashMap<String, f64> = HashMap::new();
    for project in &report.projects {
        for session in &project.sessions {
            for turn in &session.turns {
                if turn.timestamp.len() >= 10 {
                    let day = turn.timestamp[..10].to_string();
                    let cost: f64 = turn.calls.iter().map(|c| c.cost_usd).sum();
                    *map.entry(day).or_insert(0.0) += cost;
                }
            }
        }
    }
    map
}
