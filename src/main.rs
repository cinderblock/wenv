mod env_file;
mod screen;

use std::env;
use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Color;

use env_file::EnvFile;
use screen::{Buffer, Style, Viewport};

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
    selected: Option<usize>,
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
            selected: None,
            target: Target::Env,
            mode: Mode::List,
            edit: EditState { key: String::new(), input: String::new(), reveal: false },
            message: String::new(),
            changes: Vec::new(),
            quit: false,
        };
        app.rebuild_vars();
        if !app.vars.is_empty() {
            app.selected = Some(0);
        }
        app
    }

    fn rescan(&mut self) {
        self.example = EnvFile::load(self.dir.join(".env.example"));
        self.env = EnvFile::load(self.dir.join(".env"));
        self.local = EnvFile::load(self.dir.join(".env.local"));
        let prev = self.selected_key();
        self.rebuild_vars();
        self.selected = prev
            .and_then(|k| self.vars.iter().position(|v| v.key == k))
            .or(if self.vars.is_empty() { None } else { Some(0) });
    }

    fn rebuild_vars(&mut self) {
        let mut order: Vec<String> = Vec::new();
        for src in [&self.example, &self.env, &self.local] {
            for k in src.keys() {
                if !order.contains(&k) {
                    order.push(k);
                }
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
        self.selected.and_then(|i| self.vars.get(i)).map(|v| v.key.clone())
    }

    fn move_selection(&mut self, delta: isize) {
        if self.vars.is_empty() {
            return;
        }
        let len = self.vars.len() as isize;
        let cur = self.selected.unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len);
        self.selected = Some(next as usize);
    }

    fn file(&self, col: Target) -> &EnvFile {
        match col {
            Target::Env => &self.env,
            Target::Local => &self.local,
        }
    }

    fn begin_edit(&mut self) {
        if let Some(key) = self.selected_key() {
            self.edit = EditState { key: key.clone(), input: String::new(), reveal: false };
            self.mode = Mode::Edit;
            self.message = format!("Editing {} in {}", key, self.target.filename());
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

fn main() -> io::Result<()> {
    let dir = match env::args_os().nth(1) {
        Some(arg) => PathBuf::from(arg),
        None => env::current_dir()?,
    };
    if !dir.is_dir() {
        eprintln!("wenv: not a directory: {}", dir.display());
        std::process::exit(2);
    }
    let mut app = App::new(dir);
    // Inline viewport: a fixed region that stays in scrollback rather than taking
    // over the whole screen. Size to the var count, within bounds.
    let rows = (app.vars.len().max(1)).min(12) as u16;
    let height = (rows + 8).min(24);
    let vp = screen::init(height)?;
    let res = run(&vp, &mut app);
    // Wipe the inline UI region, then leave only a plain-text summary in scrollback.
    let _ = screen::teardown(&vp);
    print_summary(&app);
    res
}

fn run(vp: &Viewport, app: &mut App) -> io::Result<()> {
    let mut out = io::stdout();
    while !app.quit {
        let mut buf = Buffer::new(vp.width(), vp.height);
        draw(&mut buf, app, vp.height);
        buf.flush(&mut out, vp.ox, vp.oy)?;
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
        KeyCode::Left | KeyCode::Char('h') => app.target = Target::Env,
        KeyCode::Right | KeyCode::Char('l') => app.target = Target::Local,
        KeyCode::Tab => app.target = app.target.toggled(),
        KeyCode::Enter => app.begin_edit(),
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

fn draw(buf: &mut Buffer, app: &App, height: u16) {
    draw_header(buf, app);
    draw_table(buf, app, height);
    draw_footer(buf, app, height);
}

fn present(exists: bool) -> Style {
    if exists {
        Style::fg(Color::Green)
    } else {
        Style::fg(Color::DarkGrey)
    }
}

fn draw_header(buf: &mut Buffer, app: &App) {
    let w = buf.w;
    let max_x = w.saturating_sub(1);
    buf.draw_box(0, 0, w, 4, " wenv ", true, Style::new());
    buf.put(1, 1, max_x, &app.dir.display().to_string(), Style::new().bold());
    let mut x = buf.put(1, 2, max_x, "files: ", Style::new());
    x = buf.put(x, 2, max_x, ".env", present(app.env.exists));
    x = buf.put(x, 2, max_x, "  ", Style::new());
    x = buf.put(x, 2, max_x, ".env.local", present(app.local.exists));
    x = buf.put(x, 2, max_x, "  ", Style::new());
    buf.put(x, 2, max_x, ".env.example", present(app.example.exists));
}

fn draw_table(buf: &mut Buffer, app: &App, height: u16) {
    let w = buf.w;
    let y0 = 4u16;
    let table_h = height - 5;
    let title = format!(" variables ({}) ", app.vars.len());
    buf.draw_box(0, y0, w, table_h, &title, false, Style::new());

    let inner_x = 1u16;
    let max_x = w.saturating_sub(1);
    let label_y = y0 + 1;
    let first_row_y = y0 + 2;
    let visible = (table_h as usize).saturating_sub(3);

    // Column geometry: KEY then two equal file columns.
    let longest = app.vars.iter().map(|v| v.key.chars().count() as u16).max().unwrap_or(6);
    let avail = max_x.saturating_sub(inner_x);
    let key_w = longest.clamp(6, avail.saturating_sub(20).max(6));
    let gap = 1u16;
    let env_x = inner_x + key_w + gap;
    let file_w = max_x.saturating_sub(env_x).saturating_sub(gap) / 2;
    let local_x = env_x + file_w + gap;

    // Column labels; the active file column is highlighted.
    buf.put(inner_x, label_y, env_x, "KEY", Style::new().dim());
    let env_label = Style::fg(Color::Yellow).bold();
    let local_label = Style::fg(Color::Yellow).bold();
    let dim = Style::new().dim();
    buf.put(env_x, label_y, local_x, ".env", if app.target == Target::Env { env_label } else { dim });
    buf.put(local_x, label_y, max_x, ".env.local", if app.target == Target::Local { local_label } else { dim });

    if app.vars.is_empty() {
        buf.put(
            inner_x,
            first_row_y,
            max_x,
            "No variables found in .env / .env.local / .env.example.",
            Style::fg(Color::DarkGrey),
        );
        return;
    }

    let sel = app.selected.unwrap_or(0);
    let offset = if sel >= visible { sel + 1 - visible } else { 0 };

    for (i, v) in app.vars.iter().enumerate().skip(offset).take(visible) {
        let y = first_row_y + (i - offset) as u16;
        let row_selected = app.selected == Some(i);
        let key_style = if row_selected { Style::new().bold() } else { Style::new() };
        buf.put(inner_x, y, env_x - gap, &trunc(&v.key, key_w), key_style);
        draw_cell(buf, app, &v.key, Target::Env, env_x, y, env_x + file_w, row_selected);
        draw_cell(buf, app, &v.key, Target::Local, local_x, y, local_x + file_w, row_selected);
    }
}

fn draw_cell(buf: &mut Buffer, app: &App, key: &str, col: Target, x: u16, y: u16, max_x: u16, row_selected: bool) {
    let active = row_selected && app.target == col;
    buf.fill(x, y, max_x, Style::new().reversed_if(active));

    if active && matches!(app.mode, Mode::Edit) {
        let shown = if app.edit.reveal {
            app.edit.input.clone()
        } else {
            "•".repeat(app.edit.input.chars().count())
        };
        let cap = (max_x.saturating_sub(x)).saturating_sub(1) as usize;
        let text = clip_tail(&shown, cap);
        let cx = buf.put(x, y, max_x, &text, Style::fg(Color::White).reversed_if(active));
        buf.put(cx, y, max_x, "_", Style::fg(Color::White).reversed_if(active).blink());
        return;
    }

    let (label, color) = match app.file(col).get(key).as_deref() {
        None => ("unset".to_string(), Color::DarkGrey),
        Some("") => ("empty".to_string(), Color::Yellow),
        Some(v) => (fingerprint(v), Color::Green),
    };
    buf.put(x, y, max_x, &label, Style::fg(color).reversed_if(active));
}

fn draw_footer(buf: &mut Buffer, app: &App, height: u16) {
    let y = height - 1;
    let max_x = buf.w;
    let help = match app.mode {
        Mode::List => "\u{2191}\u{2193} row  \u{2190}\u{2192}/Tab file  Enter edit  s rescan  q quit",
        Mode::Edit => "type value  Ctrl+R reveal  Enter save  Esc cancel",
    };
    if app.message.is_empty() {
        buf.put(0, y, max_x, help, Style::new().dim());
    } else {
        let x = buf.put(0, y, max_x, &app.message, Style::fg(Color::Cyan));
        let x = buf.put(x, y, max_x, "   ", Style::new());
        buf.put(x, y, max_x, help, Style::new().dim());
    }
}

/// A masked preview of a set value: a few leading and trailing chars so a secret
/// can be recognized without being revealed. Short values show only length dots.
fn fingerprint(v: &str) -> String {
    const PRE: usize = 3;
    const SUF: usize = 2;
    let chars: Vec<char> = v.chars().collect();
    let n = chars.len();
    if n <= PRE + SUF + 1 {
        return "\u{2022}".repeat(n.min(6));
    }
    let pre: String = chars[..PRE].iter().collect();
    let suf: String = chars[n - SUF..].iter().collect();
    format!("{}\u{2026}{}", pre, suf)
}

fn trunc(s: &str, w: u16) -> String {
    let w = w as usize;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= w {
        s.to_string()
    } else if w == 0 {
        String::new()
    } else {
        let mut t: String = chars[..w - 1].iter().collect();
        t.push('\u{2026}');
        t
    }
}

fn clip_tail(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        s.to_string()
    } else {
        chars[chars.len() - width..].iter().collect()
    }
}

/// True when it's safe to emit ANSI color: stdout is a real terminal, NO_COLOR is
/// not set, and the platform supports ANSI (on Windows this also enables VT
/// processing). Result of supports_ansi() is cached by crossterm.
fn color_enabled() -> bool {
    use std::io::IsTerminal;
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if !std::io::stdout().is_terminal() {
        return false;
    }
    #[cfg(windows)]
    {
        crossterm::ansi_support::supports_ansi()
    }
    #[cfg(not(windows))]
    {
        std::env::var("TERM").map_or(true, |t| t != "dumb")
    }
}

fn print_summary(app: &App) {
    use crossterm::style::{Color, Stylize};

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

    println!("{}  {}", bold("wenv"), paint(&app.dir.display().to_string(), Color::DarkGrey));

    if app.vars.is_empty() {
        println!(
            "  {}",
            paint("no variables found (.env / .env.local / .env.example)", Color::DarkGrey)
        );
    } else {
        println!("  {}", bold("variables:"));
        for v in &app.vars {
            let tag = paint(&format!("{:<5}", status_word(v.status)), status_color(v.status));
            println!("    [{}] {:<28} {}", tag, v.key, paint(&format!("({})", v.source), Color::DarkGrey));
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
