//! Property values: the [`PropValue`] zoo and its typed get/set/destroy.

use std::ffi::c_void;

use crate::load_offsets;
use crate::sys::sys;
use crate::types::{read_field_fname, read_field_u32, FName, UObject, ZProperty};

use super::find::object_name_text;
use super::reflect::{
    find_field, property_addr, property_array_dim, property_class_name, property_element_size,
    property_enum, property_inner, property_offset, property_struct, property_underlying,
    struct_fields,
};
use super::text::{decode_wide, encode_wide, fname_from_str, fname_to_string, ftext_from_str};

/// A typed property value. Static arrays (`ArrayDim > 1`) round-trip as
/// [`PropValue::StaticArray`], one entry per element.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Int(i32),
    Float(f32),
    Bool(bool),
    Byte(u8),
    String(String),
    Object(*mut UObject),
    Name(String),
    /// `EnumProperty` (or `ByteProperty` with a non-null `Enum`): the raw
    /// underlying value plus its `UEnum` name (resolve entries with
    /// [`enum_names`](super::enum_names)).
    Enum { enum_name: String, value: i64 },
    Struct(StructValue),
    Array(Vec<PropValue>),
    Delegate(DelegateValue),
    /// `MulticastDelegateProperty`: the invocation list
    /// (`TArray<FScriptDelegate>`, `wrapped_multicast_delegate.h`).
    MulticastDelegate(Vec<DelegateValue>),
    /// `WeakObjectProperty`: the raw [`FWeakObjectPtr`](crate::types::FWeakObjectPtr)
    /// pair. UE3 has no serial table to resolve against
    /// (`gobjects.cpp` refuses outright), so the index/serial
    /// pair is all the engine itself stores.
    WeakObject(WeakObjectValue),
    /// `SoftObjectProperty`/`SoftClassProperty`: the identifier path
    /// (`FSoftObjectPath`, `tpersistentobjectptr.h`). Resolution to
    /// a live object needs the BL3-only weak-pointer support, so only the
    /// path round-trips.
    SoftObject(SoftObjectValue),
    /// A static array (`ArrayDim > 1`): one entry per element.
    StaticArray(Vec<PropValue>),
}

/// A `StructProperty` value: its fields by name, recursively converted.
#[derive(Clone, Debug, PartialEq)]
pub struct StructValue {
    /// The `UScriptStruct`'s name.
    pub name: String,
    pub fields: Vec<(String, PropValue)>,
}

/// A `DelegateProperty` value (`FScriptDelegate`,
/// `structs/fscriptdelegate.h`): `{UObject* object; FName func_name}`.
/// A null `object` is unbound.
#[derive(Clone, Debug, PartialEq)]
pub struct DelegateValue {
    pub object: *mut UObject,
    pub function: String,
}

/// A `WeakObjectProperty` value: the raw `FWeakObjectPtr`
/// (`structs/fweakobjectptr.h`) `{index, serial}` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeakObjectValue {
    pub object_index: i32,
    pub object_serial: i32,
}

/// A `SoftObjectProperty` value: the identifier path
/// (`FSoftObjectPath`, `tpersistentobjectptr.h`) —
/// `asset_path_name` plus the trailing `subpath` (e.g. `".", ""` for a
/// plain asset reference).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftObjectValue {
    pub asset_path_name: String,
    pub subpath: String,
}

impl PropValue {
    /// The variant name ("Int", "Float", ...), for error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            PropValue::Int(_) => "Int",
            PropValue::Float(_) => "Float",
            PropValue::Bool(_) => "Bool",
            PropValue::Byte(_) => "Byte",
            PropValue::String(_) => "String",
            PropValue::Object(_) => "Object",
            PropValue::Name(_) => "Name",
            PropValue::Enum { .. } => "Enum",
            PropValue::Struct(_) => "Struct",
            PropValue::Array(_) => "Array",
            PropValue::Delegate(_) => "Delegate",
            PropValue::MulticastDelegate(_) => "MulticastDelegate",
            PropValue::WeakObject(_) => "WeakObject",
            PropValue::SoftObject(_) => "SoftObject",
            PropValue::StaticArray(_) => "StaticArray",
        }
    }
}

