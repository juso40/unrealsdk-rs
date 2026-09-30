//! Typed raw access to live engine memory.
//!
//! The layer under [`crate::objects`]: offset-typed pointer reads/writes for
//! the structs the dynamic offset table describes only by example and the
//! script mirrors cover otherwise (see [`crate::types`]). Call sites pass
//! an [`Offset`] — from the table ([`Offset::from_table`]) or a reflected
//! property ([`Offset::from_reflected`]) — instead of a bare integer that a
//! narrowing cast silently truncates.
//!
//! Two widths matter: [`GAME_PTR`] is the game ABI's pointer size
//! (i686 on the shipped builds: 4 bytes), while the
//! games `INT`/`DWORD` are [`GAME_WORD`] at every target width. An
//! [`Offset`] addresses a byte in live engine memory.

use std::ffi::c_void;

/// Pointer size of the game ABI (i686: 4 bytes). Struct fields spaced by
/// a pointer (`TArray::data`, delegate objects) sit one [`GAME_PTR`] apart,
/// never one [`GAME_WORD`].
pub const GAME_PTR: usize = size_of::<*mut c_void>();

/// Word size of the game ABI's scalars: `INT`/`DWORD`/`UBOOL` are 4 bytes
/// at every target width (UE3 `INT` is `int32_t` even on win64).
pub const GAME_WORD: usize = 4;

/// Byte offset into a live engine struct.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset(usize);

impl Offset {
    /// A raw byte offset (layout constants).
    pub const fn bytes(raw: usize) -> Self {
        Self(raw)
    }

    /// An offset from the runtime offset table
    /// (`offset_type = u16`, `offsets.h`).
    pub const fn from_table(raw: u16) -> Self {
        Self(raw as usize)
    }

    /// A reflected `UProperty::Offset`.
    pub fn from_reflected(raw: i32) -> Option<Self> {
        usize::try_from(raw).ok().map(Self)
    }

    /// The offset as a byte count.
    pub const fn raw(self) -> usize {
        self.0
    }

    /// A further byte displacement (element fields, indexed records).
    pub const fn plus(self, extra: usize) -> Self {
        Self(self.0 + extra)
    }
}

// Reads are null-tolerant (null base yields 0/null).
// Writes require a live target.

/// Read a pointer-sized field. Null base → null.
///
/// # Safety
/// `base + off` must point at live engine memory for the duration of the
/// read (or `base` is null).
pub unsafe fn read_ptr(base: *const c_void, off: Offset) -> *mut c_void {
    if base.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { *(base.byte_add(off.raw()) as *const *mut c_void) }
}

/// Read a signed 32-bit field. Null base → 0.
///
/// # Safety
/// `base + off` must point at live engine memory for the duration of the
/// read (or `base` is null).
pub unsafe fn read_i32(base: *const c_void, off: Offset) -> i32 {
    if base.is_null() {
        return 0;
    }
    unsafe { *(base.byte_add(off.raw()) as *const i32) }
}

/// Read an unsigned 32-bit field. Null base → 0.
///
/// # Safety
/// `base + off` must point at live engine memory for the duration of the
/// read (or `base` is null).
pub unsafe fn read_u32(base: *const c_void, off: Offset) -> u32 {
    if base.is_null() {
        return 0;
    }
    unsafe { *(base.byte_add(off.raw()) as *const u32) }
}

/// Write a pointer-sized field.
///
/// # Safety
/// `base + off` must point at writable engine memory for the duration of
/// the write.
pub unsafe fn write_ptr(base: *mut c_void, off: Offset, value: *mut c_void) {
    unsafe { *(base.byte_add(off.raw()) as *mut *mut c_void) = value };
}

/// Write a signed 32-bit field.
///
/// # Safety
/// `base + off` must point at writable engine memory for the duration of
/// the write.
pub unsafe fn write_i32(base: *mut c_void, off: Offset, value: i32) {
    unsafe { *(base.byte_add(off.raw()) as *mut i32) = value };
}

/// Read-modify-write an unsigned 32-bit field.
///
/// # Safety
/// `base + off` must point at writable engine memory for the duration of
/// the read-modify-write.
pub unsafe fn write_u32(base: *mut c_void, off: Offset, value: u32) {
    unsafe { *(base.byte_add(off.raw()) as *mut u32) = value };
}
