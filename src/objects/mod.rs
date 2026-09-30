//! The object and property API over [`crate::sys`]: find/call/prop access
//! with `u_free` RAII, [`Obj`] handles, typed property values ([`PropValue`])
//! and marshalled function calls.
//!
//! Two altitudes over the same engine metadata:
//!
//! - free functions taking live engine pointers (`*mut UObject`,
//!   `*mut ZProperty`, ...) — what [`crate::hooks`] and [`crate::commands`]
//!   use internally (consumer code builds on them too, like the texloader
//!   `textures` crate's `materials` rebind);
//! - the [`Obj`] handle with `macro_rules!` sugar on top of them
//!   (`obj!`/`get!`/`set!`/`call!`, exported at the crate root).
//!
//! The Python SDK can say
//! `find_object("Object", "Transient.SomeObject").ChildObject.OtherObject.Function()`.
//! Rust field access is compile-time bound (there is no `__getattr__`
//! hook), so the literal `.ChildObject` syntax is not expressible. What
//! this module provides is the same lookups behind method chains and
//! `macro_rules!` sugar that takes dotted idents and expands to them at
//! compile time:
//!
//! ```ignore
//! use unrealsdk_rs::{call, get, obj, set};
//! use unrealsdk_rs::objects::{self, Obj};
//!
//! let root = Obj::new(objects::find_object(None, "Transient.SomeObject")?)?;
//! let go = obj!(root, ChildObject.OtherObject)?;   // property-deref chain
//! let hp = get!(go, Health as f32)?;               // typed read
//! set!(go, Health, 2.0f32)?;                       // checked write
//! let outcome = call!(go, ToggleTimer, 1i32)?;     // marshalled call
//! ```
//!
//! On type safety: property and function names are runtime game metadata
//! (they differ per game and per mod loadout), so no compiler can check
//! them. A miss is a [`Error::NotFound`] at the call site, exactly
//! like Python's `AttributeError`, just as a `Result`. What *is* checked
//! is the value side: [`Obj::get`] verifies the live `ZProperty` class
//! before converting, and [`Obj::set`] refuses a value variant that
//! matches nothing in the property before writing a byte.
//!
//! Rules: every public function returns `Option`/`Result`, never panics,
//! never touches Python. Off-game (or SDK-teardown) everything degrades to
//! `None`/`false`/[`Error::NotInGame`]. All Unreal field access goes
//! through [`crate::types::OffsetTable`] — no hardcoded offsets.
//!
//! Property-type coverage matrix (WILLOW/UE3 class names, checked against
//! the property's `UObject.Class.Name` before any read/write):
//!
//! | class | Rust | get | set |
//! |---|---|---|---|
//! | `IntProperty` | `i32` | yes | yes |
//! | `FloatProperty` | `f32` | yes | yes |
//! | `BoolProperty` | `bool` | yes (u32 word + `FieldMask`, BL2 layout `game/bl2/offsets.h`) | yes (read-modify-write) |
//! | `ByteProperty` | `u8`/`Enum` | yes (plain byte, or `Enum` when `Enum != null`) | yes |
//! | `EnumProperty` | `Enum` | yes (via `UnderlyingProp`, zenumproperty.h) | yes |
//! | `NameProperty` | `Name` | yes | yes |
//! | `StrProperty` | `String` | yes (`UnmanagedFString` = `TArray<wchar_t>`, `structs/fstring.h`) | yes (allocator-owned assign, see [`set_property`]) |
//! | `ObjectProperty`/`ClassProperty`/`InterfaceProperty` | `*mut UObject` | yes | yes (raw pointer write) |
//! | `StructProperty` | `Struct` (recursive) | yes | yes (per-field) |
//! | `ArrayProperty` | `Array` (recursive) | yes | yes (grows via `u_realloc`) |
//! | `DelegateProperty` | `Delegate` | yes | yes |
//! | `MulticastDelegateProperty` | `MulticastDelegate` | yes (the `TArray<FScriptDelegate>` invocation list, `wrapped_multicast_delegate.h`) | yes (wholesale list replace) |
//! | `WeakObjectProperty` | `WeakObject` | yes (raw `{index, serial}` pair — UE3 has no serial table to resolve against, `gobjects.cpp`) | yes (raw pair) |
//! | `SoftObjectProperty`/`SoftClassProperty` | `SoftObject` | yes (identifier path only) | yes (path write + cleared resolution cache) |
//! | `LazyObjectProperty` | — | no | no (BL3-only upstream support) |
//!
//! Static arrays (`ArrayDim > 1`) round-trip as `PropValue::StaticArray`,
//! or per element via [`get_property_at`]/[`set_property_at`] (and
//! [`Obj::get_at`]/[`Obj::set_at`]).
//!
//! `TextProperty` is write-only (`PropValue::String` in): `FTextData` is
//! opaque on WILLOW (`FTEXT_FORMAT_NOT_IMPLEMENTED`, `flavour.h`) and
//! the C API exports no text getter. Weak/soft object pointers cannot be
//! *resolved* to live objects on WILLOW (upstream's weak-pointer path
//! throws `version_error` on the UE3 `GObjects` format,
//! `gobjects.cpp`) — the raw pair / identifier path is what
//! round-trips instead. Still deferred: GBX
//! def-ptr/inline-struct/game-data-handle and attribute properties.
//! A type mismatch never panics: it logs at `warn` and returns `None`.
//! (Virtual-call helpers live under "Virtual calls" in the `calls`
//! submodule — [`post_edit_change_property`] — mirroring the SDK's own
//! `UObject::post_edit_change_property`.)

