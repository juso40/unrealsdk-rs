//! Text codecs: wide strings, `FName`s and `FText` writes.

use std::ffi::c_void;

use crate::sys::sys;
use crate::types::FName;

// ---------------------------------------------------------------------------
// Wide-string codec (pure; pointer+size convention, no NUL needed)
// ---------------------------------------------------------------------------

/// Encode `&str` as UTF-16 units for a `*const u16 + size` call. No NUL
/// terminator: callers pass `.as_ptr()`/`.len()` exactly like the SDK's own
/// C++ wrappers do (`hook_manager.cpp`).
pub fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Decode `len` UTF-16 units at `ptr` (lossy). `None` on null pointer.
/// `len` counts **wchar_t units, not bytes**.
pub fn decode_wide(ptr: *const u16, len: usize) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    if len == 0 {
        return Some(String::new());
    }
    // SAFETY: caller guarantees `len` readable units; lossy decode cannot fail.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(String::from_utf16_lossy(slice))
}


// ---------------------------------------------------------------------------
// FName helpers
// ---------------------------------------------------------------------------

/// `fname_init` (`unrealsdk_main.cpp` + `unrealsdk.h`: the input
/// must be NUL-terminated, so one is pushed — unlike the pointer+size calls).
pub fn fname_from_str(s: &str, number: u32) -> Option<FName> {
    let sys_ref = sys()?;
    let mut wide = encode_wide(s);
    wide.push(0);
    let mut name = FName::default();
    // SAFETY: resolved export; `wide` is NUL-terminated and alive for the call.
    unsafe { (sys_ref.fname_init)(&mut name, wide.as_ptr(), number) };
    Some(name)
}

/// Decoded text of an `FName` (`fname_get_str`, `unrealsdk_main.cpp`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FNameText {
    /// True when the SDK returned wide units, false for narrow bytes.
    pub is_wide: bool,
    pub text: String,
}

/// `fname_get_str`: `None` on null data or off-game.
pub fn fname_to_string(name: &FName) -> Option<FNameText> {
    let s = sys()?;
    let mut ptr: *const c_void = std::ptr::null();
    let mut size: usize = 0;
    let mut is_wide = false;
    // SAFETY: resolved export; out-params are stack-local.
    unsafe { (s.fname_get_str)(*name, &mut ptr, &mut size, &mut is_wide) };
    if ptr.is_null() {
        return None;
    }
    if is_wide {
        return Some(FNameText {
            is_wide: true,
            text: decode_wide(ptr as *const u16, size)?,
        });
    }
    // SAFETY: `size` bytes readable per the export contract.
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) };
    Some(FNameText {
        is_wide: false,
        text: String::from_utf8_lossy(bytes).into_owned(),
    })
}


/// `ftext_as_culture_invariant` (`unrealsdk_main.cpp`). `text` must
/// point at a live `FText`.
pub fn ftext_from_str(text: *mut crate::types::FText, s: &str) -> bool {
    if text.is_null() {
        return false;
    }
    let Some(sys_ref) = sys() else { return false };
    let wide = encode_wide(s);
    // SAFETY: resolved export; `wide` alive for the call.
    unsafe { (sys_ref.ftext_as_culture_invariant)(text, wide.as_ptr(), wide.len()) };
    true
}
