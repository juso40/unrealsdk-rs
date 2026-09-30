//! Reflection metadata: property/function/field lookup and the offset
//! readers over `ZProperty` subclasses.

use std::ffi::c_void;

use crate::load_offsets;
use crate::types::{
    read_field_fname, read_field_i32, read_field_ptr, read_field_u16, read_field_u32,
    read_field_u64, UClass, UEnum, UFunction, UObject, UScriptStruct, ZProperty,
};

use super::find::{object_class, object_name, object_name_text, object_path_text, objects};
use super::text::{decode_wide, fname_from_str, fname_to_string};

// ---------------------------------------------------------------------------
// Property walking + typed access
// ---------------------------------------------------------------------------

/// Run `visit` over every `ZProperty` of a `UStruct`: its own `Children`
/// first, then up the `SuperField` chain (`ustruct.h`, `ufield.h`).
/// Return `false` from `visit` to stop early. `st` may be any `UStruct` (a
/// `UClass` or a `UScriptStruct`).
fn walk_fields(st: *const c_void, mut visit: impl FnMut(*mut ZProperty) -> bool) {
    let Some(table) = load_offsets() else {
        return;
    };
    // SAFETY: `st` is a live UStruct; all offsets from the live table.
    unsafe {
        let mut st = st;
        while !st.is_null() {
            let mut child = read_field_ptr::<crate::types::UField>(st, table.ustruct_children());
            while !child.is_null() {
                if !visit(child as *mut ZProperty) {
                    return;
                }
                child = read_field_ptr(child as *const c_void, table.ufield_next());
            }
            st = read_field_ptr(st, table.ustruct_superfield());
        }
    }
}

/// First field of `st` named `name` (`FName` index + number 0) that `pred`
/// accepts.
fn find_field_where(
    st: *const c_void,
    name: &str,
    pred: impl Fn(*mut ZProperty) -> bool,
) -> Option<*mut ZProperty> {
    let table = load_offsets()?;
    let want = fname_from_str(name, 0)?;
    let mut found = None;
    walk_fields(st, |f| {
        // SAFETY: `f` is a live UField from the walk.
        if unsafe { read_field_fname(f as *const c_void, table.uobject_name()) } == want && pred(f) {
            found = Some(f);
            return false;
        }
        true
    });
    found
}

/// All `ZProperty` fields of a `UStruct` ([`find_property`]'s walk,
/// collected). `st` may be any `UStruct` (a `UClass` or a `UScriptStruct`).
pub fn struct_fields(st: *const c_void) -> Vec<*mut ZProperty> {
    let mut out = Vec::new();
    walk_fields(st, |f| {
        out.push(f);
        true
    });
    out
}

/// [`struct_fields`] lookup by exact name (`FName` index + number 0).
/// `st` may be any `UStruct`; for classes prefer [`find_property`].
pub fn find_field(st: *const c_void, name: &str) -> Option<*mut ZProperty> {
    find_field_where(st, name, |_| true)
}

/// Walk `class` + its `SuperField` chain, following `Children`/`Next`, for
/// the property named `name` (`ustruct.h`, `ufield.h`). `None` when
/// absent or off-game. Name match is exact (`FName` index + number 0). (This
/// is [`find_field`] with a `UClass` parameter; both share the one walk.)
pub fn find_property(class: *mut UClass, name: &str) -> Option<*mut ZProperty> {
    find_field(class as *const c_void, name)
}

/// Walk `class` + its `SuperField` chain for the **function** named `name`
/// (same walk as [`find_property`], but the match must be a `UFunction`:
/// functions and properties share the `Children` chain on UE3, so the
/// candidate's class is verified to be `Function`). `None` when absent or
/// off-game.
pub fn find_function(class: *mut UClass, name: &str) -> Option<*mut UFunction> {
    let table = load_offsets()?;
    let func_class = fname_from_str("Function", 0)?;
    let f = find_field_where(class as *const c_void, name, |f| {
        // SAFETY: `f` is a live UField; class via the live table.
        let cls = unsafe { read_field_ptr::<UClass>(f as *const c_void, table.uobject_class()) };
        !cls.is_null()
            // SAFETY: as above.
            && unsafe { read_field_fname(cls as *const c_void, table.uobject_name()) } == func_class
    })?;
    Some(f as *mut UFunction)
}
/// The `CPF_Parm` properties of `func` in call order (the `PropertyLink`
/// chain filtered, the shape `bound_function.h`'s `func_params` walks),
/// including the return property.
fn parm_chain(func: *const UFunction) -> Vec<*mut ZProperty> {
    use crate::flags::property_flags::CPF_PARM;
    let mut out = Vec::new();
    let Some(table) = load_offsets() else {
        return out;
    };
    // SAFETY: `func` is a live UFunction; chain per `zproperty.h`.
    unsafe {
        let mut p =
            read_field_ptr::<ZProperty>(func as *const c_void, table.ustruct_property_link());
        while !p.is_null() {
            if property_flags(p).is_some_and(|f| f & CPF_PARM != 0) {
                out.push(p);
            }
            p = read_field_ptr(p as *const c_void, table.zproperty_link_next());
        }
    }
    out
}

