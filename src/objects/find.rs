//! Object lookup, creation and offset-driven object field access.

use std::ffi::c_void;

use crate::load_offsets;
use crate::sys::sys;
use crate::types::{
    read_field_fname, read_field_i32, read_field_ptr, read_field_u64, write_field_u64, FName,
    UClass, UObject,
};

use super::alloc::UFreeString;
use super::text::{encode_wide, fname_from_str, fname_to_string};

/// `uobject_path_name` (`unrealsdk_main.cpp`). `None` on null object
/// or off-game.
pub fn object_path_name(obj: *const UObject) -> Option<UFreeString> {
    if obj.is_null() {
        return None;
    }
    let s = sys()?;
    let mut size: usize = 0;
    // SAFETY: resolved export; `size` is the `size_t&` out-param.
    let ptr = unsafe { (s.uobject_path_name)(obj, &mut size) };
    if ptr.is_null() {
        return None;
    }
    Some(UFreeString { ptr, len: size })
}

// ---------------------------------------------------------------------------
// Object lookup / lifetime
// ---------------------------------------------------------------------------

/// `find_object` (`unrealsdk_main.cpp`). `class: None` searches all
/// classes (null `UClass*`). `None` when not found or off-game.
pub fn find_object(class: Option<*mut UClass>, name: &str) -> Option<*mut UObject> {
    let s = sys()?;
    let wide = encode_wide(name);
    // SAFETY: resolved export; `wide` alive for the call.
    let obj = unsafe {
        (s.find_object)(
            class.unwrap_or(std::ptr::null_mut()),
            wide.as_ptr(),
            wide.len(),
        )
    };
    if obj.is_null() {
        None
    } else {
        Some(obj)
    }
}

/// `find_class_cstr` (`find_class.cpp`). The string overload resolves
/// **fully qualified paths** — it delegates to `find_object`
/// (`namedobjectcache.h`) — so pass `"Engine.Texture2D"`, not
/// `"Texture2D"`. Bare-name lookup goes through [`find_class_by_fname`]
/// (the FName overload's per-class cache, `namedobjectcache.h`).
/// `None` when not found or off-game.
pub fn find_class(name: &str) -> Option<*mut UClass> {
    let s = sys()?;
    let wide = encode_wide(name);
    // SAFETY: resolved export; `wide` alive for the call.
    let cls = unsafe { (s.find_class_cstr)(wide.as_ptr(), wide.len()) };
    if cls.is_null() {
        None
    } else {
        Some(cls)
    }
}

/// `find_class_fname` (`find_class.cpp`).
pub fn find_class_by_fname(name: &FName) -> Option<*mut UClass> {
    let s = sys()?;
    // SAFETY: resolved export.
    let cls = unsafe { (s.find_class_fname)(name) };
    if cls.is_null() {
        None
    } else {
        Some(cls)
    }
}

/// `construct_object` (`unrealsdk_main.cpp`). `name: None` passes a
/// null `FName*` (the SDK default-constructs one). `template_obj: None` is
/// null. `None` on null class/outer or off-game.
pub fn construct_object(
    cls: *mut UClass,
    outer: *mut UObject,
    name: Option<&str>,
    flags: u64,
    template_obj: Option<*mut UObject>,
) -> Option<*mut UObject> {
    if cls.is_null() || outer.is_null() {
        return None;
    }
    let s = sys()?;
    // `fname_from_str` needs the SDK too; a failed init aborts, passing null
    // instead would silently misname — prefer `None`.
    let cname = match name {
        Some(n) => Some(fname_from_str(n, 0)?),
        None => None,
    };
    let name_ptr = cname
        .as_ref()
        .map_or(std::ptr::null(), |n| n as *const FName);
    // SAFETY: resolved export; `cname` outlives the call.
    let obj = unsafe {
        (s.construct_object)(
            cls,
            outer,
            name_ptr,
            flags,
            template_obj.unwrap_or(std::ptr::null_mut()),
        )
    };
    if obj.is_null() {
        None
    } else {
        Some(obj)
    }
}

