//! SDK state queries: thin, safe wrappers over the [`crate::sys`] table.

use std::sync::OnceLock;

use crate::sys::sys;
use crate::types::OffsetTable;

/// Check if the unrealsdk is initialized.
pub fn is_initialized() -> bool {
    let Some(s) = sys() else { return false };
    // SAFETY: resolved export.
    unsafe { (s.is_initialized)() }
}

/// Check if the ingame console is ready.
pub fn is_console_ready() -> bool {
    let Some(s) = sys() else { return false };
    // SAFETY: resolved export.
    unsafe { (s.is_console_ready)() }
}

/// The loaded SDK's `get_version` word (`major << 16 | minor << 8 | patch`;
/// `version.cpp`). `None` off-game or on a build without the export.
pub fn sdk_version() -> Option<u32> {
    let s = sys()?;
    // SAFETY: resolved export.
    Some(unsafe { (s.get_version?)() })
}

/// The loaded SDK's human-readable version (`get_version_str`, e.g.
/// `"unrealsdk v3.2.0 (abcdef12)"`). `None` off-game, on a build without
/// the export, or on a null return.
pub fn sdk_version_string() -> Option<String> {
    let s = sys()?;
    // SAFETY: resolved export; the returned string is a static `std::string`
    // owned by the SDK (version.cpp) — copy it out immediately.
    let ptr = unsafe { (s.get_version_str?)() };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: NUL-terminated C string from the SDK.
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    Some(cstr.to_string_lossy().into_owned())
}

/// Get the populated `OffsetList` of the unrealsdk.
pub fn load_offsets() -> Option<OffsetTable> {
    static CACHED: OnceLock<Option<OffsetTable>> = OnceLock::new();
    *CACHED.get_or_init(|| {
        let s = sys()?;
        let ptr = unsafe { (s.get_offsets)() };
        if ptr.is_null() {
            return None;
        }
        Some(OffsetTable::from_list(unsafe { *ptr }))
    })
}
