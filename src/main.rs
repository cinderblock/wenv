#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

extern crate alloc;

mod env_file;
#[cfg(all(windows, not(test)))]
mod rt;
mod screen;
mod sys;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use env_file::EnvFile;
use screen::{Buffer, Color, Style, Viewport};
use sys::Key;

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
    dir: String,
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

fn join(dir: &str, name: &str) -> String {
    let mut p = String::from(dir);
    if !p.is_empty() && !p.ends_with('/') && !p.ends_with('\\') {
        p.push('/');
    }
    p.push_str(name);
    p
}

impl App {
    fn new(dir: String) -> Self {
        let example = EnvFile::load(join(&dir, ".env.example"));
        let env = EnvFile::load(join(&dir, ".env"));
        let local = EnvFile::load(join(&dir, ".env.local"));
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
        self.example = EnvFile::load(join(&self.dir, ".env.example"));
        self.env = EnvFile::load(join(&self.dir, ".env"));
        self.local = EnvFile::load(join(&self.dir, ".env.local"));
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
            self.message = format!("Editing {} in {}", key, self.target.filename());
            self.edit = EditState { key, input: String::new(), reveal: false };
            self.mode = Mode::Edit;
        }
    }

    fn confirm_edit(&mut self) {
        let key = self.edit.key.clone();
        let value = self.edit.input.clone();
        let target = self.target;
        let ok = {
            let file = match target {
                Target::Env => &mut self.env,
                Target::Local => &mut self.local,
            };
            file.set(&key, &value);
            file.save()
        };
        if ok {
            self.message = format!("Wrote {} -> {}", key, target.filename());
            self.record_change(key, target.filename());
        } else {
            self.message = format!("ERROR writing {}", target.filename());
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

#[cfg(all(windows, not(test)))]
#[unsafe(no_mangle)]
pub extern "C" fn mainCRTStartup() -> ! {
    let code = real_main();
    sys::exit(code)
}

#[cfg(all(unix, not(test)))]
#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    sys::set_args(argc, argv);
    real_main()
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    sys::write_stdout(b"\x1b[0m\r\nwenv panicked\r\n");
    sys::exit(101)
}

fn real_main() -> i32 {
    let dir = match sys::args().into_iter().nth(1) {
        Some(a) => a,
        None => match sys::cwd() {
            Some(d) => d,
            None => {
                sys::write_stdout(b"wenv: cannot determine current directory\r\n");
                return 2;
            }
        },
    };
    if !sys::is_dir(&dir) {
        let mut m = String::from("wenv: not a directory: ");
        m.push_str(&dir);
        m.push_str("\r\n");
        sys::write_stdout(m.as_bytes());
        return 2;
    }

    let app = App::new(dir);
    // No console (piped/redirected): skip the interactive UI, just print state.
    if !sys::interactive() {
        print_summary(&app);
        return 0;
    }
    let mut app = app;
    let rows = (app.vars.len().max(1)).min(12) as u16;
    let height = (rows + 8).min(24);
    let vp = screen::init(height);
    run(&vp, &mut app);
    screen::teardown(&vp);
    print_summary(&app);
    0
}

fn run(vp: &Viewport, app: &mut App) {
    while !app.quit {
        let mut buf = Buffer::new(vp.width(), vp.height);
        draw(&mut buf, app, vp.height);
        buf.flush(vp.ox, vp.oy);
        let key = sys::read_key();
        match app.mode {
            Mode::List => handle_list_key(app, key),
            Mode::Edit => handle_edit_key(app, key),
        }
    }
}

fn handle_list_key(app: &mut App, key: Key) {
    match key {
        Key::Char('q') | Key::Esc => app.quit = true,
        Key::Up | Key::Char('k') => app.move_selection(-1),
        Key::Down | Key::Char('j') => app.move_selection(1),
        Key::Left | Key::Char('h') => app.target = Target::Env,
        Key::Right | Key::Char('l') => app.target = Target::Local,
        Key::Tab => app.target = app.target.toggled(),
        Key::Enter => app.begin_edit(),
        Key::Char('s') => {
            app.rescan();
            app.message = "Rescanned".to_string();
        }
        _ => {}
    }
}

fn handle_edit_key(app: &mut App, key: Key) {
    match key {
        Key::Esc => {
            app.mode = Mode::List;
            app.message = "Edit cancelled".to_string();
        }
        Key::Enter => app.confirm_edit(),
        Key::Backspace => {
            app.edit.input.pop();
        }
        Key::Ctrl('r') => {
            app.edit.reveal = !app.edit.reveal;
        }
        Key::Char(c) => {
            app.edit.input.push(c);
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
    buf.put(1, 1, max_x, &app.dir, Style::new().bold());
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

    let longest = app.vars.iter().map(|v| v.key.chars().count() as u16).max().unwrap_or(6);
    let avail = max_x.saturating_sub(inner_x);
    let key_w = longest.clamp(6, avail.saturating_sub(20).max(6));
    let gap = 1u16;
    let env_x = inner_x + key_w + gap;
    let file_w = max_x.saturating_sub(env_x).saturating_sub(gap) / 2;
    let local_x = env_x + file_w + gap;

    buf.put(inner_x, label_y, env_x, "KEY", Style::new().dim());
    let active = Style::fg(Color::Yellow).bold();
    let dim = Style::new().dim();
    buf.put(env_x, label_y, local_x, ".env", if app.target == Target::Env { active } else { dim });
    buf.put(local_x, label_y, max_x, ".env.local", if app.target == Target::Local { active } else { dim });

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
            let mut s = String::new();
            for _ in 0..app.edit.input.chars().count() {
                s.push('•');
            }
            s
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
        let mut s = String::new();
        for _ in 0..n.min(6) {
            s.push('\u{2022}');
        }
        return s;
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

fn color_enabled() -> bool {
    sys::env_var("NO_COLOR").is_none() && sys::stdout_is_tty()
}

fn paint(out: &mut String, on: bool, code: &str, text: &str) {
    if on {
        out.push_str("\x1b[");
        out.push_str(code);
        out.push('m');
        out.push_str(text);
        out.push_str("\x1b[0m");
    } else {
        out.push_str(text);
    }
}

fn status_code(s: Status) -> &'static str {
    match s {
        Status::Set => "32",
        Status::Empty => "33",
        Status::Unset => "90",
    }
}

fn print_summary(app: &App) {
    let color = color_enabled();
    let mut out = String::new();

    paint(&mut out, color, "1", "wenv");
    out.push_str("  ");
    paint(&mut out, color, "90", &app.dir);
    out.push_str("\r\n");

    if app.vars.is_empty() {
        out.push_str("  ");
        paint(&mut out, color, "90", "no variables found (.env / .env.local / .env.example)");
        out.push_str("\r\n");
    } else {
        out.push_str("  ");
        paint(&mut out, color, "1", "variables:");
        out.push_str("\r\n");
        for v in &app.vars {
            out.push_str("    [");
            paint(&mut out, color, status_code(v.status), &format!("{:<5}", status_word(v.status)));
            out.push_str("] ");
            out.push_str(&format!("{:<28}", v.key));
            out.push(' ');
            paint(&mut out, color, "90", &format!("({})", v.source));
            out.push_str("\r\n");
        }
    }

    if !app.changes.is_empty() {
        out.push_str("  ");
        paint(&mut out, color, "1", "changed this session:");
        out.push_str("\r\n");
        for c in &app.changes {
            let status = app.vars.iter().find(|v| v.key == c.key).map(|v| v.status);
            let word = status.map(status_word).unwrap_or("?");
            out.push_str("    ");
            out.push_str(&format!("{:<28} ", c.key));
            match status {
                Some(s) => paint(&mut out, color, status_code(s), word),
                None => out.push_str(word),
            }
            out.push(' ');
            paint(&mut out, color, "90", "->");
            out.push(' ');
            paint(&mut out, color, "36", c.target);
            out.push_str("\r\n");
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
    out.push_str("  ");
    paint(&mut out, color, "32", &set.to_string());
    out.push_str(" set  ");
    paint(&mut out, color, "33", &empty.to_string());
    out.push_str(" empty  ");
    paint(&mut out, color, "90", &unset.to_string());
    out.push_str(" unset\r\n");

    sys::write_stdout(out.as_bytes());
}
