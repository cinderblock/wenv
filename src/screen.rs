//! A minimal terminal renderer built directly on crossterm.
//!
//! Replaces ratatui with just what wenv needs: an inline viewport (a fixed
//! region that stays in scrollback instead of an alternate screen), a tiny
//! cell buffer, and box/text drawing primitives. Each frame builds a fresh
//! `Buffer`, then `flush` paints it in place from the viewport origin.

use std::io::{self, Write};

use crossterm::cursor::{self, MoveTo};
use crossterm::style::{Attribute, Color, Print, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{execute, queue};

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

    /// Write `s` left-to-right starting at (x, y), stopping before `max_x` (or
    /// the buffer edge). Returns the column just past the last char written so
    /// callers can chain styled spans on one line.
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

    /// Fill a row span [x, max_x) with blanks of the given style.
    pub fn fill(&mut self, x: u16, y: u16, max_x: u16, style: Style) {
        let limit = max_x.min(self.w);
        let mut cx = x;
        while cx < limit {
            self.set(cx, y, ' ', style);
            cx += 1;
        }
    }

    /// Draw a single-line box. `title` is embedded in the top border, centered
    /// when `center` is true, otherwise left-aligned after the corner.
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
            let tx = if center && tlen < inner {
                x + 1 + (inner - tlen) / 2
            } else {
                x + 1
            };
            self.put(tx, y, right, title, border);
        }
    }

    /// Paint the buffer in place starting at the viewport origin (ox, oy),
    /// overwriting the previous frame without clearing first (no flicker).
    pub fn flush(&self, out: &mut impl Write, ox: u16, oy: u16) -> io::Result<()> {
        for r in 0..self.h {
            queue!(out, MoveTo(ox, oy + r))?;
            let mut cur: Option<Style> = None;
            for c in 0..self.w {
                let cell = self.cells[self.idx(c, r)];
                if cur != Some(cell.style) {
                    queue!(out, SetAttribute(Attribute::Reset))?;
                    if cell.style.bold {
                        queue!(out, SetAttribute(Attribute::Bold))?;
                    }
                    if cell.style.dim {
                        queue!(out, SetAttribute(Attribute::Dim))?;
                    }
                    if cell.style.reverse {
                        queue!(out, SetAttribute(Attribute::Reverse))?;
                    }
                    if cell.style.blink {
                        queue!(out, SetAttribute(Attribute::SlowBlink))?;
                    }
                    queue!(out, SetForegroundColor(cell.style.fg))?;
                    cur = Some(cell.style);
                }
                queue!(out, Print(cell.ch))?;
            }
        }
        queue!(out, SetAttribute(Attribute::Reset))?;
        out.flush()
    }
}

/// An inline drawing region: `height` rows anchored at (ox, oy) that scroll
/// with the rest of the terminal's scrollback.
pub struct Viewport {
    pub ox: u16,
    pub oy: u16,
    pub height: u16,
}

impl Viewport {
    /// Current usable width (re-queried so horizontal resizes are handled).
    pub fn width(&self) -> u16 {
        terminal::size().map(|(w, _)| w).unwrap_or(80)
    }
}

/// Reserve `height` rows below the cursor and enter raw mode. Newlines are
/// emitted while still in cooked mode so each one returns to column 0 (and the
/// terminal scrolls if we're at the bottom); then we climb back to the top of
/// the reserved region and record it as the origin.
pub fn init(height: u16) -> io::Result<Viewport> {
    let mut out = io::stdout();
    write!(out, "{}", "\n".repeat(height as usize))?;
    out.flush()?;
    execute!(out, cursor::MoveToPreviousLine(height))?;
    let (ox, oy) = cursor::position()?;
    terminal::enable_raw_mode()?;
    execute!(out, cursor::Hide)?;
    Ok(Viewport { ox, oy, height })
}

/// Clear the reserved region, restore the cursor to the origin, and leave raw
/// mode so the caller can print a plain-text summary into scrollback.
pub fn teardown(vp: &Viewport) -> io::Result<()> {
    let mut out = io::stdout();
    for r in 0..vp.height {
        queue!(out, MoveTo(vp.ox, vp.oy + r), Clear(ClearType::CurrentLine))?;
    }
    queue!(out, MoveTo(vp.ox, vp.oy), cursor::Show)?;
    out.flush()?;
    terminal::disable_raw_mode()
}