/// Release what a property value owns at `base + Offset_Internal` (string,
/// array and nested-struct buffers) and zero the freed headers: the SDK's
/// `PropTraits::destroy` equivalent, for tearing down a params block
/// ([`crate::objects::Obj::call`]). A no-op for scalars, names, enums,
/// objects and delegates.
pub fn destroy_property_value(base: *mut c_void, prop: *mut ZProperty) {
    if let Some(addr) = property_addr(base as *const c_void, prop as *const ZProperty) {
        destroy_value_at(prop, addr);
    }
}

/// Read a property at `base` (object or struct address). Type-checked against
/// the coverage matrix; mismatch logs at `warn` and returns `None`. A static
/// array (`ArrayDim > 1`) reads whole as [`PropValue::StaticArray`].
pub fn get_property(base: *const c_void, prop: *const ZProperty) -> Option<PropValue> {
    get_value_at(prop, property_addr(base, prop)?)
}

/// Read element `idx` of a property at `base`, bounds-checked against its
/// `ArrayDim` (the SDK's `get_property<T>(prop, idx, addr)` shape,
/// `prop_traits.h`).
pub fn get_property_at(
    base: *const c_void,
    prop: *const ZProperty,
    idx: usize,
) -> Option<PropValue> {
    if idx >= static_dim(prop) {
        return None;
    }
    let elem = property_element_size(prop)?.max(0) as usize;
    let addr = property_addr(base, prop)?;
    get_scalar_at(prop, unsafe { addr.byte_add(idx * elem) })
}

/// The property's static-array length (`ZProperty.ArrayDim`, min 1).
fn static_dim(prop: *const ZProperty) -> usize {
    property_array_dim(prop).unwrap_or(1).max(1) as usize
}

/// [`get_property`] at an already-resolved `addr` (element slots, fields).
/// Static arrays read whole as [`PropValue::StaticArray`].
fn get_value_at(prop: *const ZProperty, addr: *const c_void) -> Option<PropValue> {
    let dim = static_dim(prop);
    if dim > 1 {
        let elem = property_element_size(prop)?.max(0) as usize;
        let mut out = Vec::with_capacity(dim);
        for i in 0..dim {
            out.push(get_scalar_at(prop, unsafe { addr.byte_add(i * elem) })?);
        }
        Some(PropValue::StaticArray(out))
    } else {
        get_scalar_at(prop, addr)
    }
}

