//! Static-layout value types: identical on every game build.
//!
//! Each replica carries a compile-time size assertion against its SDK
//! header. See the [`types`](super) module docs for the layout discipline.

use std::ffi::c_void;

use super::handles::{UFunction, UObject, UStruct, ZProperty};

// ---------------------------------------------------------------------------
// Static-layout value types
// ---------------------------------------------------------------------------

/// `unreal::FName` (`structs/fname.h`): an index into the global name
/// table plus a disambiguating number. 8 bytes, passed by value across the
/// C API (`fname_get_str`, `unrealsdk_main.cpp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FName {
    pub index: u32,
    pub number: u32,
}

const _: () = assert!(size_of::<FName>() == 8);

/// `unreal::TArray<T>` (`structs/tarray.h`): `{data, count, max}`.
/// `UnmanagedFString` is `TArray<wchar_t>` (`structs/fstring.h`).
///
/// The header ops ([`TArray::read`] / [`TArray::write`]) always use the
/// *game* layout ([`crate::mem::GAME_WORD`] fields) even when the host's
/// `repr(C)` layout differs — game memory is always laid out by the game.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TArray<T> {
    pub data: *mut T,
    pub count: i32,
    pub max: i32,
}

impl<T> TArray<T> {
    /// The header's footprint in the game ABI (i686: three words).
    pub const GAME_SIZE: usize = crate::mem::GAME_PTR + 2 * crate::mem::GAME_WORD;

    /// Read a `TArray` header from live engine memory at `at` (game
    /// layout: `Data` at +0, `Count` one pointer in, `Max` one pointer
    /// plus one game word in).
    ///
    /// # Safety
    /// `at` must point at a live `TArray` header (or be null — a null base
    /// reads as an empty array).
    pub unsafe fn read(at: *const c_void) -> Self {
        use crate::mem::{read_i32, read_ptr, Offset, GAME_PTR, GAME_WORD};
        unsafe {
            Self {
                data: read_ptr(at, Offset::bytes(0)) as *mut T,
                count: read_i32(at, Offset::bytes(GAME_PTR)),
                max: read_i32(at, Offset::bytes(GAME_PTR + GAME_WORD)),
            }
        }
    }

    /// Stamp a fresh header (`Max = Count`) at `at` — handing a new array
    /// to the engine.
    ///
    /// # Safety
    /// `at` must point at a writable engine `TArray` header
    /// ([`Self::GAME_SIZE`] game bytes); `data` must hold `count` live `T`s
    /// for as long as the engine keeps the array.
    pub unsafe fn write(at: *mut c_void, data: *mut T, count: i32) {
        use crate::mem::{write_i32, write_ptr, Offset, GAME_PTR, GAME_WORD};
        unsafe {
            write_ptr(at, Offset::bytes(0), data as *mut c_void);
            write_i32(at, Offset::bytes(GAME_PTR), count);
            write_i32(at, Offset::bytes(GAME_PTR + GAME_WORD), count);
        }
    }

    /// Read element `i` (copy). For an indirect array (`T` = pointer) this
    /// is the element pointer.
    ///
    /// # Safety
    /// `self.data` must hold `self.count` `T`s; `i < self.count`.
    pub unsafe fn element(&self, i: usize) -> T
    where
        T: Copy,
    {
        unsafe { *self.data.add(i) }
    }

    /// Address of record `i` of a fixed-`stride` record array (records
    /// wider than `T`, e.g. `{FName, void*}` table entries).
    ///
    /// # Safety
    /// `self.data` must hold `self.count` records of `stride` bytes each;
    /// `i < self.count`.
    pub unsafe fn record(&self, i: usize, stride: usize) -> *mut c_void {
        unsafe { (self.data as *mut u8).byte_add(i * stride) as *mut c_void }
    }
}

const _: () = assert!(size_of::<TArray<u8>>() == size_of::<usize>() + 8);

/// Engine-owned wide string (`structs/fstring.h`). Read-only from Rust:
/// never realloc, never free — the engine owns the buffer.
pub type UnmanagedFString = TArray<u16>;

/// `unreal::FWeakObjectPtr` (`structs/fweakobjectptr.h`).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FWeakObjectPtr {
    pub object_index: i32,
    pub object_serial_number: i32,
}

const _: () = assert!(size_of::<FWeakObjectPtr>() == 8);

/// WILLOW `GObjects` wrapper (`wrappers/gobjects.h`): a single
/// `internal` pointer to the game's `TArray<UObject*>` (`flavour.h`).
/// The export returns a pointer to the SDK-owned wrapper
/// (`unrealsdk_main.cpp`); Rust reads through it, never writes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GObjects {
    pub internal: *const TArray<*mut UObject>,
}

const _: () = assert!(size_of::<GObjects>() == size_of::<usize>());


/// Opaque `FTextData` (`structs/ftext.h`): only the address crosses.
#[repr(C)]
pub struct FTextData {
    pub vftable: *mut usize,
}

/// `unreal::FText` on WILLOW (`structs/ftext.h` with
/// `FTEXT_FORMAT_NOT_IMPLEMENTED`, `flavour.h`): `{FTextData* data;
/// uint32_t flags}`. Opaque contents; initialise via
/// `ftext_as_culture_invariant`, never by field writes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FText {
    pub data: *mut FTextData,
    pub flags: u32,
}

#[cfg(target_pointer_width = "32")]
const _: () = assert!(size_of::<FText>() == 8);

/// `unreal::BoundFunction` (`wrappers/bound_function.h`):
/// `{UFunction* func; UObject* object}` — plain pointers, safe to copy.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BoundFunction {
    pub func: *mut UFunction,
    pub object: *mut UObject,
}

const _: () = assert!(size_of::<BoundFunction>() == 2 * size_of::<usize>());

/// `unreal::UnrealPointer<T>` (`wrappers/unreal_pointer.h`):
/// `{control, ptr}`. The control block runs C++ virtual refcounting
/// (`unreal_pointer.h`); Rust treats this as an 8-byte borrowed view
/// and never touches `control`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UnrealPointer {
    pub control: *mut c_void,
    pub ptr: *mut c_void,
}

const _: () = assert!(size_of::<UnrealPointer>() == 2 * size_of::<usize>());

/// `unreal::PropertyProxy` (`wrappers/property_proxy.h`):
/// `{ZProperty* prop; UnrealPointer<void> ptr}`. Borrowed view only (see the
/// module docs); reading `prop` is a plain pointer read and always safe.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PropertyProxy {
    pub prop: *mut ZProperty,
    pub ptr: UnrealPointer,
}

const _: () = assert!(size_of::<PropertyProxy>() == 3 * size_of::<usize>());

/// `unreal::WrappedStruct` (`wrappers/wrapped_struct.h`):
/// `{const UStruct* type; UnrealPointer<void> base}`. Borrowed view only.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WrappedStruct {
    pub ty: *const UStruct,
    pub base: UnrealPointer,
}

const _: () = assert!(size_of::<WrappedStruct>() == 3 * size_of::<usize>());

/// `hook_manager::Details` (`hook_manager.h`):
/// `{obj, args, ret, func}` = 4+4+12+8 = 28 bytes on i686. Borrowed by hook
/// callbacks for the duration of the call only.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Details {
    pub obj: *mut UObject,
    pub args: *mut WrappedStruct,
    pub ret: PropertyProxy,
    pub func: BoundFunction,
}

// 7 pointers on every width (obj, args, ret{prop,control,ptr},
// func{func,object}); 28 bytes on i686.
const _: () = assert!(size_of::<Details>() == 7 * size_of::<usize>());