// The functions here take engine pointers and are deliberately *safe* fns:
// they null-check and return `Option`/`bool`, and a *live* engine pointer is
// the caller's contract (the same contract the SDK's own C++ API states —
// see the module table). Making them `unsafe fn` would push `unsafe` into
// every caller without lifting that contract; the lint rejects this API
// choice, so it is opted out of here, once, deliberately.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_void;
use std::fmt;

use crate::flags::property_flags::CPF_OPTIONAL_PARM;
use crate::types::{BoundFunction, UClass, UFunction, UObject, ZProperty};

mod alloc;
mod calls;
mod convert;
mod find;
mod macros;
pub mod refs;
mod reflect;
mod text;
mod values;

pub use alloc::UBuffer;
pub use calls::{
    call_bound_function, post_edit_change_chain_property, post_edit_change_chain_property_index,
    post_edit_change_property, post_edit_change_property_index, process_event,
    DEFAULT_POST_EDIT_CHANGE_CHAIN_PROPERTY_VF_IDX, DEFAULT_POST_EDIT_CHANGE_PROPERTY_VF_IDX,
};
pub use find::{
    construct_object, find_class, find_class_by_fname, find_engine_class, find_object, is_kind_of,
    load_package, object_at, object_class, object_class_name, object_count, object_flags,
    object_index, object_name, object_name_text, object_outer, object_path_name,
    object_path_text, objects, objects_kind_of, objects_of_class_name, set_object_flags,
};
pub use reflect::{
    all_uenums, class_default_object, class_interfaces, const_value, enum_names, find_field,
    find_function, find_property, function_flags, function_params, function_params_size,
    function_return_param, implements_interface, property_addr, property_array_dim,
    property_class_name, property_element_size, property_enum, property_flags, property_inner,
    property_offset, property_struct, property_underlying, struct_fields, UEnumInfo,
};
pub use text::{
    decode_wide, encode_wide, fname_from_str, fname_to_string, ftext_from_str, FNameText,
};
pub use values::{
    destroy_property_value, get_property, get_property_at, set_property, set_property_at,
    DelegateValue, PropValue, SoftObjectValue, StructValue, WeakObjectValue,
};
pub use crate::error::{Error, Result};

/// A handle to a live game object. `Copy`: it wraps the engine
/// pointer and keeps nothing alive (GC survival is `RF_*`'s job, see
/// [`crate::flags::object_flags`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obj(*mut UObject);

/// A property resolved along a path: its `ZProperty` and the base address
/// its `Offset_Internal` resolves against (object or struct start).
struct Resolved {
    base: *mut c_void,
    prop: *mut ZProperty,
}