/// [`get_value_at`] for one element slot (the `ArrayDim == 1` case).
fn get_scalar_at(prop: *const ZProperty, addr: *const c_void) -> Option<PropValue> {
    let class = property_class_name(prop)?;
    // SAFETY: `addr` points at this property's live storage.
    unsafe {
        match class.as_str() {
            "IntProperty" => Some(PropValue::Int(*(addr as *const i32))),
            "FloatProperty" => Some(PropValue::Float(*(addr as *const f32))),
            "BoolProperty" => {
                let table = load_offsets()?;
                let mask = read_field_u32(prop as *const c_void, table.zbool_field_mask());
                let word = *(addr as *const u32);
                Some(PropValue::Bool(word & mask != 0))
            }
            "ByteProperty" => {
                let v = *(addr as *const u8);
                match property_enum(prop) {
                    Some(en) => Some(PropValue::Enum {
                        enum_name: object_name_text(en as *mut UObject),
                        value: i64::from(v),
                    }),
                    None => Some(PropValue::Byte(v)),
                }
            }
            "EnumProperty" => Some(PropValue::Enum {
                enum_name: object_name_text(property_enum(prop)? as *mut UObject),
                value: enum_value_at(prop, addr)?,
            }),
            "NameProperty" => {
                let name = read_field_fname(addr, 0);
                Some(PropValue::Name(fname_to_string(&name)?.text))
            }
            "StrProperty" => {
                let fstr = &*(addr as *const crate::types::UnmanagedFString);
                // `count` includes the terminator; guard a garbage count.
                let len = fstr.count.max(0) as usize;
                Some(PropValue::String(
                    decode_wide(fstr.data, len)?
                        .trim_end_matches('\0')
                        .to_owned(),
                ))
            }
            "TextProperty" => {
                // FText content is engine-side (FTextData behind a vftable)
                // and the C API only writes (`ftext_as_culture_invariant`;
                // WILLOW is `FTEXT_FORMAT_NOT_IMPLEMENTED`). Reads cannot
                // work without a new upstream export.
                log::warn!(
                    "unrealsdk-rs: get_property: FText content is not readable through the \
                     unrealsdk C API"
                );
                None
            }
            "ObjectProperty" | "ClassProperty" | "InterfaceProperty" => {
                Some(PropValue::Object(*(addr as *const *mut UObject)))
            }
            "StructProperty" => {
                let st = property_struct(prop)?;
                let mut fields = Vec::new();
                for f in struct_fields(st as *const c_void) {
                    let off = property_offset(f)? as usize;
                    let name = object_name_text(f as *mut UObject);
                    let value = get_value_at(f, (addr as *const u8).add(off).cast())?;
                    fields.push((name, value));
                }
                Some(PropValue::Struct(StructValue {
                    name: object_name_text(st as *mut UObject),
                    fields,
                }))
            }
            "ArrayProperty" => {
                let inner = property_inner(prop)?;
                let elem = property_element_size(inner)?.max(0) as usize;
                if elem == 0 {
                    return None;
                }
                let arr = crate::types::TArray::<u8>::read(addr);
                let mut out = Vec::new();
                for i in 0..arr.count.max(0) as usize {
                    let slot = (arr.data as *const u8).add(i * elem).cast();
                    out.push(get_value_at(inner, slot)?);
                }
                Some(PropValue::Array(out))
            }
            "DelegateProperty" => Some(PropValue::Delegate(read_script_delegate(addr))),
            "MulticastDelegateProperty" => {
                // `FMulticastScriptDelegate` is a `TArray<FScriptDelegate>`
                // (`wrapped_multicast_delegate.h`).
                let arr = crate::types::TArray::<u8>::read(addr);
                let mut out = Vec::new();
                for i in 0..arr.count.max(0) as usize {
                    let slot = (arr.data as *const u8).add(i * SCRIPT_DELEGATE_SIZE);
                    out.push(read_script_delegate(slot as *const c_void));
                }
                Some(PropValue::MulticastDelegate(out))
            }
            "WeakObjectProperty" => {
                let idx = *(addr as *const i32);
                let serial = *((addr as *const i32).add(1));
                Some(PropValue::WeakObject(WeakObjectValue {
                    object_index: idx,
                    object_serial: serial,
                }))
            }
            "SoftObjectProperty" | "SoftClassProperty" => {
                Some(PropValue::SoftObject(read_soft_object(addr)?))
            }
            "LazyObjectProperty" => {
                log::warn!(
                    "unrealsdk-rs: get_property: LazyObjectProperty is unsupported \
                     (lazy object pointers are BL3-only upstream)"
                );
                None
            }
            other => {
                log::warn!("unrealsdk-rs: get_property: unsupported property class '{other}'");
                None
            }
        }
    }
}

/// The raw value of an `EnumProperty` at `addr`: signed per the SDK
/// (`zenumproperty.cpp`: negatives are possible), width per the
/// `UnderlyingProp`.
fn enum_value_at(prop: *const ZProperty, addr: *const c_void) -> Option<i64> {
    let underlying = property_underlying(prop)?;
    let class = property_class_name(underlying)?;
    // SAFETY: `addr` holds the enum's storage; width per the underlying prop.
    unsafe {
        Some(match class.as_str() {
            "ByteProperty" => i64::from(*(addr as *const u8)),
            "IntProperty" => i64::from(*(addr as *const i32)),
            other => {
                log::warn!("unrealsdk-rs: get_property: enum underlying '{other}' unsupported");
                return None;
            }
        })
    }
}

/// Write a property at `base`. The variant must match the property's class
/// (same matrix as reads); `String` only fits in-place (`len + 1 <= max`),
/// otherwise `None`. Mismatch logs at `warn`.
/// Write `value` at `base + Offset_Internal`. Type-checked against the
/// coverage matrix: a mismatch logs at `warn` and returns `false`, never
/// panics. Strings and arrays use the game allocator (`u_malloc`/
/// `u_realloc`/`u_free` are the game's `FMalloc`, `game/bl2/memory.cpp`),
/// the same one the engine's destructors free with, so grown buffers stay
/// engine-owned. The old value's owned data is released first (the SDK's
/// `PropTraits::set` + `destroy` contract).
/// `PropTraits::set` + `destroy` contract). `StaticArray` writes every
/// element; a scalar writes element 0 (the SDK's `set<T>` default index,
/// `prop_traits.h`).
pub fn set_property(base: *mut c_void, prop: *mut ZProperty, value: &PropValue) -> bool {
    let Some(addr) = property_addr(base as *const c_void, prop as *const ZProperty) else {
        return false;
    };
    set_value_at(prop, addr, value)
}

