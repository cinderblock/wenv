//! Freestanding runtime symbols the linker needs when there is no C runtime.
//!
//! On Windows the C runtime would normally provide the `mem*` intrinsics that
//! LLVM lowers bulk copies/compares to, plus the MSVC exception personality. On
//! Unix the system libc provides `mem*`, but the precompiled `core`/`alloc`
//! rlibs (built with `panic = "unwind"`) still reference the unwinding
//! personality. Under `panic = "abort"` no unwinding ever happens, so these only
//! satisfy the linker — they are never actually called.

#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        unsafe { *dest.add(i) = *src.add(i) };
        i += 1;
    }
    dest
}

#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dest as usize) < (src as usize) {
        let mut i = 0;
        while i < n {
            unsafe { *dest.add(i) = *src.add(i) };
            i += 1;
        }
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            unsafe { *dest.add(i) = *src.add(i) };
        }
    }
    dest
}

#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dest: *mut u8, c: i32, n: usize) -> *mut u8 {
    let byte = c as u8;
    let mut i = 0;
    while i < n {
        unsafe { *dest.add(i) = byte };
        i += 1;
    }
    dest
}

#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let x = unsafe { *a.add(i) };
        let y = unsafe { *b.add(i) };
        if x != y {
            return x as i32 - y as i32;
        }
        i += 1;
    }
    0
}

#[cfg(windows)]
#[unsafe(no_mangle)]
pub extern "C" fn __CxxFrameHandler3() -> i32 {
    0
}

#[cfg(unix)]
#[unsafe(no_mangle)]
pub extern "C" fn rust_eh_personality() {}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn _Unwind_Resume() -> ! {
    // Unreachable under panic = "abort"; present only to resolve the reference.
    loop {}
}
