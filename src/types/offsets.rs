//! OffsetList replica + OffsetTable, plus crate-internal field readers.
//!
//! One `u16` field offset per X-macro field, in header order; the table is
//! copied once from `get_offsets()` (`unrealsdk_main.cpp`).
//! **Hardcoded field offsets are forbidden** — every version-dependent
//! read goes through here.

use std::ffi::c_void;

use super::structs::FName;

// ---------------------------------------------------------------------------
// OffsetList replica + OffsetTable
// ---------------------------------------------------------------------------

/// One `u16` per X-macro field, in header order. Each C++ `Offsets` struct is
/// `N * offset_type` (`offsets.h`, `offset_type = uint16_t`), so the
/// replica is one `u16` per listed field.
macro_rules! offsets_struct {
    ($name:ident : $($field:ident),+) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $name {
            $(pub $field: u16),+
        }
    };
}

// Order MUST match `UNREALSDK__DYNAMIC_OFFSET_TYPES` (offset_list.h);
// field order within each MUST match the class X-macro cited.
offsets_struct!(FFieldOffsets: class, owner, next, name); // ffield.h
offsets_struct!(FFieldClassOffsets: name, super_field); // ffield.h
offsets_struct!(FFrameOffsets: node, object, code); // fframe.h
offsets_struct!(FNameEntryOffsets: name, flags); // gnames.h
offsets_struct!(UClassOffsets: class_default_object, interfaces); // uclass.h
offsets_struct!(UConstOffsets: value); // uconst.h
offsets_struct!(UEnumOffsets: names); // uenum.h
offsets_struct!(UFieldOffsets: next); // ufield.h
offsets_struct!(UFunctionOffsets: function_flags, num_params, params_size, return_value_offset); // ufunction.h
offsets_struct!(UObjectOffsets: object_flags, internal_index, class, name, outer); // uobject.h
offsets_struct!(UScriptStructOffsets: struct_flags); // uscriptstruct.h
                                                     // WILLOW: no MinAlignment (`USTRUCT_HAS_ALIGNMENT == false`) and no
                                                     // ChildProperties (`PROPERTIES_ARE_FFIELD == false`) — ustruct.h,
                                                     // flavour.h.
offsets_struct!(UStructOffsets: super_field, children, property_size, property_link);
offsets_struct!(ZArrayPropertyOffsets: inner); // zarrayproperty.h
offsets_struct!(ZBoolPropertyOffsets: field_mask); // zboolproperty.h
offsets_struct!(ZByteAttributePropertyOffsets: modifier_stack_property, other_attribute_property); // attribute_property.h
offsets_struct!(ZBytePropertyOffsets: en); // zbyteproperty.h (`Enum`)
offsets_struct!(ZClassPropertyOffsets: meta_class); // zclassproperty.h
offsets_struct!(ZDelegatePropertyOffsets: signature); // zdelegateproperty.h
offsets_struct!(ZEnumPropertyOffsets: underlying_prop, en); // zenumproperty.h
offsets_struct!(ZFloatAttributePropertyOffsets: modifier_stack_property, other_attribute_property); // attribute_property.h
offsets_struct!(ZGameDataHandlePropertyOffsets: type_handle); // zgamedatahandleproperty.h
offsets_struct!(ZGbxDefPtrPropertyOffsets: strct); // zgbxdefptrproperty.h (`Struct`)
offsets_struct!(ZGbxInlineStructPropertyOffsets: meta_struct); // zgbxinlinestructproperty.h
offsets_struct!(ZIntAttributePropertyOffsets: modifier_stack_property, other_attribute_property); // attribute_property.h
offsets_struct!(ZInterfacePropertyOffsets: interface_class); // zinterfaceproperty.h
offsets_struct!(ZMulticastDelegatePropertyOffsets: signature); // zmulticastdelegateproperty.h
offsets_struct!(ZObjectPropertyOffsets: property_class); // zobjectproperty.h
offsets_struct!(ZPropertyOffsets: array_dim, element_size, property_flags, offset_internal, property_link_next); // zproperty.h
offsets_struct!(ZSoftClassPropertyOffsets: meta_class); // persistent_object_ptr_property.h
offsets_struct!(ZStructPropertyOffsets: strct); // zstructproperty.h (`Struct`)