/// Write element `idx` of a property at `base`, bounds-checked against its
/// `ArrayDim` (the SDK's `set_property<T>(prop, idx, addr, value)` shape,
/// `prop_traits.h`).
pub fn set_property_at(
    base: *mut c_void,
    prop: *mut ZProperty,
    idx: usize,
    value: &PropValue,
) -> bool {
    if idx >= static_dim(prop) {
        return false;
    }
    let Some(elem) = property_element_size(prop) else {
        return false;
    };
    let elem = elem.max(0) as usize;
    let Some(addr) = property_addr(base as *const c_void, prop as *const ZProperty) else {
        return false;
    };
    set_scalar_at(prop, unsafe { addr.byte_add(idx * elem) }, value)
}

/// [`set_property`] at an already-resolved `addr` (element slots, fields).
fn set_value_at(prop: *mut ZProperty, addr: *mut c_void, value: &PropValue) -> bool {
    match value {
        PropValue::StaticArray(vals) => {
            let dim = static_dim(prop);
            if vals.len() != dim {
                log::warn!(
                    "unrealsdk-rs: set_property: static array wants {dim} elements, got {}",
                    vals.len()
                );
                return false;
            }
            let Some(elem) = property_element_size(prop) else {
                return false;
            };
            let elem = elem.max(0) as usize;
            for (i, v) in vals.iter().enumerate() {
                if !set_scalar_at(prop, unsafe { addr.byte_add(i * elem) }, v) {
                    return false;
                }
            }
            true
        }
        _ => set_scalar_at(prop, addr, value),
    }
}

/// [`set_value_at`] for one element slot (the `ArrayDim == 1` case).
fn set_scalar_at(prop: *mut ZProperty, addr: *mut c_void, value: &PropValue) -> bool {
    let Some(class) = property_class_name(prop) else {
        return false;
    };
    // SAFETY: `addr` points at this property's live storage.
    unsafe {
        match (class.as_str(), value) {
            ("IntProperty", PropValue::Int(v)) => {
                *(addr as *mut i32) = *v;
                true
            }
            ("FloatProperty", PropValue::Float(v)) => {
                *(addr as *mut f32) = *v;
                true
            }
            ("BoolProperty", PropValue::Bool(v)) => {
                let Some(table) = load_offsets() else {
                    return false;
                };
                let mask = read_field_u32(prop as *const c_void, table.zbool_field_mask());
                let word = addr as *mut u32;
                if *v {
                    *word |= mask;
                } else {
                    *word &= !mask;
                }
                true
            }
            ("ByteProperty", PropValue::Byte(v)) => {
                *(addr as *mut u8) = *v;
                true
            }
            ("ByteProperty", PropValue::Enum { value, .. }) => match u8::try_from(*value) {
                Ok(v) => {
                    *(addr as *mut u8) = v;
                    true
                }
                Err(_) => {
                    log::warn!("unrealsdk-rs: set_property: enum value {value} exceeds a byte");
                    false
                }
            },
            ("EnumProperty", PropValue::Enum { value, .. }) => set_enum_value(prop, addr, *value),
            ("EnumProperty", PropValue::Byte(v)) => set_enum_value(prop, addr, i64::from(*v)),
            ("NameProperty", PropValue::Name(v)) => {
                let Some(fname) = fname_from_str(v, 0) else {
                    return false;
                };
                *(addr as *mut FName) = fname;
                true
            }
            ("StrProperty", PropValue::String(v)) => set_fstring(addr, v),
            ("TextProperty", PropValue::String(v)) => {
                ftext_from_str(addr as *mut crate::types::FText, v)
            }
            ("ObjectProperty" | "ClassProperty" | "InterfaceProperty", PropValue::Object(v)) => {
                *(addr as *mut *mut UObject) = *v;
                true
            }
            ("StructProperty", PropValue::Struct(sv)) => {
                let Some(st) = property_struct(prop) else {
                    return false;
                };
                for (name, v) in &sv.fields {
                    let Some(f) = find_field(st as *const c_void, name) else {
                        log::warn!(
                            "unrealsdk-rs: set_property: struct '{}' has no field '{name}'",
                            sv.name
                        );
                        return false;
                    };
                    let Some(off) = property_offset(f) else {
                        return false;
                    };
                    let slot = (addr as *mut u8).add(off.max(0) as usize) as *mut c_void;
                    if !set_value_at(f, slot, v) {
                        return false;
                    }
                }
                true
            }
            ("ArrayProperty", PropValue::Array(vals)) => set_array_value(prop, addr, vals),
            ("DelegateProperty", PropValue::Delegate(d)) => write_script_delegate(addr, d),
            ("MulticastDelegateProperty", PropValue::MulticastDelegate(vals)) => {
                set_multicast_value(addr, vals)
            }
            ("WeakObjectProperty", PropValue::WeakObject(w)) => {
                *(addr as *mut i32) = w.object_index;
                *((addr as *mut i32).add(1)) = w.object_serial;
                true
            }
            ("SoftObjectProperty" | "SoftClassProperty", PropValue::SoftObject(v)) => {
                set_soft_object(addr, v)
            }
            ("LazyObjectProperty", _) => {
                log::warn!(
                    "unrealsdk-rs: set_property: LazyObjectProperty is unsupported \
                     (lazy object pointers are BL3-only upstream)"
                );
                false
            }
            (cls, val) => {
                log::warn!("unrealsdk-rs: set_property: class '{cls}' vs value '{val:?}' mismatch");
                false
            }
        }
    }
}

