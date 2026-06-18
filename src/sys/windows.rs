//! Windows backend: talks to the OS through kernel32 (+shell32 for argv) only —
//! no C runtime. Provides files, env, terminal raw mode, sizing, key input, and
//! a HeapAlloc-backed global allocator.

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::Key;

type Handle = *mut c_void;
type Bool = i32;
type Dword = u32;
type Word = u16;

const STD_INPUT_HANDLE: Dword = -10i32 as Dword;
const STD_OUTPUT_HANDLE: Dword = -11i32 as Dword;
const INVALID_HANDLE_VALUE: isize = -1;

const GENERIC_READ: Dword = 0x8000_0000;
const GENERIC_WRITE: Dword = 0x4000_0000;
const FILE_SHARE_READ: Dword = 0x0000_0001;
const OPEN_EXISTING: Dword = 3;
const CREATE_ALWAYS: Dword = 2;
const FILE_ATTRIBUTE_NORMAL: Dword = 0x80;
const FILE_ATTRIBUTE_DIRECTORY: Dword = 0x10;
const INVALID_FILE_ATTRIBUTES: Dword = 0xFFFF_FFFF;

const ENABLE_PROCESSED_INPUT: Dword = 0x0001;
const ENABLE_LINE_INPUT: Dword = 0x0002;
const ENABLE_ECHO_INPUT: Dword = 0x0004;
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: Dword = 0x0004;
const CP_UTF8: u32 = 65001;

const KEY_EVENT: Word = 0x0001;
const LEFT_CTRL_PRESSED: Dword = 0x0008;
const RIGHT_CTRL_PRESSED: Dword = 0x0004;

#[repr(C)]
#[derive(Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
struct ConsoleScreenBufferInfo {
    size: Coord,
    cursor: Coord,
    attributes: Word,
    window: SmallRect,
    max_window: Coord,
}

#[repr(C)]
struct SystemTime {
    year: Word,
    month: Word,
    day_of_week: Word,
    day: Word,
    hour: Word,
    minute: Word,
    second: Word,
    milliseconds: Word,
}

#[repr(C)]
struct KeyEventRecord {
    key_down: Bool,
    repeat_count: Word,
    virtual_key_code: Word,
    virtual_scan_code: Word,
    unicode_char: Word,
    control_key_state: Dword,
}

#[repr(C)]
struct InputRecord {
    event_type: Word,
    _pad: Word,
    key: KeyEventRecord,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(n: Dword) -> Handle;
    fn WriteFile(h: Handle, buf: *const u8, len: Dword, written: *mut Dword, ovl: *mut c_void) -> Bool;
    fn ReadFile(h: Handle, buf: *mut u8, len: Dword, read: *mut Dword, ovl: *mut c_void) -> Bool;
    fn CreateFileW(name: *const u16, access: Dword, share: Dword, sec: *mut c_void, disp: Dword, flags: Dword, template: Handle) -> Handle;
    fn CloseHandle(h: Handle) -> Bool;
    fn GetFileSizeEx(h: Handle, size: *mut i64) -> Bool;
    fn GetFileAttributesW(name: *const u16) -> Dword;
    fn GetLocalTime(time: *mut SystemTime);
    fn GetCommandLineW() -> *const u16;
    fn GetEnvironmentVariableW(name: *const u16, buf: *mut u16, size: Dword) -> Dword;
    fn GetCurrentDirectoryW(size: Dword, buf: *mut u16) -> Dword;
    fn GetConsoleMode(h: Handle, mode: *mut Dword) -> Bool;
    fn SetConsoleMode(h: Handle, mode: Dword) -> Bool;
    fn GetConsoleOutputCP() -> u32;
    fn SetConsoleOutputCP(cp: u32) -> Bool;
    fn GetConsoleScreenBufferInfo(h: Handle, info: *mut ConsoleScreenBufferInfo) -> Bool;
    fn ReadConsoleInputW(h: Handle, buf: *mut InputRecord, len: Dword, read: *mut Dword) -> Bool;
    fn ExitProcess(code: u32) -> !;
    fn GetProcessHeap() -> Handle;
    fn HeapAlloc(heap: Handle, flags: Dword, bytes: usize) -> *mut c_void;
    fn HeapReAlloc(heap: Handle, flags: Dword, mem: *mut c_void, bytes: usize) -> *mut c_void;
    fn HeapFree(heap: Handle, flags: Dword, mem: *mut c_void) -> Bool;
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn CommandLineToArgvW(cmd: *const u16, argc: *mut i32) -> *mut *mut u16;
}