/// `load_package` (`unrealsdk_main.cpp`; may block for seconds while
/// the package loads — never call on the frame thread). `flags` is the
/// engine's `ELoadFlags` word (`UObject::LoadPackage(UPackage*, const
/// TCHAR*, DWORD LoadFlags)`, `UnObjBas.h`), see
/// [`crate::flags::load_flags`]; `0` is the usual value.
pub fn load_package(name: &str, flags: u32) -> Option<*mut UObject> {
    let s = sys()?;
    let wide = encode_wide(name);
    // SAFETY: resolved export; `wide` alive for the call.
    let obj = unsafe { (s.load_package)(wide.as_ptr(), wide.len(), flags) };
    if obj.is_null() {
        None
    } else {
        Some(obj)
    }
}


// ---------------------------------------------------------------------------
// Offset-driven object readers
// ---------------------------------------------------------------------------

/// `UObject.Class` via the table (`uobject.h`). `None` on null object
/// or off-game.
pub fn object_class(obj: *const UObject) -> Option<*mut UClass> {
    if obj.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: `obj` is a live UObject; offset comes from the live table.
    Some(unsafe { read_field_ptr(obj as *const c_void, table.uobject_class()) })
}

/// `UObject.Name` via the table.
pub fn object_name(obj: *const UObject) -> Option<FName> {
    if obj.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: as above.
    Some(unsafe { read_field_fname(obj as *const c_void, table.uobject_name()) })
}

/// `UObject.Outer` via the table.
pub fn object_outer(obj: *const UObject) -> Option<*mut UObject> {
    if obj.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: as above.
    Some(unsafe { read_field_ptr(obj as *const c_void, table.uobject_outer()) })
}

/// `UObject.InternalIndex` via the table (`uobject.h`): the object's slot
/// in `GObjects`.
pub fn object_index(obj: *const UObject) -> Option<i32> {
    if obj.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: as above.
    Some(unsafe { read_field_i32(obj as *const c_void, table.uobject_internal_index()) })
}


/// `UObject.ObjectFlags` as the full 64-bit `EObjectFlags` word
/// (`UnObjBas.h` typedefs it `QWORD`; the SDK agrees,
/// `game/bl2/offsets.h`). A `u32` read would truncate the high half
/// where `RF_Public`, `RF_Standalone`, `RF_Transient`, `RF_PendingKill`
/// and friends live (see [`crate::flags::object_flags`]). `None`
/// off-game or on a null object.
pub fn object_flags(obj: *const UObject) -> Option<u64> {
    if obj.is_null() {
        return None;
    }
    let table = load_offsets()?;
    // SAFETY: live object; offset from the live table.
    Some(unsafe { read_field_u64(obj as *const c_void, table.uobject_flags()) })
}

/// `set_object_flags`: the write side of [`object_flags`], same two-`u32`
/// word layout. Overwrites the whole `EObjectFlags` word. Read with
/// [`object_flags`] first and OR in what you want, or use
/// [`crate::objects::Obj::with_flags`] for read-modify-write. `false` on a
/// null object or off-game.
pub fn set_object_flags(obj: *mut UObject, flags: u64) -> bool {
    if obj.is_null() {
        return false;
    }
    let Some(table) = load_offsets() else {
        return false;
    };
    // SAFETY: live object; offset from the live table.
    unsafe { write_field_u64(obj as *mut c_void, table.uobject_flags(), flags) };
    true
}

/// The live `GObjects` table as a snapshot of object pointers (the
/// `TArray<UObject*>` behind the SDK's `GObjects` wrapper, `flavour.h`;
/// `GOBJECTS_FORMAT == TARRAY` on WILLOW). `None` off-game.
///
/// A *copy*, deliberately: the underlying `TArray` reallocates as the game
/// spawns objects, so a borrowed slice could dangle mid-frame. Entries are
/// engine-owned and unpinned — an object the GC reaps between calls may
/// vanish from the snapshot.
pub fn objects() -> Option<Vec<*mut UObject>> {
    let s = crate::sys::sys()?;
    // SAFETY: the SDK owns the wrapper and the array it points at for the
    // lifetime of the game process; the copy is taken immediately.
    unsafe {
        let wrapper = (s.gobjects)();
        if wrapper.is_null() {
            return None;
        }
        let arr = &*(*wrapper).internal;
        if arr.data.is_null() || arr.count < 0 {
            return Some(Vec::new());
        }
        Some(std::slice::from_raw_parts(arr.data, arr.count as usize).to_vec())
    }
}

