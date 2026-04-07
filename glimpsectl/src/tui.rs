use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use futures_util::StreamExt;
use glimpse_client::{Client, SubscriptionEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::{Frame, Terminal};
use tokio::sync::mpsc;

use crate::picker::{self, Picker};

struct Message {
    direction: Direction,
    text: String,
    /// Pre-rendered lines for display.
    lines: Vec<Line<'static>>,
}

#[derive(Clone, Copy)]
enum Direction {
    Out,
    In,
}

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Messages,
    Input,
    Picker,
}

struct App {
    messages: Vec<Message>,
    input: String,
    selected: usize,
    focus: Focus,
    should_quit: bool,
    client: Client,
    event_rx: mpsc::Receiver<SubscriptionEvent>,
    event_tx: mpsc::Sender<SubscriptionEvent>,
    history: Vec<String>,
    history_pos: Option<usize>,
    picker: Option<Picker>,
}

impl App {
    fn select_last(&mut self) {
        self.selected = self.messages.len().saturating_sub(1);
    }

    fn push_out(&mut self, text: String) {
        let lines = format_message_lines(&text, Direction::Out);
        self.messages.push(Message {
            direction: Direction::Out,
            text,
            lines,
        });
        self.select_last();
    }

    fn push_in(&mut self, text: String) {
        let lines = format_message_lines(&text, Direction::In);
        self.messages.push(Message {
            direction: Direction::In,
            text,
            lines,
        });
        self.select_last();
    }

    async fn open_picker(&mut self) {
        let entries = match self.client.get("inspect.providers").await {
            Ok(data) => picker::build_entries(&data),
            Err(_) => Vec::new(),
        };
        self.picker = Some(Picker::new(entries));
        self.focus = Focus::Picker;
    }

    fn close_picker(&mut self) {
        self.picker = None;
        self.focus = Focus::Input;
    }

    async fn accept_picker(&mut self) {
        if let Some(picker) = &self.picker {
            if let Some(cmd) = picker.selected_command() {
                self.input = cmd.to_owned();
            }
        }
        self.close_picker();
        self.execute_command().await;
    }