/// `offsets::OffsetList` (`unreal/offset_list.h`): 30 member structs,
/// 56 `u16` fields = 112 bytes. Read-only snapshot owned by Rust; the live
/// table stays with the SDK.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct OffsetList {
    pub ffield: FFieldOffsets,
    pub ffield_class: FFieldClassOffsets,
    pub fframe: FFrameOffsets,
    pub fname_entry: FNameEntryOffsets,
    pub uclass: UClassOffsets,
    pub uconst: UConstOffsets,
    pub uenum: UEnumOffsets,
    pub ufield: UFieldOffsets,
    pub ufunction: UFunctionOffsets,
    pub uobject: UObjectOffsets,
    pub uscript_struct: UScriptStructOffsets,
    pub ustruct: UStructOffsets,
    pub zarray_property: ZArrayPropertyOffsets,
    pub zbool_property: ZBoolPropertyOffsets,
    pub zbyte_attribute_property: ZByteAttributePropertyOffsets,
    pub zbyte_property: ZBytePropertyOffsets,
    pub zclass_property: ZClassPropertyOffsets,
    pub zdelegate_property: ZDelegatePropertyOffsets,
    pub zenum_property: ZEnumPropertyOffsets,
    pub zfloat_attribute_property: ZFloatAttributePropertyOffsets,
    pub zgamedatahandle_property: ZGameDataHandlePropertyOffsets,
    pub zgbxdefptr_property: ZGbxDefPtrPropertyOffsets,
    pub zgbxinlinestruct_property: ZGbxInlineStructPropertyOffsets,
    pub zint_attribute_property: ZIntAttributePropertyOffsets,
    pub zinterface_property: ZInterfacePropertyOffsets,
    pub zmulticastdelegate_property: ZMulticastDelegatePropertyOffsets,
    pub zobject_property: ZObjectPropertyOffsets,
    pub zproperty: ZPropertyOffsets,
    pub zsoftclass_property: ZSoftClassPropertyOffsets,
    pub zstruct_property: ZStructPropertyOffsets,
}

// 56 fields × 2 bytes. If this fires, a field was added/dropped and every
// member after it shifted — re-check against offset_list.h.
const _: () = assert!(size_of::<OffsetList>() == 112);

/// Runtime-adapted field offsets for the current game build: the key to
/// version independence. Copied once from `get_offsets()` and cached;
/// `None` off-game. Only the fields the [`crate::objects`] layer actually
/// reads are
/// exposed — the rest stay reachable through [`OffsetList`] for later waves.
#[derive(Clone, Copy, Debug)]
pub struct OffsetTable {
    inner: OffsetList,
}

impl OffsetTable {
    /// Wrap an already-copied list (snapshot via [`crate::load_offsets`];
    /// direct construction in tests).
    pub fn from_list(inner: OffsetList) -> Self {
        Self { inner }
    }

    /// The raw snapshot (escape hatch for later waves, still read-only).
    pub fn as_list(&self) -> &OffsetList {
        &self.inner
    }

    // UObject (uobject.h)
    pub fn uobject_flags(&self) -> u16 {
        self.inner.uobject.object_flags
    }
    pub fn uobject_internal_index(&self) -> u16 {
        self.inner.uobject.internal_index
    }
    pub fn uobject_class(&self) -> u16 {
        self.inner.uobject.class
    }
    pub fn uobject_name(&self) -> u16 {
        self.inner.uobject.name
    }
    pub fn uobject_outer(&self) -> u16 {
        self.inner.uobject.outer
    }
    // UField (ufield.h)
    pub fn ufield_next(&self) -> u16 {
        self.inner.ufield.next
    }
    // UStruct (ustruct.h)
    pub fn ustruct_superfield(&self) -> u16 {
        self.inner.ustruct.super_field
    }
    pub fn ustruct_children(&self) -> u16 {
        self.inner.ustruct.children
    }
    pub fn ustruct_property_size(&self) -> u16 {
        self.inner.ustruct.property_size
    }
    pub fn ustruct_property_link(&self) -> u16 {
        self.inner.ustruct.property_link
    }
    // ZProperty (zproperty.h)
    pub fn zproperty_array_dim(&self) -> u16 {
        self.inner.zproperty.array_dim
    }
    pub fn zproperty_element_size(&self) -> u16 {
        self.inner.zproperty.element_size
    }
    pub fn zproperty_flags(&self) -> u16 {
        self.inner.zproperty.property_flags
    }
    pub fn zproperty_offset(&self) -> u16 {
        self.inner.zproperty.offset_internal
    }
    pub fn zproperty_link_next(&self) -> u16 {
        self.inner.zproperty.property_link_next
    }
    // UEnum (uenum.h — UE3 `names_type` is `TArray<FName>`)
    pub fn uenum_names(&self) -> u16 {
        self.inner.uenum.names
    }
    // UFunction (ufunction.h)
    pub fn ufunction_flags(&self) -> u16 {
        self.inner.ufunction.function_flags
    }
    pub fn ufunction_num_params(&self) -> u16 {
        self.inner.ufunction.num_params
    }
    pub fn ufunction_params_size(&self) -> u16 {
        self.inner.ufunction.params_size
    }
    pub fn ufunction_ret_offset(&self) -> u16 {
        self.inner.ufunction.return_value_offset
    }
    // UClass (uclass.h)
    pub fn uclass_default_object(&self) -> u16 {
        self.inner.uclass.class_default_object
    }
    // ZBoolProperty (zboolproperty.h; mask is u32 on WILLOW, flavour.h)
    pub fn zbool_field_mask(&self) -> u16 {
        self.inner.zbool_property.field_mask
    }
    // FFrame (fframe.h)
    pub fn fframe_node(&self) -> u16 {
        self.inner.fframe.node
    }
    pub fn fframe_object(&self) -> u16 {
        self.inner.fframe.object
    }
    pub fn fframe_code(&self) -> u16 {
        self.inner.fframe.code
    }
}