/// Enum storage write: width per the `UnderlyingProp` (the write side of
/// [`enum_value_at`]).
fn set_enum_value(prop: *mut ZProperty, addr: *mut c_void, value: i64) -> bool {
    let Some(underlying) = property_underlying(prop) else {
        return false;
    };
    let Some(class) = property_class_name(underlying) else {
        return false;
    };
    // SAFETY: `addr` holds the enum's storage; width per the underlying prop.
    unsafe {
        match class.as_str() {
            "ByteProperty" => match u8::try_from(value) {
                Ok(v) => {
                    *(addr as *mut u8) = v;
                    true
                }
                Err(_) => {
                    log::warn!("unrealsdk-rs: set_property: enum value {value} exceeds a byte");
                    false
                }
            },
            "IntProperty" => match i32::try_from(value) {
                Ok(v) => {
                    *(addr as *mut i32) = v;
                    true
                }
                Err(_) => {
                    log::warn!("unrealsdk-rs: set_property: enum value {value} exceeds an i32");
                    false
                }
            },
            other => {
                log::warn!("unrealsdk-rs: set_property: enum underlying '{other}' unsupported");
                false
            }
        }
    }
}

/// `FString` assignment with allocator pairing: grow via `u_malloc` when
/// the string does not fit, free the old buffer via `u_free` (both the
/// game's `FMalloc`, which `FString`'s destructor frees with).
fn set_fstring(addr: *mut c_void, v: &str) -> bool {
    let Some(s) = sys() else {
        return false;
    };
    let wide = encode_wide(v);
    let needed = wide.len() + 1;
    // SAFETY: `addr` holds a live FString (`TArray<u16>`) header.
    let fstr = unsafe { &mut *(addr as *mut crate::types::UnmanagedFString) };
    if fstr.data.is_null() || needed > fstr.max.max(0) as usize {
        // SAFETY: resolved export; allocation pairs with the engine's free.
        let buf = unsafe { (s.u_malloc)(needed * std::mem::size_of::<u16>()) } as *mut u16;
        if buf.is_null() {
            log::warn!("unrealsdk-rs: set_property: string allocation failed");
            return false;
        }
        if !fstr.data.is_null() {
            // SAFETY: buffer came from the same allocator (`FMalloc`).
            unsafe { (s.u_free)(fstr.data as *mut c_void) };
        }
        fstr.data = buf;
        fstr.max = needed as i32;
    }
    // SAFETY: `wide.len()` units fit the (re)allocated buffer.
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), fstr.data, wide.len());
        *fstr.data.add(wide.len()) = 0;
    }
    fstr.count = wide.len() as i32 + 1;
    true
}

