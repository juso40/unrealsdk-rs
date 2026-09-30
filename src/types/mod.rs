//! `repr(C)` replicas of the SDK's UE3 types, plus the offset table.
//!
//! Two kinds of types live here:
//!
//! 1. **Static layout** — identical on every game build: [`FName`]
//!    (`fname.h`), [`TArray`] (`tarray.h`), the WILLOW [`GObjects`]
//!    wrapper (`wrappers/gobjects.h`, whose `internal` is a
//!    `TArray<UObject*>*` because `GOBJECTS_FORMAT == TARRAY` on WILLOW,
//!    `flavour.h`), [`LogMessage`](crate::logging::LogMessage)
//!    (`logging.h`), [`FText`]
//!    (`ftext.h`; WILLOW `FTEXT_FORMAT` is `NOT_IMPLEMENTED`,
//!    `flavour.h`, so `data` is a raw `FTextData*`), [`BoundFunction`]
//!    (`bound_function.h`), [`PropertyProxy`] (`property_proxy.h`)
//!    over [`UnrealPointer`] (`unreal_pointer.h`), [`WrappedStruct`]
//!    (`wrapped_struct.h`) and hook [`Details`] (`hook_manager.h`).
//!
//! 2. **Version-dependent layout** — every `UObject`-derived class, `FField`,
//!    `FFrame` and `FNameEntry`. Their fields move between game builds, so
//!    Rust holds only an opaque handle (the leading `vftable` pointer, which
//!    is always first) and reads everything else
//!    through [`OffsetTable`], copied once from `get_offsets()`
//!    (`unrealsdk_main.cpp`).
//!    Any member access that is not a static-layout read above
//!    goes through the table.
//!
//! WILLOW packing (`flavour.h`, `pack(push, 0x4)`) is a no-op on the
//! i686 target: no field here needs more than 4-byte alignment (Rust aligns
//! `u64` to 4 on 32-bit x86, matching MSVC+pack(4)).
//!
//! [`PropertyProxy`]/[`WrappedStruct`] own C++ RAII memory (`UnrealPointer`
//! control blocks, `unreal_pointer.h`). Rust must **never construct,
//! move or drop** one — only borrow the fields of live SDK-owned instances
//! (hook `Details`, function args). Treating them as POD is the load-bearing
//! rule of this module.

mod handles;
mod offsets;
mod structs;

pub use handles::*;
pub use offsets::*;
pub use structs::*;

pub(crate) use offsets::{
    read_field_fname, read_field_i32, read_field_ptr, read_field_u16, read_field_u32,
    read_field_u64, write_field_u64,
};