impl Obj {
    /// Wrap a live object pointer; [`Error::NullObject`] on null.
    pub fn new(ptr: *mut UObject) -> Result<Self> {
        if ptr.is_null() {
            return Err(Error::NullObject);
        }
        Ok(Obj(ptr))
    }

    /// The wrapped engine pointer.
    pub fn raw(self) -> *mut UObject {
        self.0
    }

    /// Object name.
    pub fn name(self) -> Result<String> {
        self.live()?;
        Ok(object_name_text(self.0))
    }

    /// Full path name (`Outer.Outer.Name`).
    pub fn path(self) -> Result<String> {
        self.live()?;
        object_path_text(self.0).ok_or(Error::NotInGame)
    }

    /// The object's class.
    pub fn class(self) -> Result<Obj> {
        self.live()?;
        let cls = object_class(self.0).ok_or(Error::NotInGame)? as *mut UObject;
        Obj::new(cls)
    }

    /// The object's class name as text.
    pub fn class_name(self) -> Result<String> {
        self.live()?;
        Ok(object_class_name(self.0))
    }

    /// The object's outer (`None` for a top-level package object).
    pub fn outer(self) -> Result<Option<Obj>> {
        self.live()?;
        Ok(object_outer(self.0).and_then(|o| Obj::new(o).ok()))
    }

    /// `UObject.InternalIndex` (`uobject.h`): the object's slot in
    /// [`objects`](super::objects).
    pub fn index(self) -> Result<i32> {
        self.live()?;
        object_index(self.0).ok_or(Error::NotInGame)
    }

    /// Whether this object's class derives from `base`
    /// (`UObject::is_instance`, `uobject.cpp`). For interface checks
    /// see [`implements_interface`].
    pub fn is_a(self, base: *mut UClass) -> Result<bool> {
        self.live()?;
        is_kind_of(self.0, base).ok_or(Error::NotInGame)
    }

    /// The `EObjectFlags` word ([`crate::flags::object_flags`]). Written
    /// back with [`Obj::set_flags`] / [`Obj::with_flags`].
    pub fn flags(self) -> Result<u64> {
        self.live()?;
        object_flags(self.0).ok_or(Error::NotInGame)
    }

    /// Overwrite the `EObjectFlags` word ([`crate::flags::object_flags`]).
    /// Replaces all 64 bits: read with [`Obj::flags`] and OR in what you
    /// want, or use [`Obj::with_flags`] for a read-modify-write.
    pub fn set_flags(self, flags: u64) -> Result<()> {
        self.live()?;
        if set_object_flags(self.0, flags) {
            Ok(())
        } else {
            Err(Error::NotInGame)
        }
    }

    /// Read-modify-write the `EObjectFlags` word: `update` receives the
    /// current flags and returns the new ones
    /// (`o.with_flags(|f| f | RF_ROOT_SET)?`). Returns the word written
    /// back. Not atomic against a concurrent flag writer (the 32-bit game
    /// ABI stores the word as two halves anyway).
    pub fn with_flags(self, update: impl FnOnce(u64) -> u64) -> Result<u64> {
        let flags = update(self.flags()?);
        self.set_flags(flags)?;
        Ok(flags)
    }

    /// Walk object-property derefs along `path` ("A.B.C"): each segment is
    /// an object-valued property, nulls stop the walk with
    /// [`Error::NullObject`].
    pub fn child(self, path: &str) -> Result<Obj> {
        let r = self.resolve(path)?;
        match self.class_name_inner(r.prop)?.as_str() {
            "ObjectProperty" | "ClassProperty" | "InterfaceProperty" => {
                // SAFETY: `resolve` proved the property and owner live;
                // this reads the reference stored at the slot.
                let obj = unsafe { *(self.slot(&r)? as *const *mut UObject) };
                Obj::new(obj)
            }
            other => Err(Error::TypeMismatch {
                expected: "object property".to_owned(),
                got: other.to_owned(),
            }),
        }
    }

    /// Read a property; `path` may cross object hops and struct hops
    /// ("A.MyStruct.Field").
    pub fn get_prop(self, path: &str) -> Result<PropValue> {
        let r = self.resolve(path)?;
        get_property(r.base, r.prop).ok_or_else(|| self.read_err(path))
    }