/// `TArray` replace (the SDK's `WrappedArray::resize` shape): destroy the
/// old elements, grow the buffer via `u_realloc` when needed, zero the
/// fresh slots (owning writes allocate into zeroed storage), write the
/// new elements and stamp the header.
fn set_array_value(prop: *mut ZProperty, addr: *mut c_void, vals: &[PropValue]) -> bool {
    use crate::types::TArray;
    let Some(s) = sys() else {
        return false;
    };
    let Some(inner) = property_inner(prop) else {
        return false;
    };
    let Some(elem) = property_element_size(inner) else {
        return false;
    };
    let elem = elem.max(0) as usize;
    if elem == 0 {
        return false;
    }
    // SAFETY: `addr` holds a live TArray header; allocators pair with the
    // engine's free (see `set_property` docs).
    unsafe {
        let arr = TArray::<u8>::read(addr);
        for i in 0..arr.count.max(0) as usize {
            destroy_value_at(inner, arr.data.add(i * elem) as *mut c_void);
        }
        if vals.is_empty() {
            if !arr.data.is_null() {
                (s.u_free)(arr.data as *mut c_void);
            }
            TArray::<u8>::write(addr, std::ptr::null_mut(), 0);
            return true;
        }
        let n = vals.len();
        let mut data = arr.data;
        if n > arr.max.max(0) as usize {
            let bytes = n * elem;
            let new = if data.is_null() {
                (s.u_malloc)(bytes)
            } else {
                (s.u_realloc)(data as *mut c_void, bytes)
            };
            if new.is_null() {
                log::warn!("unrealsdk-rs: set_property: array allocation failed");
                TArray::<u8>::write(addr, std::ptr::null_mut(), 0);
                return false;
            }
            data = new as *mut u8;
        }
        std::ptr::write_bytes(data, 0, n * elem);
        for (i, v) in vals.iter().enumerate() {
            if !set_value_at(inner, data.add(i * elem) as *mut c_void, v) {
                TArray::<u8>::write(addr, data, i as i32);
                return false;
            }
        }
        TArray::<u8>::write(addr, data, n as i32);
        true
    }
}

/// [`destroy_property_value`] at an already-resolved `addr`. Static
/// arrays destroy every element.
fn destroy_value_at(prop: *mut ZProperty, addr: *mut c_void) {
    let dim = static_dim(prop);
    if dim > 1 {
        let Some(elem) = property_element_size(prop) else {
            return;
        };
        let elem = elem.max(0) as usize;
        for i in 0..dim {
            destroy_scalar_at(prop, unsafe { addr.byte_add(i * elem) });
        }
    } else {
        destroy_scalar_at(prop, addr);
    }
}