fn to_wide(s: &str) -> Vec<u16> {
    let mut w: Vec<u16> = s.encode_utf16().collect();
    w.push(0);
    w
}

unsafe fn wide_to_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = core::slice::from_raw_parts(ptr, len);
        char::decode_utf16(slice.iter().copied())
            .map(|r| r.unwrap_or('\u{fffd}'))
            .collect()
    }
}

pub fn write_stdout(bytes: &[u8]) {
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut written: Dword = 0;
        WriteFile(h, bytes.as_ptr(), bytes.len() as Dword, &mut written, core::ptr::null_mut());
    }
}

pub fn exit(code: i32) -> ! {
    unsafe { ExitProcess(code as u32) }
}

pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let w = to_wide(path);
    unsafe {
        let h = CreateFileW(
            w.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ,
            core::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        );
        if h as isize == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut size: i64 = 0;
        if GetFileSizeEx(h, &mut size) == 0 || size < 0 {
            CloseHandle(h);
            return None;
        }
        let mut buf: Vec<u8> = vec![0u8; size as usize];
        let mut read: Dword = 0;
        let ok = ReadFile(h, buf.as_mut_ptr(), size as Dword, &mut read, core::ptr::null_mut());
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        buf.truncate(read as usize);
        Some(buf)
    }
}

pub fn write_file(path: &str, data: &[u8]) -> bool {
    let w = to_wide(path);
    unsafe {
        let h = CreateFileW(
            w.as_ptr(),
            GENERIC_WRITE,
            0,
            core::ptr::null_mut(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        );
        if h as isize == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut written: Dword = 0;
        let ok = WriteFile(h, data.as_ptr(), data.len() as Dword, &mut written, core::ptr::null_mut());
        CloseHandle(h);
        ok != 0 && written as usize == data.len()
    }
}

pub fn is_dir(path: &str) -> bool {
    let w = to_wide(path);
    unsafe {
        let attr = GetFileAttributesW(w.as_ptr());
        attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY) != 0
    }
}

pub fn exists(path: &str) -> bool {
    let w = to_wide(path);
    unsafe { GetFileAttributesW(w.as_ptr()) != INVALID_FILE_ATTRIBUTES }
}

/// Local date as (year, month, day).
pub fn today() -> (u16, u8, u8) {
    unsafe {
        let mut st: SystemTime = core::mem::zeroed();
        GetLocalTime(&mut st);
        (st.year, st.month as u8, st.day as u8)
    }
}

pub fn args() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let cmd = GetCommandLineW();
        let mut argc: i32 = 0;
        let argv = CommandLineToArgvW(cmd, &mut argc);
        if argv.is_null() {
            return out;
        }
        for i in 0..argc as isize {
            out.push(wide_to_string(*argv.offset(i)));
        }
        LocalFree(argv as *mut c_void);
    }
    out
}

pub fn env_var(name: &str) -> Option<String> {
    let w = to_wide(name);
    unsafe {
        let needed = GetEnvironmentVariableW(w.as_ptr(), core::ptr::null_mut(), 0);
        if needed == 0 {
            return None;
        }
        let mut buf: Vec<u16> = vec![0u16; needed as usize];
        let got = GetEnvironmentVariableW(w.as_ptr(), buf.as_mut_ptr(), needed);
        if got == 0 {
            return None;
        }
        Some(wide_to_string(buf.as_ptr()))
    }
}

pub fn cwd() -> Option<String> {
    unsafe {
        let needed = GetCurrentDirectoryW(0, core::ptr::null_mut());
        if needed == 0 {
            return None;
        }
        let mut buf: Vec<u16> = vec![0u16; needed as usize];
        let got = GetCurrentDirectoryW(needed, buf.as_mut_ptr());
        if got == 0 {
            return None;
        }
        Some(wide_to_string(buf.as_ptr()))
    }
}