    /// Read a property and convert it (`get::<f32>("Health")`). The
    /// conversion is checked against the live property class.
    pub fn get<T: TryFrom<PropValue, Error = Error>>(self, path: &str) -> Result<T> {
        T::try_from(self.get_prop(path)?)
    }

    /// Read one element of a static-array property (`ArrayDim > 1`),
    /// bounds-checked (the SDK's `get<T>(name, idx)`, `prop_traits.h`).
    pub fn get_at(self, path: &str, idx: usize) -> Result<PropValue> {
        let r = self.resolve(path)?;
        get_property_at(r.base, r.prop, idx).ok_or_else(|| self.read_err(path))
    }

    /// Write one element of a static-array property, bounds-checked (the
    /// SDK's `set<T>(name, idx, value)`, `prop_traits.h`). [`Obj::set`]
    /// with a [`PropValue::StaticArray`] writes every element at once.
    pub fn set_at(self, path: &str, idx: usize, value: impl Into<PropValue>) -> Result<()> {
        let r = self.resolve(path)?;
        if set_property_at(r.base, r.prop, idx, &value.into()) {
            Ok(())
        } else {
            Err(Error::Unsupported(format!("write to '{path}[{idx}]'")))
        }
    }

    /// Write a property (refuses mismatched value variants before writing).
    pub fn set(self, path: &str, value: impl Into<PropValue>) -> Result<()> {
        let r = self.resolve(path)?;
        if set_property(r.base, r.prop, &value.into()) {
            Ok(())
        } else {
            Err(Error::Unsupported(format!("write to '{path}'")))
        }
    }

    /// Call a function with positional [`PropValue`] args, marshalled into
    /// the function's params block (`PropertyLink` order, the SDK's
    /// `func_params` walk). Out params and the return value come back in
    /// [`CallOutcome`]; the block is torn down again on the way out.
    pub fn call(self, name: &str, args: &[PropValue]) -> Result<CallOutcome> {
        self.live()?;
        let func = self.find_func(name)?;
        let props = function_params(func);
        let arg_err = |got: usize| Error::ArgCount {
            function: name.to_owned(),
            expected: props.len(),
            got,
        };
        if args.len() > props.len() {
            return Err(arg_err(args.len()));
        }
        // Trailing unprovided params must be optional (BL1E-style).
        for &p in &props[args.len()..] {
            if property_flags(p).is_none_or(|f| f & CPF_OPTIONAL_PARM == 0) {
                return Err(arg_err(args.len()));
            }
        }
        let size = function_params_size(func).ok_or(Error::NotInGame)? as usize;
        let block = UBuffer::alloc(size.max(1)).ok_or(Error::AllocFailed)?;
        let params = block.as_mut_ptr();
        for (&p, a) in props.iter().zip(args) {
            if !set_property(params, p, a) {
                return Err(Error::TypeMismatch {
                    expected: property_class_name(p).unwrap_or_else(|| "property".to_owned()),
                    got: a.kind().to_owned(),
                });
            }
        }
        let bound = BoundFunction {
            func,
            object: self.0,
        };
        // The SDK's locked call path (`bound_function_call_with_params`:
        // takes the call lock and toggles `FUNC_NATIVE` around
        // `process_event`).
        call_bound_function(&bound, params);
        // Read every out value back before tearing the block down.
        let mut out = Vec::new();
        for &p in &props {
            if let Some(v) = get_property(params, p) {
                out.push((object_name_text(p as *mut UObject), v));
            }
        }
        let ret_prop = function_return_param(func);
        let ret = ret_prop.and_then(|p| get_property(params, p));
        // Release what the call left owned (strings, arrays, structs);
        // `UBuffer` frees the block itself on drop.
        for &p in props.iter().chain(ret_prop.iter()) {
            destroy_property_value(params, p);
        }
        Ok(CallOutcome { ret, args: out })
    }

    /// The raw `ZProperty` behind `path`.
    pub fn find_prop(self, path: &str) -> Result<*mut ZProperty> {
        Ok(self.resolve(path)?.prop)
    }

    /// The raw `UFunction` named `name` on this object's class chain.
    pub fn find_func(self, name: &str) -> Result<*mut UFunction> {
        self.live()?;
        let class = object_class(self.0).ok_or(Error::NotInGame)?;
        find_function(class, name).ok_or_else(|| Error::NotFound(name.to_owned()))
    }

