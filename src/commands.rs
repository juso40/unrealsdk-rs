//! Safe console-command wrappers (`commands.cpp`).
//!
//! Commands match the first whitespace-delimited block of a console line,
//! case-insensitively (`commands.cpp`); names are lowercased by the SDK
//! on registration. Callbacks are the same `DLLSafeCallback` pseudo-vtable
//! construction as hooks (see [`crate::hooks` for the handshake]), but over
//! the command signature `void(const wchar_t*, size_t, size_t)` — line,
//! line length, matched-command length (`commands.h`).
//!
//! The `NEXT_LINE` one-shot (`commands.h`, `commands.cpp`) is
//! intentionally not wrapped: it hijacks the next console line globally.
//!
//! [`crate::hooks` for the handshake]: crate::hooks

use std::ffi::c_void;

use crate::error::{Error, Result};
use crate::objects::{decode_wide, encode_wide};
use crate::sys::sys;

/// What triggered a command callback: the full submitted line plus the
/// length of the matched command (`line[cmd_len]` is the first whitespace
/// after the command, `commands.h`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInvocation {
    pub line: String,
    pub cmd_len: usize,
}

// ---------------------------------------------------------------------------
// Pseudo-vtable machinery (command signature; same ownership handshake as hooks)
// ---------------------------------------------------------------------------

#[repr(C)]
struct CmdVTable {
    destroy: unsafe extern "C" fn(this: *mut CmdCallback),
    call:
        unsafe extern "C" fn(this: *mut CmdCallback, line: *const u16, size: usize, cmd_len: usize),
}

#[repr(C)]
struct CmdCallback {
    vftable: *const CmdVTable,
    user: *mut CmdCell,
}

const _: () = assert!(size_of::<CmdCallback>() == 2 * size_of::<*mut c_void>());
const _: () = assert!(size_of::<CmdVTable>() == 2 * size_of::<*mut c_void>());

struct CmdCell {
    cb: Box<dyn Fn(CommandInvocation) + Send>,
}

#[repr(C)]
struct CmdHolder {
    inner: *mut CmdCallback,
}

unsafe extern "C" fn destroy_shim(this: *mut CmdCallback) {
    if this.is_null() {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: built by `Box::new`, destroyed through this shim exactly
        // once (SDK `~DLLSafeCallback`, `utils.h`).
        unsafe {
            let raw = Box::from_raw(this);
            let _ = Box::from_raw(raw.user);
        }
    }));
}

unsafe extern "C" fn call_shim(
    this: *mut CmdCallback,
    line: *const u16,
    size: usize,
    cmd_len: usize,
) {
    if this.is_null() {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: live for the call (same vtable contract as hooks).
        let cell = unsafe { &*((*this).user as *const CmdCell) };
        let text = decode_wide(line, size).unwrap_or_default();
        (cell.cb)(CommandInvocation {
            line: text,
            cmd_len,
        });
    }));
}

static VTABLE: CmdVTable = CmdVTable {
    destroy: destroy_shim,
    call: call_shim,
};

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// A registered console command. Dropping it unregisters (`remove_command`);
/// removal is best-effort and `None`-safe.
#[derive(Debug)]
pub struct CommandHandle {
    cmd: Vec<u16>,
    active: bool,
}

impl CommandHandle {
    /// Unregister now; consumes the handle (Drop becomes a no-op).
    pub fn remove(mut self) -> bool {
        self.remove_inner()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    fn remove_inner(&mut self) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        let Some(s) = sys() else { return false };
        // SAFETY: resolved export; buffer alive for the call.
        unsafe { (s.remove_command)(self.cmd.as_ptr(), self.cmd.len()) }
    }
}

impl Drop for CommandHandle {
    fn drop(&mut self) {
        let _ = self.remove_inner();
    }
}

/// `add_command` (`commands.cpp`). Errors: [`Error::NotInGame`],
/// [`Error::EmptyName`], [`Error::Duplicate`], or a bare [`Error::Failed`].
/// The callback must never touch Python (it runs on the console/game
/// thread).
pub fn add_command(
    cmd: &str,
    cb: Box<dyn Fn(CommandInvocation) + Send>,
) -> Result<CommandHandle> {
    if cmd.is_empty() {
        return Err(Error::EmptyName("command name".to_owned()));
    }
    let Some(s) = sys() else {
        return Err(Error::NotInGame);
    };
    let cmd_wide = encode_wide(cmd);

    let cell = Box::new(CmdCell { cb });
    let raw = Box::into_raw(Box::new(CmdCallback {
        vftable: &VTABLE,
        user: Box::into_raw(cell),
    }));
    let holder = CmdHolder { inner: raw };
    // SAFETY: resolved export; `holder` address serves as the
    // `DLLSafeCallback&&`; buffer alive for the call.
    let added = unsafe {
        (s.add_command)(
            cmd_wide.as_ptr(),
            cmd_wide.len(),
            &holder as *const CmdHolder as *const c_void,
        )
    };
    if holder.inner.is_null() {
        if added {
            return Ok(CommandHandle {
                cmd: cmd_wide,
                active: true,
            });
        }
        return Err(Error::Failed("add_command failed".to_owned()));
    }
    // SAFETY: untouched by the callee; free our own boxes.
    unsafe {
        let raw = Box::from_raw(holder.inner);
        let _ = Box::from_raw(raw.user);
    }
    Err(Error::Duplicate(format!("command '{cmd}'")))
}

/// `has_command` (`commands.cpp`). `false` off-game.
pub fn has_command(cmd: &str) -> bool {
    let Some(s) = sys() else { return false };
    if cmd.is_empty() {
        return false;
    }
    let wide = encode_wide(cmd);
    // SAFETY: resolved export; buffer alive for the call.
    unsafe { (s.has_command)(wide.as_ptr(), wide.len()) }
}

/// `remove_command` (`commands.cpp`). `false` when absent or off-game.
pub fn remove_command(cmd: &str) -> bool {
    let Some(s) = sys() else { return false };
    if cmd.is_empty() {
        return false;
    }
    let wide = encode_wide(cmd);
    // SAFETY: resolved export; buffer alive for the call.
    unsafe { (s.remove_command)(wide.as_ptr(), wide.len()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_has_fail_gracefully_off_game() {
        assert_eq!(
            add_command("", Box::new(|_: CommandInvocation| {})).unwrap_err(),
            Error::EmptyName("command name".to_owned())
        );
        assert_eq!(
            add_command("cmd", Box::new(|_: CommandInvocation| {})).unwrap_err(),
            Error::NotInGame
        );
        assert!(!has_command("cmd"));
        assert!(!remove_command("cmd"));
    }
}