/// The number of entries in the live `GObjects` table (`None` off-game).
pub fn object_count() -> Option<usize> {
    let s = crate::sys::sys()?;
    // SAFETY: see `objects`.
    unsafe {
        let wrapper = (s.gobjects)();
        if wrapper.is_null() {
            return None;
        }
        let arr = &*(*wrapper).internal;
        Some(arr.count.max(0) as usize)
    }
}

/// Object `i` of the live `GObjects` table (bounds-checked, `None` off-game
/// or out of range). The slot may hold null for a GC'd entry.
pub fn object_at(i: usize) -> Option<*mut UObject> {
    let s = crate::sys::sys()?;
    // SAFETY: see `objects`; the index is bounds-checked against the live
    // header before the dereference.
    unsafe {
        let wrapper = (s.gobjects)();
        if wrapper.is_null() {
            return None;
        }
        let arr = &*(*wrapper).internal;
        if i >= arr.count.max(0) as usize {
            return None;
        }
        Some(*arr.data.add(i))
    }
}

/// Every live object whose class is `cls` or derives from it
/// ([`is_kind_of`]). `None` off-game.
pub fn objects_kind_of(cls: *mut crate::types::UClass) -> Option<Vec<*mut UObject>> {
    let all = objects()?;
    Some(
        all.into_iter()
            .filter(|&obj| is_kind_of(obj, cls) == Some(true))
            .collect(),
    )
}

/// Every live object of the class named `class_name` (or derived from it):
/// [`objects_kind_of`] with the class resolved by [`find_engine_class`].
/// `None` off-game or when the class cannot be found.
pub fn objects_of_class_name(class_name: &str) -> Option<Vec<*mut UObject>> {
    let cls = find_engine_class(class_name)?;
    objects_kind_of(cls)
}

/// An object's name as text (`?` when unreadable).
pub fn object_name_text(obj: *const UObject) -> String {
    object_name(obj)
        .and_then(|name| fname_to_string(&name))
        .map(|t| t.text)
        .unwrap_or_else(|| "?".to_owned())
}

/// An object's class name as text (empty when unreadable).
pub fn object_class_name(obj: *mut UObject) -> String {
    object_class(obj)
        .map(|cls| object_name_text(cls as *const UObject))
        .unwrap_or_default()
}

/// An object's full path as text, `None` when unreadable.
pub fn object_path_text(obj: *mut UObject) -> Option<String> {
    object_path_name(obj).and_then(|s| s.to_string())
}

/// Engine class lookup: `Engine.<name>` first, then the bare name. The
/// *string* [`find_class`] resolves fully qualified paths
/// (`namedobjectcache.h` delegates to `find_object`), so a bare
/// class name only matches through the FName overload's cache — try both.
pub fn find_engine_class(name: &str) -> Option<*mut UClass> {
    find_class(&format!("Engine.{name}")).or_else(|| {
        let bare = fname_from_str(name, 0)?;
        find_class_by_fname(&bare)
    })
}

/// Whether `obj`'s class is `base` or derives from it (walks
/// `UStruct::SuperField`). `None` off-game or on null inputs.
pub fn is_kind_of(obj: *const UObject, base: *const UClass) -> Option<bool> {
    if obj.is_null() || base.is_null() {
        return None;
    }
    let table = load_offsets()?;
    let mut cls = object_class(obj)?;
    // SAFETY: live class chain; offsets from the live table.
    unsafe {
        loop {
            if std::ptr::eq(cls, base) {
                return Some(true);
            }
            cls = read_field_ptr(cls as *const c_void, table.ustruct_superfield());
            if cls.is_null() {
                return Some(false);
            }
        }
    }
}
