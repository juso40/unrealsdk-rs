//! Engine-allocator RAII guards over `u_malloc`/`u_free`.

use std::ffi::c_void;

use crate::sys::sys;

use super::text::decode_wide;

// ---------------------------------------------------------------------------
// Unreal allocator guards
// ---------------------------------------------------------------------------

/// `u_malloc`'d buffer (`unrealsdk.h`; memory is zeroed by contract).
/// Frees with `u_free` on drop; a missing SDK leaks rather than crashes.
pub struct UBuffer {
    pub(crate) ptr: *mut c_void,
    pub(crate) len: usize,
}

impl UBuffer {
    /// Allocate `len` zeroed bytes. `None` for `len == 0`.
    pub fn alloc(len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }
        let s = sys()?;
        // SAFETY: resolved export.
        let ptr = unsafe { (s.u_malloc)(len) };
        if ptr.is_null() {
            return None;
        }
        Some(Self { ptr, len })
    }

    pub fn as_mut_ptr(&self) -> *mut c_void {
        self.ptr
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` for the impossible zero-length buffer ([`UBuffer::alloc`]
    /// rejects `len == 0`).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Drop for UBuffer {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        if let Some(s) = sys() {
            // SAFETY: pointer came from `u_malloc`.
            unsafe { (s.u_free)(self.ptr) };
        }
    }
}

/// Buffer returned by `uobject_path_name`/`ffield_path_name`
/// (`unrealsdk_main.cpp`): NUL-terminated wide text the caller must
/// `u_free`. Same drop discipline as [`UBuffer`].
pub struct UFreeString {
    pub(crate) ptr: *mut u16,
    pub(crate) len: usize,
}

impl UFreeString {
    /// The text, decoded lossy. `len` excludes the terminator.
    pub fn to_string(&self) -> Option<String> {
        decode_wide(self.ptr as *const u16, self.len)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Drop for UFreeString {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        if let Some(s) = sys() {
            // SAFETY: pointer came from a `u_malloc`-backed path-name export.
            unsafe { (s.u_free)(self.ptr as *mut c_void) };
        }
    }
}