/// The parameter properties of `func` in call order, excluding the return
/// property ([`function_return_param`]).
pub fn function_params(func: *const UFunction) -> Vec<*mut ZProperty> {
    use crate::flags::property_flags::CPF_RETURN_PARM;
    parm_chain(func)
        .into_iter()
        .filter(|&p| property_flags(p).is_none_or(|f| f & CPF_RETURN_PARM == 0))
        .collect()
}

/// The `CPF_ReturnParm` property of `func`, if any (`ufunction.h`'s
/// `find_return_param`).
pub fn function_return_param(func: *const UFunction) -> Option<*mut ZProperty> {
    use crate::flags::property_flags::CPF_RETURN_PARM;
    parm_chain(func)
        .into_iter()
        .find(|&p| property_flags(p).is_some_and(|f| f & CPF_RETURN_PARM != 0))
}

/// The property's class name (e.g. `IntProperty`) via
/// `UObject.Class.Name` + `fname_get_str`.
pub fn property_class_name(prop: *const ZProperty) -> Option<String> {
    if prop.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live property; offsets from the live table.
    let cls = unsafe { read_field_ptr::<UClass>(prop as *const c_void, table.uobject_class()) };
    if cls.is_null() {
        return None;
    }
    let n = unsafe { read_field_fname(cls as *const c_void, table.uobject_name()) };
    fname_to_string(&n).map(|t| t.text)
}

/// `ZProperty.Offset_Internal` (`zproperty.h`) — where the value lives
/// inside instances of the owning class. `None` off-game or on a null prop.
pub fn property_offset(prop: *const ZProperty) -> Option<i32> {
    if prop.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live property; offset from the live table.
    Some(unsafe { read_field_i32(prop as *const c_void, table.zproperty_offset()) })
}

/// `ZProperty.ElementSize` (`zproperty.h`) — per-element byte size (for
/// `ArrayProperty`, the size of one array element). `None` off-game.
pub fn property_element_size(prop: *const ZProperty) -> Option<i32> {
    if prop.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live property; offset from the live table.
    Some(unsafe { read_field_i32(prop as *const c_void, table.zproperty_element_size()) })
}

/// `ZProperty.ArrayDim` (`zproperty.h`). `None` off-game or null prop.
pub fn property_array_dim(prop: *const ZProperty) -> Option<i32> {
    if prop.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live property; offset from the live table.
    Some(unsafe { read_field_i32(prop as *const c_void, table.zproperty_array_dim()) })
}

/// UE3 `UEnum.Names` (`uenum.h`): a `TArray<FName>` whose **index is
/// the value**. Returns the entry names in declaration order. `None` off-game
/// or on a null object.
pub fn enum_names(enum_obj: *const crate::types::UEnum) -> Option<Vec<String>> {
    if enum_obj.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live UEnum; offsets from the live table. `UEnum::Names` is an
    // **inline** `TArray<FName>` (`uenum.h`), not a pointer to one — read
    // the header fields in place (data @off, count @off+4). Dereferencing the
    // first word as a `TArray*` (the old bug) reinterpreted entry 0's FName as
    // a header and read its number (0) as the count.
    unsafe {
        let base = enum_obj as *const c_void;
        let off = table.uenum_names();
        let data = read_field_ptr::<crate::types::FName>(base, off);
        // Sanity bound: no real UE3 enum has thousands of entries, and a
        // garbage count must not drive a wild iteration over `data`.
        let count = (read_field_i32(base, off + 4).max(0) as usize).min(4096);
        if data.is_null() && count > 0 {
            return None;
        }
        Some(
            (0..count)
                .map(|i| {
                    let name = *data.add(i);
                    fname_to_string(&name).map(|t| t.text).unwrap_or_default()
                })
                .collect(),
        )
    }
}


