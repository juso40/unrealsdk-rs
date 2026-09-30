//! Opaque version-dependent handles: the leading `vftable` pointer only.
//!
//! Their fields move between game builds, so Rust holds an opaque handle
//! (the `vftable` is always first: `uobject.h`) and reads everything
//! else through [`OffsetTable`](crate::types::OffsetTable).

// ---------------------------------------------------------------------------
// Opaque version-dependent handles (vftable first, tail via OffsetTable)
// ---------------------------------------------------------------------------

macro_rules! opaque_handle {
    ($(#[$meta:meta])* $name:ident, $header:literal) => {
        $(#[$meta])*
        #[repr(C)]
        pub struct $name {
            /// Always the first field (`$header`); the remainder of the
            /// object is version-dependent: use [`OffsetTable`](crate::types::OffsetTable).
            pub vftable: *mut usize,
        }
    };
}

opaque_handle!(
    /// `unreal::UObject` (`classes/uobject.h`).
    UObject,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::UField` (`classes/ufield.h`).
    UField,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::UStruct` (`classes/ustruct.h`).
    UStruct,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::UClass` (`classes/uclass.h`).
    UClass,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::UFunction` (`classes/ufunction.h`).
    UFunction,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::UEnum` (`classes/uenum.h`).
    UEnum,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::UConst` (`classes/uconst.h`).
    UConst,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::UScriptStruct` (`classes/uscriptstruct.h`).
    UScriptStruct,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::ZProperty` (`properties/zproperty.h`; on WILLOW
    /// `PROPERTIES_ARE_FFIELD == false`, `flavour.h`, so it derives from
    /// `UField`, keeping the `vftable`-first layout).
    ZProperty,
    "classes/uobject.h"
);
opaque_handle!(
    /// `unreal::FField` (`structs/ffield.h`; generic layout
    /// `offsets_generic.h` — `vftable` first).
    FField,
    "offsets_generic.h"
);
opaque_handle!(
    /// `unreal::FFieldClass` (`structs/ffield.h`).
    FFieldClass,
    "offsets_generic.h"
);
opaque_handle!(
    /// `unreal::FFrame` (`structs/fframe.h`; BL2 layout
    /// `game/bl2/offsets.h` — `VfTable` first).
    FFrame,
    "game/bl2/offsets.h"
);
opaque_handle!(
    /// `unreal::FNameEntry` (`structs/gnames.h`).
    FNameEntry,
    "structs/gnames.h"
);
