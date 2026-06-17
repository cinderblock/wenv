mod env_file;

use std::env;
use std::path::PathBuf;

use env_file::EnvFile;
use ratatui::backend::Backend;
use ratatui::crossterm::cursor::MoveTo;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::disable_raw_mode;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Env,
    Local,
}

impl Target {
    fn filename(self) -> &'static str {
        match self {
            Target::Env => ".env",
            Target::Local => ".env.local",
        }
    }
    fn toggled(self) -> Target {
        match self {
            Target::Env => Target::Local,
            Target::Local => Target::Env,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Set,
    Empty,
    Unset,
}

fn status_word(s: Status) -> &'static str {
    match s {
        Status::Set => "set",
        Status::Empty => "empty",
        Status::Unset => "unset",
    }
}

struct VarEntry {
    key: String,
    status: Status,
    source: &'static str,
}

struct Change {
    key: String,
    target: &'static str,
}

enum Mode {
    List,
    Edit,
}

struct EditState {
    key: String,
    input: String,
    reveal: bool,
}

struct App {
    dir: PathBuf,
    example: EnvFile,
    env: EnvFile,
    local: EnvFile,
    vars: Vec<VarEntry>,
    list_state: ListState,
    target: Target,
    mode: Mode,
    edit: EditState,
    message: String,
    changes: Vec<Change>,
    quit: bool,
}

impl App {
    fn new(dir: PathBuf) -> Self {
        let example = EnvFile::load(dir.join(".env.example"));
        let env = EnvFile::load(dir.join(".env"));
        let local = EnvFile::load(dir.join(".env.local"));
        let mut app = App {
            dir,
            example,
            env,
            local,
            vars: Vec::new(),
            list_state: ListState::default(),
            target: Target::Env,
            mode: Mode::List,
            edit: EditState { key: String::new(), input: String::new(), reveal: false },
            message: String::new(),
            changes: Vec::new(),
            quit: false,
        };
        app.rebuild_vars();
        if !app.vars.is_empty() {
            app.list_state.select(Some(0));
        }
        app
    }

    fn rescan(&mut self) {
        self.example = EnvFile::load(self.dir.join(".env.example"));
        self.env = EnvFile::load(self.dir.join(".env"));
        self.local = EnvFile::load(self.dir.join(".env.local"));
        let prev = self.selected_key();
        self.rebuild_vars();
        let idx = prev
            .and_then(|k| self.vars.iter().position(|v| v.key == k))
            .or(if self.vars.is_empty() { None } else { Some(0) });
        self.list_state.select(idx);
    }

    fn rebuild_vars(&mut self) {
        let mut order: Vec<String> = Vec::new();
        for k in self.example.keys() {
            if !order.contains(&k) {
                order.push(k);
            }
        }
        for k in self.env.keys() {
            if !order.contains(&k) {
                order.push(k);
            }
        }
        for k in self.local.keys() {
            if !order.contains(&k) {
                order.push(k);
            }
        }

        self.vars = order
            .into_iter()
            .map(|key| {
                let env_v = self.env.get(&key);
                let local_v = self.local.get(&key);
                let effective = local_v.clone().or_else(|| env_v.clone());
                let status = match effective {
                    Some(v) if !v.is_empty() => Status::Set,
                    Some(_) => Status::Empty,
                    None => Status::Unset,
                };
                let source = if local_v.is_some() {
                    "local"
                } else if env_v.is_some() {
                    "*"
                } else {
                    "example"
                };
                VarEntry { key, status, source }
            })
            .collect();
    }

    fn selected_key(&self) -> Option<String> {
        self.list_state
            .selected()
            .and_then(|i| self.vars.get(i))
            .map(|v| v.key.clone())
    }

