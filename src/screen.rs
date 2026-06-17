//! A minimal cell buffer that renders to ANSI escape sequences and writes them
//! through the OS layer. No external TUI dependency.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::sys;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Reset,
    Green,
    Yellow,
    Cyan,
    White,
    DarkGrey,
}

impl Color {
    fn code(self) -> u16 {
        match self {
            Color::Reset => 39,
            Color::Green => 32,
            Color::Yellow => 33,
            Color::Cyan => 36,
            Color::White => 37,
            Color::DarkGrey => 90,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub fg: Color,
    pub bold: bool,
    pub dim: bool,
    pub reverse: bool,
    pub blink: bool,
}

impl Style {
    pub fn new() -> Self {
        Style { fg: Color::Reset, bold: false, dim: false, reverse: false, blink: false }
    }
    pub fn fg(c: Color) -> Self {
        Style { fg: c, ..Style::new() }
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }
    pub fn blink(mut self) -> Self {
        self.blink = true;
        self
    }
    pub fn reversed_if(mut self, on: bool) -> Self {
        self.reverse = on;
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    ch: char,
    style: Style,
}

pub struct Buffer {
    pub w: u16,
    pub h: u16,
    cells: Vec<Cell>,
}

fn push_num(s: &mut String, mut n: u16) {
    if n == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 5];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        s.push(b as char);
    }
}

impl Buffer {
    pub fn new(w: u16, h: u16) -> Self {
        let blank = Cell { ch: ' ', style: Style::new() };
        Buffer { w, h, cells: vec![blank; w as usize * h as usize] }
    }

    fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.w as usize + x as usize
    }

    pub fn set(&mut self, x: u16, y: u16, ch: char, style: Style) {
        if x < self.w && y < self.h {
            let i = self.idx(x, y);
            self.cells[i] = Cell { ch, style };
        }
    }

    /// Write `s` starting at (x, y), stopping before `max_x` (or the buffer edge).
    /// Returns the column just past the last char written.
    pub fn put(&mut self, x: u16, y: u16, max_x: u16, s: &str, style: Style) -> u16 {
        let limit = max_x.min(self.w);
        let mut cx = x;
        for ch in s.chars() {
            if cx >= limit {
                break;
            }
            self.set(cx, y, ch, style);
            cx += 1;
        }
        cx
    }

    pub fn fill(&mut self, x: u16, y: u16, max_x: u16, style: Style) {
        let limit = max_x.min(self.w);
        let mut cx = x;
        while cx < limit {
            self.set(cx, y, ' ', style);
            cx += 1;
        }
    }

    pub fn draw_box(&mut self, x: u16, y: u16, w: u16, h: u16, title: &str, center: bool, border: Style) {
        if w < 2 || h < 2 {
            return;
        }
        let right = x + w - 1;
        let bottom = y + h - 1;
        self.set(x, y, '┌', border);
        self.set(right, y, '┐', border);
        self.set(x, bottom, '└', border);
        self.set(right, bottom, '┘', border);
        for cx in (x + 1)..right {
            self.set(cx, y, '─', border);
            self.set(cx, bottom, '─', border);
        }
        for cy in (y + 1)..bottom {
            self.set(x, cy, '│', border);
            self.set(right, cy, '│', border);
        }
        if !title.is_empty() {
            let inner = w.saturating_sub(2);
            let tlen = title.chars().count() as u16;
            let tx = if center && tlen < inner { x + 1 + (inner - tlen) / 2 } else { x + 1 };
            self.put(tx, y, right, title, border);
        }
    }

    fn push_style(out: &mut String, st: Style) {
        out.push_str("\x1b[0");
        if st.bold {
            out.push_str(";1");
        }
        if st.dim {
            out.push_str(";2");
        }
        if st.reverse {
            out.push_str(";7");
        }
        if st.blink {
            out.push_str(";5");
        }
        out.push(';');
        push_num(out, st.fg.code());
        out.push('m');
    }

    /// Render the buffer in place at viewport origin (ox, oy), 0-based. Builds one
    /// ANSI string and writes it in a single call.
    pub fn flush(&self, ox: u16, oy: u16) {
        let mut out = String::with_capacity(self.cells.len() * 2 + self.h as usize * 8);
        for r in 0..self.h {
            out.push_str("\x1b[");
            push_num(&mut out, oy + r + 1);
            out.push(';');
            push_num(&mut out, ox + 1);
            out.push('H');
            let mut cur: Option<Style> = None;
            for c in 0..self.w {
                let cell = self.cells[self.idx(c, r)];
                if cur != Some(cell.style) {
                    Self::push_style(&mut out, cell.style);
                    cur = Some(cell.style);
                }
                out.push(cell.ch);
            }
        }
        out.push_str("\x1b[0m");
        sys::write_stdout(out.as_bytes());
    }
}

/// Inline viewport: `height` rows anchored at (ox, oy) within scrollback.
pub struct Viewport {
    pub ox: u16,
    pub oy: u16,
    pub height: u16,
    term: sys::Term,
}

/// Reserve `height` rows, enter raw mode, hide the cursor, and record the origin.
pub fn init(height: u16) -> Viewport {
    let term = sys::term_init();
    // Reserve space by emitting newlines, then read where the cursor landed and
    // back up to the top of the region. Absolute positioning is used per frame.
    let mut nl = String::new();
    for _ in 0..height {
        nl.push_str("\r\n");
    }
    sys::write_stdout(nl.as_bytes());
    let (_, cy) = sys::cursor_pos();
    let oy = cy.saturating_sub(height);
    sys::write_stdout(b"\x1b[?25l");
    Viewport { ox: 0, oy, height, term }
}

impl Viewport {
    pub fn width(&self) -> u16 {
        sys::term_size().0
    }
}

/// Clear the region, restore the cursor and console state, and leave the cursor
/// at the origin so the caller can print a plain summary.
pub fn teardown(vp: &Viewport) {
    let mut out = String::new();
    for r in 0..vp.height {
        out.push_str("\x1b[");
        push_num(&mut out, vp.oy + r + 1);
        out.push_str(";1H\x1b[2K");
    }
    out.push_str("\x1b[");
    push_num(&mut out, vp.oy + 1);
    out.push_str(";1H\x1b[?25h");
    sys::write_stdout(out.as_bytes());
    sys::term_restore(&vp.term);
}
