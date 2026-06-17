//! Unix backend (Linux + macOS): talks to the OS through the system libc, which
//! is always present (nothing to install). The `libc` crate only provides FFI
//! declarations and per-platform constants/structs.

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;
use core::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use libc::{c_char, c_int};

use super::Key;

static ARGC: AtomicIsize = AtomicIsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(core::ptr::null_mut());

/// Called from the libc `main(argc, argv)` entry before anything else.
pub fn set_args(argc: i32, argv: *const *const u8) {
    ARGC.store(argc as isize, Ordering::Relaxed);
    ARGV.store(argv as *mut *const u8, Ordering::Relaxed);
}

unsafe fn cstr_to_string(p: *const u8) -> String {
    let mut len = 0usize;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        let slice = core::slice::from_raw_parts(p, len);
        String::from_utf8_lossy(slice).into_owned()
    }
}

fn cstring(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s.as_bytes());
    v.push(0);
    v
}

pub fn write_stdout(bytes: &[u8]) {
    let mut off = 0usize;
    while off < bytes.len() {
        let n = unsafe { libc::write(1, bytes[off..].as_ptr() as *const c_void, bytes.len() - off) };
        if n <= 0 {
            break;
        }
        off += n as usize;
    }
}

pub fn exit(code: i32) -> ! {
    unsafe { libc::exit(code) }
}

pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let cpath = cstring(path);
    unsafe {
        let fd = libc::open(cpath.as_ptr() as *const c_char, libc::O_RDONLY);
        if fd < 0 {
            return None;
        }
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = libc::read(fd, chunk.as_mut_ptr() as *mut c_void, chunk.len());
            if n < 0 {
                libc::close(fd);
                return None;
            }
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n as usize]);
        }
        libc::close(fd);
        Some(buf)
    }
}

pub fn write_file(path: &str, data: &[u8]) -> bool {
    let cpath = cstring(path);
    unsafe {
        let fd = libc::open(
            cpath.as_ptr() as *const c_char,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o644 as c_int,
        );
        if fd < 0 {
            return false;
        }
        let mut off = 0usize;
        while off < data.len() {
            let n = libc::write(fd, data[off..].as_ptr() as *const c_void, data.len() - off);
            if n <= 0 {
                libc::close(fd);
                return false;
            }
            off += n as usize;
        }
        libc::close(fd);
        true
    }
}

pub fn is_dir(path: &str) -> bool {
    let cpath = cstring(path);
    unsafe {
        let fd = libc::open(cpath.as_ptr() as *const c_char, libc::O_RDONLY | libc::O_DIRECTORY);
        if fd >= 0 {
            libc::close(fd);
            true
        } else {
            false
        }
    }
}

pub fn args() -> Vec<String> {
    let argc = ARGC.load(Ordering::Relaxed);
    let argv = ARGV.load(Ordering::Relaxed) as *const *const u8;
    let mut out = Vec::new();
    if argv.is_null() {
        return out;
    }
    unsafe {
        for i in 0..argc {
            let p = *argv.offset(i);
            if p.is_null() {
                break;
            }
            out.push(cstr_to_string(p));
        }
    }
    out
}

pub fn env_var(name: &str) -> Option<String> {
    let cname = cstring(name);
    unsafe {
        let p = libc::getenv(cname.as_ptr() as *const c_char);
        if p.is_null() {
            None
        } else {
            Some(cstr_to_string(p as *const u8))
        }
    }
}

pub fn cwd() -> Option<String> {
    let mut buf = vec![0u8; 4096];
    unsafe {
        let p = libc::getcwd(buf.as_mut_ptr() as *mut c_char, buf.len());
        if p.is_null() {
            None
        } else {
            Some(cstr_to_string(buf.as_ptr()))
        }
    }
}

pub fn stdin_is_tty() -> bool {
    unsafe { libc::isatty(0) == 1 }
}

pub fn stdout_is_tty() -> bool {
    unsafe { libc::isatty(1) == 1 }
}

pub fn interactive() -> bool {
    stdin_is_tty() && stdout_is_tty()
}

pub struct Term {
    orig: libc::termios,
}

pub fn term_init() -> Term {
    unsafe {
        let mut t: libc::termios = core::mem::zeroed();
        libc::tcgetattr(0, &mut t);
        let orig = t;
        t.c_iflag &= !(libc::IGNBRK | libc::BRKINT | libc::PARMRK | libc::ISTRIP | libc::INLCR | libc::IGNCR | libc::ICRNL | libc::IXON);
        t.c_oflag &= !libc::OPOST;
        t.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
        t.c_cflag &= !(libc::CSIZE | libc::PARENB);
        t.c_cflag |= libc::CS8;
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
        libc::tcsetattr(0, libc::TCSANOW, &t);
        Term { orig }
    }
}