    fn move_selection(&mut self, delta: isize) {
        if self.vars.is_empty() {
            return;
        }
        let len = self.vars.len() as isize;
        let cur = self.list_state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len);
        self.list_state.select(Some(next as usize));
    }

    fn begin_edit(&mut self) {
        if let Some(key) = self.selected_key() {
            self.edit = EditState { key, input: String::new(), reveal: false };
            self.mode = Mode::Edit;
            self.message.clear();
        }
    }

    fn confirm_edit(&mut self) {
        let key = self.edit.key.clone();
        let value = self.edit.input.clone();
        let target = self.target;
        let result = {
            let file = match target {
                Target::Env => &mut self.env,
                Target::Local => &mut self.local,
            };
            file.set(&key, &value);
            file.save()
        };
        match result {
            Ok(()) => {
                self.message = format!("Wrote {} -> {}", key, target.filename());
                self.record_change(key, target.filename());
            }
            Err(e) => {
                self.message = format!("ERROR writing {}: {}", target.filename(), e);
            }
        }
        self.mode = Mode::List;
        self.rescan();
    }

    fn record_change(&mut self, key: String, target: &'static str) {
        match self.changes.iter_mut().find(|c| c.key == key) {
            Some(c) => c.target = target,
            None => self.changes.push(Change { key, target }),
        }
    }
}

fn main() -> std::io::Result<()> {
    let dir = env::current_dir()?;
    let mut app = App::new(dir);
    // Inline viewport: render in a fixed region that stays in scrollback rather
    // than taking over the whole screen. Size to the var count, within bounds.
    let rows = (app.vars.len().max(1)).min(12) as u16;
    let height = (rows + 8).min(24);
    let mut terminal =
        ratatui::init_with_options(TerminalOptions { viewport: Viewport::Inline(height) });
    let res = run(&mut terminal, &mut app);
    // Wipe the inline UI region, then leave only a plain-text summary in scrollback.
    // clear() restores the cursor to the viewport bottom, so move it back to the
    // viewport origin before printing or the summary lands below blank lines.
    let origin = terminal.get_frame().area();
    let _ = terminal.clear();
    let _ = terminal.backend_mut().flush();
    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), MoveTo(origin.x, origin.y));
    print_summary(&app);
    res
}

/// True when it's safe to emit ANSI color: stdout is a real terminal, NO_COLOR is
/// not set, and the platform supports ANSI (on Windows this also enables VT
/// processing). Result of supports_ansi() is cached by crossterm.
fn color_enabled() -> bool {
    use std::io::IsTerminal;
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal() && ratatui::crossterm::ansi_support::supports_ansi()
}

fn print_summary(app: &App) {
    use ratatui::crossterm::style::{Color, Stylize};

    let color = color_enabled();
    let paint = |s: &str, c: Color| -> String {
        if color {
            s.with(c).to_string()
        } else {
            s.to_string()
        }
    };
    let bold = |s: &str| -> String {
        if color {
            s.bold().to_string()
        } else {
            s.to_string()
        }
    };
    let status_color = |s: Status| match s {
        Status::Set => Color::Green,
        Status::Empty => Color::Yellow,
        Status::Unset => Color::DarkGrey,
    };

    println!(
        "{}  {}",
        bold("wenv"),
        paint(&app.dir.display().to_string(), Color::DarkGrey)
    );

    if app.vars.is_empty() {
        println!(
            "  {}",
            paint("no variables found (.env / .env.local / .env.example)", Color::DarkGrey)
        );
    } else {
        println!("  {}", bold("variables:"));
        for v in &app.vars {
            let tag = paint(&format!("{:<5}", status_word(v.status)), status_color(v.status));
            println!(
                "    [{}] {:<28} {}",
                tag,
                v.key,
                paint(&format!("({})", v.source), Color::DarkGrey)
            );
        }
    }

    if !app.changes.is_empty() {
        println!("  {}", bold("changed this session:"));
        for c in &app.changes {
            let status = app.vars.iter().find(|v| v.key == c.key).map(|v| v.status);
            let word = status.map(status_word).unwrap_or("?");
            let colored = match status {
                Some(s) => paint(word, status_color(s)),
                None => word.to_string(),
            };
            println!(
                "    {:<28} {} {} {}",
                c.key,
                colored,
                paint("->", Color::DarkGrey),
                paint(c.target, Color::Cyan)
            );
        }
    }

    let (mut set, mut empty, mut unset) = (0u32, 0u32, 0u32);
    for v in &app.vars {
        match v.status {
            Status::Set => set += 1,
            Status::Empty => empty += 1,
            Status::Unset => unset += 1,
        }
    }
    println!(
        "  {} set  {} empty  {} unset",
        paint(&set.to_string(), Color::Green),
        paint(&empty.to_string(), Color::Yellow),
        paint(&unset.to_string(), Color::DarkGrey)
    );
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    while !app.quit {
        terminal.draw(|f| draw(f, app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match app.mode {
                Mode::List => handle_list_key(app, key.code),
                Mode::Edit => handle_edit_key(app, key.code, key.modifiers),
            }
        }
    }
    Ok(())
}

fn handle_list_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::Enter => app.begin_edit(),
        KeyCode::Tab => {
            app.target = app.target.toggled();
            app.message = format!("Write target: {}", app.target.filename());
        }
        KeyCode::Char('s') => {
            app.rescan();
            app.message = "Rescanned".to_string();
        }
        _ => {}
    }
}