/// [`destroy_value_at`] for one element slot.
fn destroy_scalar_at(prop: *mut ZProperty, addr: *mut c_void) {
    let Some(s) = sys() else {
        return;
    };
    let Some(class) = property_class_name(prop) else {
        return;
    };
    // SAFETY: `addr` points at this property's live storage.
    unsafe {
        match class.as_str() {
            "StrProperty" => {
                let fstr = &mut *(addr as *mut crate::types::UnmanagedFString);
                if !fstr.data.is_null() {
                    (s.u_free)(fstr.data as *mut c_void);
                }
                crate::types::TArray::<u16>::write(addr, std::ptr::null_mut(), 0);
            }
            "TextProperty" => {
                // Assigning an empty text releases the old `FTextData`
                // engine-side (FText assignment).
                ftext_from_str(addr as *mut crate::types::FText, "");
            }
            "ArrayProperty" => {
                if let (Some(inner), Some(elem)) = (property_inner(prop), property_element_size_inner(prop)) {
                    let elem = elem.max(0) as usize;
                    let arr = crate::types::TArray::<u8>::read(addr);
                    if elem > 0 {
                        for i in 0..arr.count.max(0) as usize {
                            destroy_value_at(inner, arr.data.add(i * elem) as *mut c_void);
                        }
                    }
                    if !arr.data.is_null() {
                        (s.u_free)(arr.data as *mut c_void);
                    }
                    crate::types::TArray::<u8>::write(addr, std::ptr::null_mut(), 0);
                }
            }
            "StructProperty" => {
                if let Some(st) = property_struct(prop) {
                    for f in struct_fields(st as *const c_void) {
                        if let Some(off) = property_offset(f) {
                            destroy_value_at(
                                f,
                                (addr as *mut u8).add(off.max(0) as usize) as *mut c_void,
                            );
                        }
                    }
                }
            }
            "MulticastDelegateProperty" => {
                // Entries are POD `{UObject*, FName}` — free the buffer
                // and zero the header (`WrappedMulticastDelegate::clear`,
                // `wrapped_multicast_delegate.cpp`).
                let arr = crate::types::TArray::<u8>::read(addr);
                if !arr.data.is_null() {
                    (s.u_free)(arr.data as *mut c_void);
                }
                crate::types::TArray::<u8>::write(addr, std::ptr::null_mut(), 0);
            }
            "SoftObjectProperty" | "SoftClassProperty" => {
                // Only the identifier's subpath is owned
                // (`persistent_object_ptr_property.cpp` frees
                // exactly it).
                let subpath =
                    (addr as *mut u8).add(SOFT_SUBPATH_OFF) as *mut crate::types::UnmanagedFString;
                if !(*subpath).data.is_null() {
                    (s.u_free)((*subpath).data as *mut c_void);
                }
                crate::types::TArray::<u16>::write(
                    subpath as *mut c_void,
                    std::ptr::null_mut(),
                    0,
                );
            }
            // Scalars, names, enums, objects, weak pairs and delegates
            // hold no owned heap.
            _ => {}
        }
    }
}

/// `property_element_size` of an `ArrayProperty`'s inner prop, if any.
fn property_element_size_inner(prop: *const ZProperty) -> Option<i32> {
    property_element_size(property_inner(prop)?)
}

// ---------------------------------------------------------------------------
// Fixed-layout value shapes (game ABI)
// ---------------------------------------------------------------------------

/// The game's `FScriptDelegate` footprint on WILLOW: `{UObject* object;
/// FName func_name}` (`fscriptdelegate.h`, where
/// `HAS_NATIVE_WEAK_POINTERS` is false — one pointer plus an 8-byte
/// `FName`).
const SCRIPT_DELEGATE_SIZE: usize = crate::mem::GAME_PTR + 8;

/// `TPersistentObjectPtr` game layout (`tpersistentobjectptr.h`):
/// the `FWeakObjectPtr` `{index, serial}` pair (two `i32`,
/// `fweakobjectptr.h`) then a `tag` word, then the identifier. For
/// `FSoftObjectPath` (`tpersistentobjectptr.h`) that identifier is
/// `{FName asset_path_name; FString subpath}` — so on i686 the name sits
/// at +12 and the `TArray<u16>` subpath at +20.
const SOFT_NAME_OFF: usize = 2 * crate::mem::GAME_WORD + crate::mem::GAME_WORD;
const SOFT_SUBPATH_OFF: usize = SOFT_NAME_OFF + 8;

/// Read one `FScriptDelegate` (`{UObject*, FName}`) at `addr`.
fn read_script_delegate(addr: *const c_void) -> DelegateValue {
    // SAFETY: `addr` points at a live FScriptDelegate slot.
    unsafe {
        let object = *(addr as *const *mut UObject);
        let fname = read_field_fname(addr, crate::mem::GAME_PTR as u16);
        DelegateValue {
            object,
            function: fname_to_string(&fname).map(|t| t.text).unwrap_or_default(),
        }
    }
}

/// Write one `FScriptDelegate` at `addr` (function name resolved through
/// `FName`). `false` when the name cannot be resolved off-game.
fn write_script_delegate(addr: *mut c_void, d: &DelegateValue) -> bool {
    let Some(fname) = fname_from_str(&d.function, 0) else {
        return false;
    };
    // SAFETY: `addr` points at a writable FScriptDelegate slot.
    unsafe {
        *(addr as *mut *mut UObject) = d.object;
        *((addr as *mut u8).add(crate::mem::GAME_PTR) as *mut FName) = fname;
    }
    true
}