    async fn execute_command(&mut self) {
        let raw = self.input.trim().to_owned();
        if raw.is_empty() {
            return;
        }
        self.history.push(raw.clone());
        self.history_pos = None;
        self.input.clear();

        let parts: Vec<&str> = raw.splitn(3, ' ').collect();
        match parts.first().copied() {
            Some("quit" | "exit" | "q") => {
                self.should_quit = true;
            }
            Some("clear") => {
                self.messages.clear();
                self.selected = 0;
            }
            Some("help") => {
                self.push_in("commands:".into());
                self.push_in("  get <topic>             read a topic".into());
                self.push_in("  sub <pattern>           subscribe to events".into());
                self.push_in("  unsub <pattern>         unsubscribe".into());
                self.push_in("  call <method> [params]  call a method".into());
                self.push_in("  inspect                 list providers".into());
                self.push_in("  pick                    open command picker".into());
                self.push_in("  clear                   clear messages".into());
                self.push_in("  quit                    exit".into());
                self.push_in("".into());
                self.push_in(
                    "Tab: switch pane | Ctrl+P: picker | Ctrl+L: clear | Ctrl+Q: quit".into(),
                );
            }
            Some("get") if parts.len() >= 2 => {
                let topic = parts[1].to_owned();
                self.push_out(format!("get {topic}"));
                match self.client.get(&topic).await {
                    Ok(data) => self.push_in(format_value(&data)),
                    Err(e) => self.push_in(format!("error: {e}")),
                }
            }
            Some("sub") if parts.len() >= 2 => {
                let pattern = parts[1].to_owned();
                self.push_out(format!("sub {pattern}"));
                match self.client.subscribe(&pattern).await {
                    Ok(mut sub) => {
                        self.push_in(format!("subscribed to {pattern}"));
                        let tx = self.event_tx.clone();
                        tokio::spawn(async move {
                            while let Some(event) = sub.next().await {
                                if tx.send(event).await.is_err() {
                                    break;
                                }
                            }
                        });
                    }
                    Err(e) => self.push_in(format!("error: {e}")),
                }
            }
            Some("unsub") if parts.len() >= 2 => {
                let pattern = parts[1].to_owned();
                self.push_out(format!("unsub {pattern}"));
                match self.client.unsubscribe(&pattern).await {
                    Ok(()) => self.push_in(format!("unsubscribed from {pattern}")),
                    Err(e) => self.push_in(format!("error: {e}")),
                }
            }
            Some("call") if parts.len() >= 2 => {
                let method = parts[1].to_owned();
                let raw_params = if parts.len() >= 3 {
                    parts[2].trim()
                } else {
                    "{}"
                };
                let raw_params = raw_params
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
                    .unwrap_or(raw_params);
                let params: serde_json::Value = match serde_json::from_str(raw_params) {
                    Ok(v) => v,
                    Err(e) => {
                        self.push_out(format!("call {method} {raw_params}"));
                        self.push_in(format!("invalid JSON: {e}"));
                        return;
                    }
                };
                self.push_out(format!("call {method} {params}"));
                match self.client.call(&method, params).await {
                    Ok(data) => self.push_in(format_value(&data)),
                    Err(e) => self.push_in(format!("error: {e}")),
                }
            }
            Some("pick" | "picker" | "p") => {
                self.open_picker().await;
            }
            Some("inspect") => {
                self.push_out("inspect".into());
                match self.client.get("inspect.providers").await {
                    Ok(data) => {
                        if let Some(providers) = data.as_array() {
                            for p in providers {
                                let name = p["name"].as_str().unwrap_or("?");
                                self.push_in(format!("{name}:"));
                                if let Some(topics) = p["topics"].as_array() {
                                    for t in topics {
                                        if let Some(s) = t.as_str() {
                                            self.push_in(format!("  topic: {s}"));
                                        }
                                    }
                                }
                                if let Some(methods) = p["methods"].as_array() {
                                    for m in methods {
                                        if let Some(s) = m.as_str() {
                                            self.push_in(format!("  method: {s}"));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => self.push_in(format!("error: {e}")),
                }
            }
            _ => {
                self.push_in(format!("unknown: {raw}"));
                self.push_in("type 'help' for available commands".into());
            }
        }
    }
}

fn format_value(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

pub async fn run_tui() -> anyhow::Result<()> {
    let client = Client::connect().await?;
    let (event_tx, event_rx) = mpsc::channel(64);

    let mut app = App {
        messages: Vec::new(),
        input: String::new(),
        selected: 0,
        focus: Focus::Input,
        should_quit: false,
        client,
        event_rx,
        event_tx,
        history: Vec::new(),
        history_pos: None,
        picker: None,
    };

    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut event_stream = crossterm::event::EventStream::new();

    app.push_in("connected to glimpsed. type 'help' for commands.".into());

    loop {
        terminal.draw(|f| draw(f, &app))?;

        tokio::select! {
            Some(event) = event_stream.next() => {
                if let Ok(Event::Key(key)) = event {
                    handle_key(&mut app, key).await;
                }
            }
            Some(event) = app.event_rx.recv() => {
                app.push_in(format!("[{}] {}", event.topic, format_value(&event.data)));
            }
        }

        if app.should_quit {
            break;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    Ok(())
}

async fn handle_key(app: &mut App, key: event::KeyEvent) {
    // Global shortcuts.
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('q' | 'c')) => {
            app.should_quit = true;
            return;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
            app.messages.clear();
            app.selected = 0;
            return;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.open_picker().await;
            return;
        }
        _ => {}
    }

    match app.focus {
        Focus::Picker => handle_picker_key(app, key).await,
        Focus::Messages => handle_messages_key(app, key),
        Focus::Input => handle_input_key(app, key).await,
    }
}

async fn handle_picker_key(app: &mut App, key: event::KeyEvent) {
    if key.code == KeyCode::Enter {
        app.accept_picker().await;
        return;
    }
    match key.code {
        KeyCode::Esc => app.close_picker(),
        KeyCode::Up => {
            if let Some(p) = &mut app.picker {
                p.move_up();
            }
        }
        KeyCode::Down => {
            if let Some(p) = &mut app.picker {
                p.move_down();
            }
        }
        KeyCode::Char(c) => {
            if let Some(p) = &mut app.picker {
                p.type_char(c);
            }
        }
        KeyCode::Backspace => {
            if let Some(p) = &mut app.picker {
                p.backspace();
            }
        }
        _ => {}
    }
}

fn handle_messages_key(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Tab => app.focus = Focus::Input,
        KeyCode::Up => app.selected = app.selected.saturating_sub(1),
        KeyCode::Down => {
            if !app.messages.is_empty() {
                app.selected = (app.selected + 1).min(app.messages.len() - 1);
            }
        }
        KeyCode::PageUp => app.selected = app.selected.saturating_sub(10),
        KeyCode::PageDown => {
            if !app.messages.is_empty() {
                app.selected = (app.selected + 10).min(app.messages.len() - 1);
            }
        }
        KeyCode::Home => app.selected = 0,
        KeyCode::End => app.select_last(),
        _ => {}
    }
}

async fn handle_input_key(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Tab => app.focus = Focus::Messages,
        KeyCode::Enter => app.execute_command().await,
        KeyCode::Char(c) => app.input.push(c),
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Up => {
            if !app.history.is_empty() {
                let pos = match app.history_pos {
                    Some(p) => p.saturating_sub(1),
                    None => app.history.len() - 1,
                };
                app.history_pos = Some(pos);
                app.input = app.history[pos].clone();
            }
        }
        KeyCode::Down => {
            if let Some(pos) = app.history_pos {
                if pos + 1 < app.history.len() {
                    app.history_pos = Some(pos + 1);
                    app.input = app.history[pos + 1].clone();
                } else {
                    app.history_pos = None;
                    app.input.clear();
                }
            }
        }
        _ => {}
    }
}

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(f.area());
    draw_messages(f, app, chunks[0]);
    draw_input(f, app, chunks[1]);

    if let Some(picker) = &app.picker {
        draw_picker(f, picker, f.area());
    }
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;

    // Collect all rendered lines with message index.
    let mut all_lines: Vec<(usize, Line<'static>)> = Vec::new();
    for (msg_idx, msg) in app.messages.iter().enumerate() {
        for line in &msg.lines {
            all_lines.push((msg_idx, line.clone()));
        }
    }

    let total = all_lines.len();

    // Find first line of selected message.
    let selected_line = all_lines
        .iter()
        .position(|(idx, _)| *idx == app.selected)
        .unwrap_or(0);

    let start = if total <= height || selected_line < height / 2 {
        0
    } else if selected_line + height / 2 >= total {
        total.saturating_sub(height)
    } else {
        selected_line - height / 2
    };

    let visible: Vec<Line<'static>> = all_lines[start..]
        .iter()
        .take(height)
        .map(|(msg_idx, line)| {
            if *msg_idx == app.selected && app.focus == Focus::Messages {
                line.clone()
                    .patch_style(Style::default().bg(Color::DarkGray))
            } else {
                line.clone()
            }
        })
        .collect();

    let border_color = if app.focus == Focus::Messages {
        Color::Blue
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .title(" Messages ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let paragraph = Paragraph::new(visible).block(block);
    f.render_widget(paragraph, area);
}

/// Format a message into colored ratatui Lines.
fn format_message_lines(text: &str, direction: Direction) -> Vec<Line<'static>> {
    let (arrow, arrow_color) = match direction {
        Direction::Out => ("→ ", Color::Cyan),
        Direction::In => ("← ", Color::Green),
    };

    // Try to parse as JSON for pretty formatting.
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.to_owned());
        let mut lines = Vec::new();
        for (i, line) in pretty.lines().enumerate() {
            let prefix = if i == 0 {
                Span::styled(arrow.to_owned(), Style::default().fg(arrow_color))
            } else {
                Span::raw("  ") // indent continuation
            };
            let spans = colorize_json_line(line);
            let mut all_spans = vec![prefix];
            all_spans.extend(spans);
            lines.push(Line::from(all_spans));
        }
        lines
    } else {
        // Plain text.
        vec![Line::from(vec![
            Span::styled(arrow.to_owned(), Style::default().fg(arrow_color)),
            Span::raw(text.to_owned()),
        ])]
    }
}

/// Colorize a single line of pretty-printed JSON.
fn colorize_json_line(line: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    if !indent.is_empty() {
        spans.push(Span::raw(indent.to_owned()));
    }

    let mut pos = 0;

    while pos < trimmed.len() {
        let ch = trimmed.as_bytes()[pos];
        match ch {
            b'"' => {
                // Find closing quote.
                let rest = &trimmed[pos + 1..];
                let end = rest.find('"').map(|i| pos + 2 + i).unwrap_or(trimmed.len());
                let s = &trimmed[pos..end];
                // Check if this is a key (followed by ':').
                let after = trimmed[end..].trim_start();
                let color = if after.starts_with(':') {
                    Color::Blue
                } else {
                    Color::Green
                };
                spans.push(Span::styled(s.to_owned(), Style::default().fg(color)));
                pos = end;
            }
            b'0'..=b'9' | b'-' => {
                let end = trimmed[pos..]
                    .find(|c: char| {
                        !c.is_ascii_digit()
                            && c != '.'
                            && c != '-'
                            && c != 'e'
                            && c != 'E'
                            && c != '+'
                    })
                    .map(|i| pos + i)
                    .unwrap_or(trimmed.len());
                spans.push(Span::styled(
                    trimmed[pos..end].to_owned(),
                    Style::default().fg(Color::Yellow),
                ));
                pos = end;
            }
            b't' | b'f'
                if trimmed[pos..].starts_with("true") || trimmed[pos..].starts_with("false") =>
            {
                let word_len = if trimmed[pos..].starts_with("true") {
                    4
                } else {
                    5
                };
                spans.push(Span::styled(
                    trimmed[pos..pos + word_len].to_owned(),
                    Style::default().fg(Color::Magenta),
                ));
                pos += word_len;
            }
            b'n' if trimmed[pos..].starts_with("null") => {
                spans.push(Span::styled(
                    "null".to_owned(),
                    Style::default().fg(Color::DarkGray),
                ));
                pos += 4;
            }
            _ => {
                // Punctuation: {, }, [, ], :, ,
                spans.push(Span::styled(
                    (ch as char).to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
                pos += 1;
            }
        }
    }

    spans
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let border_color = if app.focus == Focus::Input {
        Color::Blue
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .title(" Input (Ctrl+P: picker) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let input = Paragraph::new(format!("> {}", app.input)).block(block);
    f.render_widget(input, area);

    if app.focus == Focus::Input {
        f.set_cursor_position((area.x + 3 + app.input.len() as u16, area.y + 1));
    }
}

fn draw_picker(f: &mut Frame, picker: &Picker, area: Rect) {
    let width = (area.width / 2).max(40).min(area.width.saturating_sub(4));
    let height = (area.height / 2).max(10).min(area.height.saturating_sub(4));
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;
    let popup = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup);

    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(popup);

    // Search input.
    let search_block = Block::default()
        .title(" Commands ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let search = Paragraph::new(format!("/ {}", picker.query)).block(search_block);
    f.render_widget(search, chunks[0]);
    f.set_cursor_position((chunks[0].x + 3 + picker.query.len() as u16, chunks[0].y + 1));

    // Results list.
    let visible_height = chunks[1].height.saturating_sub(2) as usize;
    let start = if picker.selected >= visible_height {
        picker.selected - visible_height + 1
    } else {
        0
    };

    let items: Vec<ListItem> = picker.filtered[start..]
        .iter()
        .take(visible_height)
        .enumerate()
        .map(|(i, &idx)| {
            let entry = &picker.entries[idx];
            let global_i = start + i;
            let mut style = Style::default();
            if global_i == picker.selected {
                style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
            }
            ListItem::new(Line::from(vec![
                Span::styled(&entry.command, style),
                Span::styled(
                    format!("  {}", entry.description),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .style(style)
        })
        .collect();

    let results_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Yellow));
    let footer = format!(
        " {} results | Enter: select | Esc: cancel ",
        picker.filtered.len()
    );
    let results_block = results_block.title_bottom(Line::from(footer).centered());
    let list = List::new(items).block(results_block);
    f.render_widget(list, chunks[1]);
}