/// `ZProperty.PropertyFlags` as the full 64-bit `EPropertyFlags` word
/// (`UnType.h` has the field `QWORD`; the SDK models only its low 32
/// bits, `flavour.h`). See [`crate::flags::property_flags`]. `None`
/// off-game or on a null prop.
pub fn property_flags(prop: *const ZProperty) -> Option<u64> {
    if prop.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live property; offset from the live table.
    Some(unsafe { read_field_u64(prop as *const c_void, table.zproperty_flags()) })
}

/// `UFunction.FunctionFlags` (`UnClass.h`, `game/bl2/offsets.h`).
/// See [`crate::flags::function_flags`]. `None` off-game or on a null
/// function.
pub fn function_flags(func: *const UFunction) -> Option<u32> {
    if func.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live function; offset from the live table.
    Some(unsafe { read_field_u32(func as *const c_void, table.ufunction_flags()) })
}

/// A live `UEnum` located in GObjects.
pub struct UEnumInfo {
    pub obj: *mut UObject,
    pub name: String,
    pub path: String,
}

/// Every live `UEnum` in GObjects (cheap `FName` class compare first —
/// names/paths are only converted for real enums). `None` off-game or when
/// GObjects is unreadable.
pub fn all_uenums() -> Option<Vec<UEnumInfo>> {
    let enum_class = fname_from_str("Enum", 0)?;
    let all = objects()?;
    let mut out = Vec::new();
    for obj in all {
        let Some(c) = object_class(obj) else { continue };
        let Some(n) = object_name(c as *const UObject) else {
            continue;
        };
        if n != enum_class {
            continue;
        }
        let name = object_name_text(obj);
        let path = object_path_text(obj).unwrap_or_else(|| name.clone());
        out.push(UEnumInfo { obj, name, path });
    }
    Some(out)
}

/// `UFunction::ParamsSize` (`ufunction.h`, `uint16_t` on this build):
/// the size in bytes of the params block for [`process_event`](super::process_event).
pub fn function_params_size(func: *const UFunction) -> Option<u16> {
    let table = load_offsets()?;
    // SAFETY: `func` is a live UFunction; offset from the live table.
    Some(unsafe { read_field_u16(func as *const c_void, table.ufunction_params_size()) })
}