    fn live(self) -> Result<()> {
        if !crate::is_initialized() {
            return Err(Error::NotInGame);
        }
        if self.0.is_null() {
            return Err(Error::NullObject);
        }
        Ok(())
    }

    fn class_name_inner(self, prop: *mut ZProperty) -> Result<String> {
        property_class_name(prop).ok_or(Error::NotInGame)
    }

    fn read_err(self, path: &str) -> Error {
        if crate::is_initialized() {
            Error::Unsupported(format!("read '{path}'"))
        } else {
            Error::NotInGame
        }
    }

    /// The leaf's storage address (`base + Offset_Internal`).
    fn slot(self, r: &Resolved) -> Result<*mut c_void> {
        property_addr(r.base, r.prop)
            .ok_or_else(|| Error::Unsupported("property offset".to_owned()))
    }

    /// Resolve a dotted property path to its leaf property and owner base.
    /// Hops: object properties deref their reference, struct properties
    /// descend into the struct's storage.
    fn resolve(self, path: &str) -> Result<Resolved> {
        self.live()?;
        let segs: Vec<&str> = path.split('.').collect();
        if segs.is_empty() || segs.iter().any(|s| s.is_empty()) {
            return Err(Error::NotFound(path.to_owned()));
        }
        let mut ty = object_class(self.0).ok_or(Error::NotInGame)? as *const c_void;
        let mut base = self.0 as *mut c_void;
        let last = segs.len() - 1;
        for (i, seg) in segs.iter().enumerate() {
            let prop = find_field(ty, seg).ok_or_else(|| Error::NotFound(path.to_owned()))?;
            if i == last {
                return Ok(Resolved { base, prop });
            }
            let r = Resolved { base, prop };
            let addr = self.slot(&r)?;
            match self.class_name_inner(prop)?.as_str() {
                "ObjectProperty" | "ClassProperty" | "InterfaceProperty" => {
                    // SAFETY: slot read from live property storage.
                    let obj = unsafe { *(addr as *const *mut UObject) };
                    if obj.is_null() {
                        return Err(Error::NullObject);
                    }
                    ty = object_class(obj).ok_or(Error::NotInGame)? as *const c_void;
                    base = obj as *mut c_void;
                }
                "StructProperty" => {
                    ty = property_struct(prop).ok_or(Error::NotInGame)? as *const c_void;
                    base = addr;
                }
                other => {
                    return Err(Error::TypeMismatch {
                        expected: "object or struct property".to_owned(),
                        got: other.to_owned(),
                    })
                }
            }
        }
        Err(Error::NotFound(path.to_owned()))
    }
}

impl fmt::Display for Obj {
    /// The object's full path, or its raw address when unreadable.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match object_path_text(self.0) {
            Some(p) => write!(f, "{p}"),
            None => write!(f, "{:#x}", self.0 as usize),
        }
    }
}

/// The outcome of [`Obj::call`]: the return value (when the function has a
/// `CPF_ReturnParm`) plus every parameter's post-call value (out params
/// read back; in params read back as given). Parameters whose class is not
/// readable (see the coverage matrix in the module docs) are omitted.
#[derive(Clone, Debug, PartialEq)]
pub struct CallOutcome {
    pub ret: Option<PropValue>,
    pub args: Vec<(String, PropValue)>,
}

impl CallOutcome {
    /// The return value converted to `T`.
    pub fn ret_as<T: TryFrom<PropValue, Error = Error>>(self) -> Result<T> {
        let v = self.ret.ok_or_else(|| Error::NotFound("return value".to_owned()))?;
        T::try_from(v)
    }

    /// One named parameter's post-call value (out params read back after
    /// the call; in params read back as given). `None` for parameters
    /// whose class is not readable or that the call did not carry.
    pub fn arg(&self, name: &str) -> Option<&PropValue> {
        self.args.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

fn mismatch(expected: &str, v: &PropValue) -> Error {
    Error::TypeMismatch {
        expected: expected.to_owned(),
        got: v.kind().to_owned(),
    }
}