/// Saved console state; restored on `term_restore`.
pub struct Term {
    stdin: Handle,
    stdout: Handle,
    in_mode: Dword,
    out_mode: Dword,
    out_cp: u32,
}

pub fn stdin_is_tty() -> bool {
    unsafe {
        let mut mode: Dword = 0;
        GetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), &mut mode) != 0
    }
}

pub fn stdout_is_tty() -> bool {
    unsafe {
        let mut mode: Dword = 0;
        GetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), &mut mode) != 0
    }
}

pub fn interactive() -> bool {
    stdin_is_tty() && stdout_is_tty()
}

pub fn term_init() -> Term {
    unsafe {
        let stdin = GetStdHandle(STD_INPUT_HANDLE);
        let stdout = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut in_mode: Dword = 0;
        let mut out_mode: Dword = 0;
        GetConsoleMode(stdin, &mut in_mode);
        GetConsoleMode(stdout, &mut out_mode);
        let out_cp = GetConsoleOutputCP();
        let raw_in = in_mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
        SetConsoleMode(stdin, raw_in);
        SetConsoleMode(stdout, out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        SetConsoleOutputCP(CP_UTF8);
        Term { stdin, stdout, in_mode, out_mode, out_cp }
    }
}

pub fn term_restore(t: &Term) {
    unsafe {
        SetConsoleMode(t.stdin, t.in_mode);
        SetConsoleMode(t.stdout, t.out_mode);
        SetConsoleOutputCP(t.out_cp);
    }
}

fn screen_info() -> Option<ConsoleScreenBufferInfo> {
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut info: ConsoleScreenBufferInfo = core::mem::zeroed();
        if GetConsoleScreenBufferInfo(h, &mut info) == 0 {
            None
        } else {
            Some(info)
        }
    }
}

pub fn term_size() -> (u16, u16) {
    match screen_info() {
        Some(i) => {
            let w = (i.window.right - i.window.left + 1).max(1) as u16;
            let h = (i.window.bottom - i.window.top + 1).max(1) as u16;
            (w, h)
        }
        None => (80, 24),
    }
}

pub fn cursor_pos() -> (u16, u16) {
    match screen_info() {
        Some(i) => (i.cursor.x.max(0) as u16, i.cursor.y.max(0) as u16),
        None => (0, 0),
    }
}

pub fn read_key() -> Key {
    unsafe {
        let h = GetStdHandle(STD_INPUT_HANDLE);
        loop {
            let mut rec: InputRecord = core::mem::zeroed();
            let mut read: Dword = 0;
            if ReadConsoleInputW(h, &mut rec, 1, &mut read) == 0 || read == 0 {
                return Key::Other;
            }
            if rec.event_type != KEY_EVENT || rec.key.key_down == 0 {
                continue;
            }
            let vk = rec.key.virtual_key_code;
            let ctrl = rec.key.control_key_state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0;
            match vk {
                0x26 => return Key::Up,
                0x28 => return Key::Down,
                0x25 => return Key::Left,
                0x27 => return Key::Right,
                0x0D => return Key::Enter,
                0x1B => return Key::Esc,
                0x09 => return Key::Tab,
                0x08 => return Key::Backspace,
                _ => {
                    if ctrl {
                        if (0x41..=0x5A).contains(&vk) {
                            return Key::Ctrl((vk as u8 as char).to_ascii_lowercase());
                        }
                        continue;
                    }
                    let uc = rec.key.unicode_char;
                    if uc >= 0x20 {
                        if let Some(c) = char::from_u32(uc as u32) {
                            return Key::Char(c);
                        }
                    }
                    continue;
                }
            }
        }
    }
}

struct WinHeap;

unsafe impl GlobalAlloc for WinHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // HeapAlloc guarantees alignment suitable for any built-in type (16 bytes
        // on 64-bit), which covers every allocation this program makes.
        unsafe { HeapAlloc(GetProcessHeap(), 0, layout.size()) as *mut u8 }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe {
            HeapFree(GetProcessHeap(), 0, ptr as *mut c_void);
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { HeapReAlloc(GetProcessHeap(), 0, ptr as *mut c_void, new_size) as *mut u8 }
    }
}

#[cfg(not(test))]
#[global_allocator]
static ALLOC: WinHeap = WinHeap;