pub fn term_restore(t: &Term) {
    unsafe {
        libc::tcsetattr(0, libc::TCSANOW, &t.orig);
    }
}

pub fn term_size() -> (u16, u16) {
    unsafe {
        let mut ws: libc::winsize = core::mem::zeroed();
        if libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            (ws.ws_col, ws.ws_row)
        } else {
            (80, 24)
        }
    }
}

fn read_byte() -> Option<u8> {
    let mut b = [0u8; 1];
    let n = unsafe { libc::read(0, b.as_mut_ptr() as *mut c_void, 1) };
    if n == 1 { Some(b[0]) } else { None }
}

fn has_input(timeout_ms: c_int) -> bool {
    let mut pfd = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    let r = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    r > 0 && (pfd.revents & libc::POLLIN) != 0
}

pub fn cursor_pos() -> (u16, u16) {
    write_stdout(b"\x1b[6n");
    let mut buf = [0u8; 32];
    let mut n = 0usize;
    while n < buf.len() {
        match read_byte() {
            Some(c) => {
                buf[n] = c;
                n += 1;
                if c == b'R' {
                    break;
                }
            }
            None => break,
        }
    }
    // Parse ESC [ row ; col R
    let mut row: u16 = 1;
    let mut col: u16 = 1;
    let mut i = 0;
    while i < n && buf[i] != b'[' {
        i += 1;
    }
    i += 1;
    let mut acc: u16 = 0;
    let mut have_row = false;
    while i < n {
        let c = buf[i];
        if c.is_ascii_digit() {
            acc = acc.saturating_mul(10).saturating_add((c - b'0') as u16);
        } else if c == b';' {
            row = acc;
            acc = 0;
            have_row = true;
        } else if c == b'R' {
            if have_row {
                col = acc;
            }
            break;
        }
        i += 1;
    }
    (col.saturating_sub(1), row.saturating_sub(1))
}

fn decode_utf8(first: u8) -> Key {
    let len = if first >= 0xF0 {
        4
    } else if first >= 0xE0 {
        3
    } else if first >= 0xC0 {
        2
    } else {
        1
    };
    let mut bytes = [first, 0, 0, 0];
    for b in bytes.iter_mut().take(len).skip(1) {
        *b = read_byte().unwrap_or(0);
    }
    match core::str::from_utf8(&bytes[..len]) {
        Ok(s) => match s.chars().next() {
            Some(c) => Key::Char(c),
            None => Key::Other,
        },
        Err(_) => Key::Other,
    }
}

pub fn read_key() -> Key {
    let b = match read_byte() {
        Some(b) => b,
        None => return Key::Other,
    };
    match b {
        0x1b => {
            if !has_input(50) {
                return Key::Esc;
            }
            let b1 = match read_byte() {
                Some(x) => x,
                None => return Key::Esc,
            };
            if b1 == b'[' || b1 == b'O' {
                let b2 = match read_byte() {
                    Some(x) => x,
                    None => return Key::Esc,
                };
                match b2 {
                    b'A' => Key::Up,
                    b'B' => Key::Down,
                    b'C' => Key::Right,
                    b'D' => Key::Left,
                    b'0'..=b'9' => {
                        // Extended sequence like ESC[3~; consume to the terminator.
                        let mut last = b2;
                        while last != b'~' {
                            match read_byte() {
                                Some(x) => last = x,
                                None => break,
                            }
                        }
                        Key::Other
                    }
                    _ => Key::Other,
                }
            } else {
                Key::Esc
            }
        }
        b'\r' | b'\n' => Key::Enter,
        0x7f | 0x08 => Key::Backspace,
        b'\t' => Key::Tab,
        0x01..=0x1a => Key::Ctrl((b - 1 + b'a') as char),
        _ => decode_utf8(b),
    }
}

struct LibcHeap;

unsafe impl GlobalAlloc for LibcHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { libc::malloc(layout.size()) as *mut u8 }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { libc::free(ptr as *mut c_void) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { libc::realloc(ptr as *mut c_void, new_size) as *mut u8 }
    }
}

#[cfg(not(test))]
#[global_allocator]
static ALLOC: LibcHeap = LibcHeap;