// ---------------------------------------------------------------------------
// crate-internal field readers (all offset-driven, no hardcoded layouts)
// ---------------------------------------------------------------------------
// Thin null-tolerant wrappers over [`crate::mem`] with a table offset —
// one raw-access implementation in the crate.

use crate::mem::{self, Offset};

/// Read a pointer-sized field at a table offset. Null base → null.
pub(crate) unsafe fn read_field_ptr<T>(base: *const c_void, off: u16) -> *mut T {
    unsafe { mem::read_ptr(base, Offset::from_table(off)) as *mut T }
}

/// Read a `u32` field at a table offset. Null base → 0.
pub(crate) unsafe fn read_field_u32(base: *const c_void, off: u16) -> u32 {
    unsafe { mem::read_u32(base, Offset::from_table(off)) }
}

/// Read an `i32` field at a table offset. Null base → 0.
/// Read a 64-bit field as two adjacent game words (the game ABI is 32-bit,
/// so a `QWORD` field is a low word at `off` and a high word at `off + 4`,
/// little-endian).
///
/// # Safety
/// `base + off..off + 8` must point at live engine memory (or `base` is
/// null).
pub(crate) unsafe fn read_field_u64(base: *const c_void, off: u16) -> u64 {
    let low = u64::from(unsafe { read_field_u32(base, off) });
    let high = u64::from(unsafe { read_field_u32(base, off.saturating_add(4)) });
    (high << 32) | low
}

/// Write a `u32` field at a table offset. Null base → no-op.
///
/// # Safety
/// `base + off` must point at writable engine memory (or `base` is null).
pub(crate) unsafe fn write_field_u32(base: *mut c_void, off: u16, value: u32) {
    if base.is_null() {
        return;
    }
    unsafe { mem::write_u32(base, Offset::from_table(off), value) };
}

/// The store side of [`read_field_u64`]: a 64-bit field is two adjacent
/// game words (low word at `off`, high word at `off + 4`, little-endian).
/// The 32-bit game ABI has no single 64-bit store, so a
/// concurrent writer to the same field can see the two halves torn.
///
/// # Safety
/// `base + off..off + 8` must point at writable engine memory (or `base`
/// is null, in which case nothing happens).
pub(crate) unsafe fn write_field_u64(base: *mut c_void, off: u16, value: u64) {
    unsafe {
        write_field_u32(base, off, value as u32);
        write_field_u32(base, off.saturating_add(4), (value >> 32) as u32);
    }
}

pub(crate) unsafe fn read_field_i32(base: *const c_void, off: u16) -> i32 {
    unsafe { mem::read_i32(base, Offset::from_table(off)) }
}

/// Read an `FName` (8 bytes) field at a table offset. Null base → default.
pub(crate) unsafe fn read_field_fname(base: *const c_void, off: u16) -> FName {
    if base.is_null() {
        return FName::default();
    }
    unsafe { *(base.byte_add(off as usize) as *const FName) }
}

/// Read a `uint16_t` field (e.g. `UFunction::ParamsSize`, `ufunction.h`).
pub(crate) unsafe fn read_field_u16(base: *const c_void, off: u16) -> u16 {
    if base.is_null() {
        return 0;
    }
    unsafe { *(base.byte_add(off as usize) as *const u16) }
}