/// Replace a multicast delegate's invocation list: the
/// `TArray<FScriptDelegate>` behind `WrappedMulticastDelegate::push_back`/
/// `clear` (`wrapped_multicast_delegate.cpp`), wholesale. Entries
/// are POD on WILLOW — the old buffer frees with no per-element teardown.
fn set_multicast_value(addr: *mut c_void, vals: &[DelegateValue]) -> bool {
    use crate::types::TArray;
    let Some(s) = sys() else {
        return false;
    };
    // SAFETY: `addr` holds a live TArray header; allocator pairing matches
    // the engine's `FMalloc` free.
    unsafe {
        let arr = TArray::<u8>::read(addr);
        if !arr.data.is_null() {
            (s.u_free)(arr.data as *mut c_void);
        }
        if vals.is_empty() {
            TArray::<u8>::write(addr, std::ptr::null_mut(), 0);
            return true;
        }
        let data = (s.u_malloc)(vals.len() * SCRIPT_DELEGATE_SIZE) as *mut u8;
        if data.is_null() {
            log::warn!("unrealsdk-rs: set_property: delegate list allocation failed");
            TArray::<u8>::write(addr, std::ptr::null_mut(), 0);
            return false;
        }
        for (i, d) in vals.iter().enumerate() {
            if !write_script_delegate(data.add(i * SCRIPT_DELEGATE_SIZE) as *mut c_void, d) {
                TArray::<u8>::write(addr, data, i as i32);
                return false;
            }
        }
        TArray::<u8>::write(addr, data, vals.len() as i32);
        true
    }
}

/// Read a soft object pointer's identifier path (see [`SOFT_NAME_OFF`]).
fn read_soft_object(addr: *const c_void) -> Option<SoftObjectValue> {
    // SAFETY: `addr` holds a live soft object pointer value.
    unsafe {
        let name = read_field_fname(addr, SOFT_NAME_OFF as u16);
        let asset_path_name = fname_to_string(&name)?.text;
        let sub =
            &*((addr as *const u8).add(SOFT_SUBPATH_OFF) as *const crate::types::UnmanagedFString);
        let len = sub.count.max(0) as usize;
        let subpath = decode_wide(sub.data, len)?.trim_end_matches('\0').to_owned();
        Some(SoftObjectValue {
            asset_path_name,
            subpath,
        })
    }
}

/// Write a soft object pointer's identifier path and clear the resolution
/// cache (the weak pair + tag): the shape `TPersistentObjectPtr` takes
/// after deserialization, resolved lazily through the path by the engine.
/// The BL3-only `fsoftobjectptr_assign` cannot stand in here — its WILLOW
/// hook throws `version_error` (`abstract_hook.cpp`), which would
/// cross the C ABI as a C++ exception.
fn set_soft_object(addr: *mut c_void, v: &SoftObjectValue) -> bool {
    let Some(fname) = fname_from_str(&v.asset_path_name, 0) else {
        return false;
    };
    // SAFETY: `addr` holds a writable soft object pointer value.
    unsafe {
        std::ptr::write_bytes(addr as *mut u8, 0, SOFT_NAME_OFF);
        *((addr as *mut u8).add(SOFT_NAME_OFF) as *mut FName) = fname;
        set_fstring(
            (addr as *mut u8).add(SOFT_SUBPATH_OFF) as *mut c_void,
            &v.subpath,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_layouts_are_derived_not_guessed() {
        // `FScriptDelegate` = `{UObject*, FName}` (fscriptdelegate.h).
        assert_eq!(SCRIPT_DELEGATE_SIZE, crate::mem::GAME_PTR + 8);
        // `TPersistentObjectPtr` = weak pair (two `i32`) + `tag` +
        // `{FName, FString}` identifier (tpersistentobjectptr.h).
        assert_eq!(SOFT_NAME_OFF, 3 * crate::mem::GAME_WORD);
        assert_eq!(SOFT_SUBPATH_OFF, SOFT_NAME_OFF + 8);
    }

    #[test]
    fn kinds_cover_the_new_variants() {
        assert_eq!(PropValue::MulticastDelegate(vec![]).kind(), "MulticastDelegate");
        assert_eq!(PropValue::StaticArray(vec![]).kind(), "StaticArray");
        assert_eq!(
            PropValue::WeakObject(WeakObjectValue {
                object_index: 0,
                object_serial: 0,
            })
            .kind(),
            "WeakObject"
        );
    }
}