fn handle_edit_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::List;
            app.message = "Edit cancelled".to_string();
        }
        KeyCode::Enter => app.confirm_edit(),
        KeyCode::Backspace => {
            app.edit.input.pop();
        }
        KeyCode::Char('r') if mods.contains(KeyModifiers::CONTROL) => {
            app.edit.reveal = !app.edit.reveal;
        }
        KeyCode::Char(c) => {
            if !mods.contains(KeyModifiers::CONTROL) {
                app.edit.input.push(c);
            }
        }
        _ => {}
    }
}

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_list(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    if let Mode::Edit = app.mode {
        draw_edit_popup(f, app);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let dir = app.dir.display().to_string();
    let present = |name: &str, exists: bool| -> Span {
        if exists {
            Span::styled(name.to_string(), Style::default().fg(Color::Green))
        } else {
            Span::styled(name.to_string(), Style::default().fg(Color::DarkGray))
        }
    };
    let files = Line::from(vec![
        Span::raw("files: "),
        present(".env", app.env.exists),
        Span::raw("  "),
        present(".env.local", app.local.exists),
        Span::raw("  "),
        present(".env.example", app.example.exists),
    ]);
    let target = Line::from(vec![
        Span::raw("writing to: "),
        Span::styled(
            app.target.filename().to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("   (Tab to switch)"),
    ]);
    let body = vec![
        Line::from(Span::styled(dir, Style::default().add_modifier(Modifier::BOLD))),
        files,
        target,
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" wenv ")
        .title_alignment(Alignment::Center);
    f.render_widget(Paragraph::new(body).block(block), area);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = if app.vars.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No variables found in .env / .env.local / .env.example.",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.vars
            .iter()
            .map(|v| {
                let (label, color) = match v.status {
                    Status::Set => ("set  ", Color::Green),
                    Status::Empty => ("empty", Color::Yellow),
                    Status::Unset => ("unset", Color::DarkGray),
                };
                let line = Line::from(vec![
                    Span::styled(format!("[{}] ", label), Style::default().fg(color)),
                    Span::raw(format!("{:<28}", v.key)),
                    Span::styled(format!("({})", v.source), Style::default().fg(Color::DarkGray)),
                ]);
                ListItem::new(line)
            })
            .collect()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" variables ({}) ", app.vars.len()));
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = app.list_state.clone();
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        Mode::List => "up/down move  Enter edit  Tab target  s rescan  q quit",
        Mode::Edit => "type value  Ctrl+R reveal  Enter save  Esc cancel",
    };
    let line = if app.message.is_empty() {
        Line::from(Span::styled(help, Style::default().fg(Color::DarkGray)))
    } else {
        Line::from(vec![
            Span::styled(app.message.clone(), Style::default().fg(Color::Cyan)),
            Span::raw("   "),
            Span::styled(help, Style::default().fg(Color::DarkGray)),
        ])
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_edit_popup(f: &mut Frame, app: &App) {
    let area = centered_rect(70, 7, f.area());
    f.render_widget(Clear, area);

    let shown = if app.edit.reveal {
        app.edit.input.clone()
    } else {
        "*".repeat(app.edit.input.chars().count())
    };
    let reveal_tag = if app.edit.reveal { "shown" } else { "masked" };

    let body = vec![
        Line::from(vec![
            Span::raw("key: "),
            Span::styled(
                app.edit.key.clone(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("target: "),
            Span::styled(app.target.filename().to_string(), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::raw("value: "),
            Span::styled(shown, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK)),
            Span::styled(format!("  [{}]", reveal_tag), Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" set value ");
    f.render_widget(Paragraph::new(body).block(block), area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}