/// `ZArrayProperty::Inner` (`zarrayproperty.h`): the element property.
pub fn property_inner(prop: *const ZProperty) -> Option<*mut ZProperty> {
    if property_class_name(prop)?.as_str() != "ArrayProperty" {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: `prop` is a live ZArrayProperty; offset from the live table.
    Some(unsafe { read_field_ptr(prop as *const c_void, table.as_list().zarray_property.inner) })
}

/// `ZStructProperty::Struct` (`zstructproperty.h`): the `UScriptStruct`.
pub fn property_struct(prop: *const ZProperty) -> Option<*mut UScriptStruct> {
    if property_class_name(prop)?.as_str() != "StructProperty" {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: `prop` is a live ZStructProperty; offset from the live table.
    Some(unsafe { read_field_ptr(prop as *const c_void, table.as_list().zstruct_property.strct) })
}

/// `ZByteProperty::Enum` / `ZEnumProperty::Enum` (`zbyteproperty.h`,
/// `zenumproperty.h`): the `UEnum`, or `None` for a plain byte.
pub fn property_enum(prop: *const ZProperty) -> Option<*mut UEnum> {
    let table = load_offsets()?;
    let list = table.as_list();
    // SAFETY: `prop` is a live ZByteProperty/ZEnumProperty.
    unsafe {
        match property_class_name(prop)?.as_str() {
            "ByteProperty" => Some(read_field_ptr(prop as *const c_void, list.zbyte_property.en)),
            "EnumProperty" => Some(read_field_ptr(prop as *const c_void, list.zenum_property.en)),
            _ => None,
        }
    }
}

/// `ZEnumProperty::UnderlyingProp` (`zenumproperty.h`): the storage
/// property describing how the value is laid out.
pub fn property_underlying(prop: *const ZProperty) -> Option<*mut ZProperty> {
    if property_class_name(prop)?.as_str() != "EnumProperty" {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: `prop` is a live ZEnumProperty; offset from the live table.
    Some(unsafe {
        read_field_ptr(prop as *const c_void, table.as_list().zenum_property.underlying_prop)
    })
}

/// Resolve a property's address: `base + Offset_Internal`. `None` on nulls,
/// negative offsets, or off-game. `get_property`/`set_property` resolve
/// through this; [`crate::objects`] walks dotted paths with it.
pub fn property_addr(base: *const c_void, prop: *const ZProperty) -> Option<*mut c_void> {
    if base.is_null() || prop.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live property; offset from the live table.
    let off = unsafe { read_field_i32(prop as *const c_void, table.zproperty_offset()) };
    if off < 0 {
        return None;
    }
    // SAFETY: caller guarantees `base` points at a live object/struct.
    Some(unsafe { base.byte_add(off as usize) as *mut c_void })
}

// ---------------------------------------------------------------------------
// UClass / UConst readers
// ---------------------------------------------------------------------------

/// `UClass::ClassDefaultObject` (`uclass.h`): the archetype instance
/// holding the class's default property values. `None` off-game or on a
/// null class.
pub fn class_default_object(cls: *const UClass) -> Option<*mut UObject> {
    if cls.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live class; offset from the live table.
    Some(unsafe {
        read_field_ptr::<UObject>(cls as *const c_void, table.uclass_default_object())
    })
}

/// `UConst::Value` (`uconst.h`): the constant's inline
/// `UnmanagedFString`. `None` off-game or on a null const.
pub fn const_value(con: *const crate::types::UConst) -> Option<String> {
    if con.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live UConst; the value is an inline `TArray<u16>` header at
    // the table offset (like `UEnum::Names`, not a pointer to one).
    unsafe {
        let fstr = crate::types::TArray::<u16>::read(
            (con as *const u8).add(table.as_list().uconst.value as usize) as *const c_void,
        );
        let len = fstr.count.max(0) as usize;
        decode_wide(fstr.data, len).map(|s| s.trim_end_matches('\0').to_owned())
    }
}

/// `UClass::Interfaces` (`uclass.h`, an inline
/// `TArray<FImplementedInterface>`): the interfaces implemented *directly*
/// on this class. UE3 entries are `{UClass* Class; ZStructProperty*
/// VFTableProperty}` (`fimplementedinterface.h`, two words). For
/// the inherited view use [`implements_interface`]. `None` off-game.
pub fn class_interfaces(cls: *const UClass) -> Option<Vec<*mut UClass>> {
    if cls.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live class; the array header is read in place at the table
    // offset, entries are two game words each.
    unsafe {
        let arr = crate::types::TArray::<u8>::read(
            (cls as *const u8).add(table.as_list().uclass.interfaces as usize) as *const c_void,
        );
        const ENTRY_SIZE: usize = 2 * crate::mem::GAME_PTR;
        let mut out = Vec::new();
        for i in 0..arr.count.max(0) as usize {
            let entry = (arr.data as *const u8).add(i * ENTRY_SIZE) as *const *mut UClass;
            if !(*entry).is_null() {
                out.push(*entry);
            }
        }
        Some(out)
    }
}

/// `UObject::is_implementation` (`uobject.cpp` over
/// `uclass.cpp`): does `obj`'s class chain implement `iface`
/// (exact `UClass*` match in any chain link's [`class_interfaces`])?
/// `None` off-game.
pub fn implements_interface(obj: *const UObject, iface: *const UClass) -> Option<bool> {
    if obj.is_null() || iface.is_null() {
        return None;
    }
    let table = load_offsets()?;
    let mut st = object_class(obj)?;
    while !st.is_null() {
        if class_interfaces(st as *const UClass)?
            .iter()
            .any(|&i| std::ptr::eq(i, iface))
        {
            return Some(true);
        }
        // SAFETY: live class chain; offset from the live table.
        st = unsafe { read_field_ptr(st as *const c_void, table.ustruct_superfield()) };
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_and_const_readers_reject_nulls() {
        assert!(class_default_object(std::ptr::null()).is_none());
        assert!(const_value(std::ptr::null()).is_none());
        assert!(class_interfaces(std::ptr::null()).is_none());
        assert!(implements_interface(std::ptr::null(), std::ptr::null()).is_none());
    }
}
