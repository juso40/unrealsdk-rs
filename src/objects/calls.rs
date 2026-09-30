//! Calls into engine code: `ProcessEvent`, bound functions and the
//! `PostEditChangeProperty` vtable dispatch.

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

use crate::sys::sys;
use crate::types::{BoundFunction, UFunction, UObject, ZProperty};

// ---------------------------------------------------------------------------
// Calls
// ---------------------------------------------------------------------------

/// `process_event` (`unrealsdk_main.cpp`). All pointers must be
/// non-null (a null params block would corrupt the engine) — `false`
/// otherwise, or off-game. Prefer a [`UBuffer`](super::UBuffer) sized by the function's
/// params for `params`.
pub fn process_event(object: *mut UObject, function: *mut UFunction, params: *mut c_void) -> bool {
    if object.is_null() || function.is_null() || params.is_null() {
        return false;
    }
    let Some(s) = sys() else { return false };
    // SAFETY: resolved export; caller guarantees live object/function/params.
    unsafe { (s.process_event)(object, function, params) };
    true
}

/// `bound_function_call_with_params` (`bound_function.cpp`; sets
/// `FUNC_NATIVE` and takes the function-call lock internally).
pub fn call_bound_function(bound: &BoundFunction, params: *mut c_void) -> bool {
    if bound.func.is_null() || bound.object.is_null() || params.is_null() {
        return false;
    }
    let Some(s) = sys() else { return false };
    // SAFETY: resolved export; caller guarantees the bound pair + params.
    unsafe { (s.bound_function_call_with_params)(bound, params) };
    true
}

// ---------------------------------------------------------------------------
// Virtual calls
// ---------------------------------------------------------------------------

/// Word index of `UObject::PostEditChangeProperty` in the object vtable on
/// WILLOW (BL2) — `flavour.h` (`PostEditChangeChainProperty` is the
/// neighbouring 18). A per-build override exists through the SDK's own
/// config key (`uobject.cpp` reads the same one, so both layers
/// always agree).
pub const DEFAULT_POST_EDIT_CHANGE_PROPERTY_VF_IDX: usize = 19;

/// Word index of `UObject::PostEditChangeChainProperty` in the object
/// vtable on WILLOW (`flavour.h`), the nested-struct edit event.
/// Config override `unrealsdk.uobject_post_edit_change_chain_property_vf_index`
/// (`uobject.cpp` reads the same one).
pub const DEFAULT_POST_EDIT_CHANGE_CHAIN_PROPERTY_VF_IDX: usize = 18;

/// Resolve one vtable-index config override (cached per key): the SDK
/// config `key` (the shipped `-1` means "no override") or `default`.
fn config_vf_index(index: &OnceLock<usize>, key: &'static str, default: usize) -> usize {
    *index.get_or_init(|| {
        let Some(s) = sys() else {
            return default;
        };
        let Some(config_get_int) = s.config_get_int else {
            return default;
        };
        let mut value: i64 = 0;
        // SAFETY: resolved export; buffers valid for the call.
        let found = unsafe { config_get_int(key.as_ptr() as *const c_char, key.len(), &mut value) };
        // Sanity-bound: the shipped default is -1 ("no override"), and a
        // bogus huge index would happily read off the vtable.
        if found && (0..512).contains(&value) {
            value as usize
        } else {
            default
        }
    })
}

/// The `PostEditChangeProperty` vtable slot in use: the SDK's config
/// override `unrealsdk.uobject_post_edit_change_property_vf_index` (the
/// shipped `-1` means "no override") or
/// [`DEFAULT_POST_EDIT_CHANGE_PROPERTY_VF_IDX`]. Resolved once and cached,
/// exactly like `uobject.cpp` does.
pub fn post_edit_change_property_index() -> usize {
    static INDEX: OnceLock<usize> = OnceLock::new();
    config_vf_index(
        &INDEX,
        "unrealsdk.uobject_post_edit_change_property_vf_index",
        DEFAULT_POST_EDIT_CHANGE_PROPERTY_VF_IDX,
    )
}

/// The `PostEditChangeChainProperty` vtable slot in use
/// ([`DEFAULT_POST_EDIT_CHANGE_CHAIN_PROPERTY_VF_IDX`] or its config
/// override, as [`post_edit_change_property_index`]).
pub fn post_edit_change_chain_property_index() -> usize {
    static INDEX: OnceLock<usize> = OnceLock::new();
    config_vf_index(
        &INDEX,
        "unrealsdk.uobject_post_edit_change_chain_property_vf_index",
        DEFAULT_POST_EDIT_CHANGE_CHAIN_PROPERTY_VF_IDX,
    )
}

/// `UObject::post_edit_change_property(ZProperty*)`
/// (`uobject.cpp`): notify the engine of an external property
/// change by dispatching the object's `PostEditChangeProperty` virtual —
/// the SDK's own mechanism for exactly this (`uobject.h`). The
/// event is the game's `FPropertyChangedEvent` (`UnType.h`):
/// `{prop, bChangesTopology=0, ChangeType=CHANGE_TYPE_UNSPECIFIED,
/// ObjectIteratorIndex=-1, ArrayIndicesPerObject=null}`.
///
/// This is how texloader rebuilds a texture's render resource on BL2:
/// `UTexture::PostEditChangeProperty` ends in the native
/// `UTexture::UpdateResource()`, which is not reachable any other way (it
/// has no `UFunction`). Game thread only.
///
/// `false` off-game, on null inputs, or on a host that cannot perform the
/// dispatch (only 32-bit Windows can — the game's ABI).
pub fn post_edit_change_property(obj: *mut UObject, prop: *mut ZProperty) -> bool {
    if obj.is_null() || prop.is_null() {
        return false;
    }
    dispatch_post_edit_change_property(obj, prop, post_edit_change_property_index())
}

/// `UObject::PostEditChangeChainProperty` (`vftable` slot 18 on WILLOW,
/// `flavour.h`): the nested-struct edit event. `prop` is the leaf
/// property that changed and `chain` lists it and its member ancestors
/// toward the root, in `FEditPropertyChain` order (`uobject.cpp`).
/// Use this (not [`post_edit_change_property`]) when the changed value
/// lives inside a struct member — handlers dispatch off the chain.
/// Game thread only.
///
/// `false` off-game, on null inputs, an empty chain, or on a host that
/// cannot perform the dispatch (only 32-bit Windows can).
pub fn post_edit_change_chain_property(
    obj: *mut UObject,
    prop: *mut ZProperty,
    chain: &[*mut ZProperty],
) -> bool {
    if obj.is_null() || prop.is_null() || chain.is_empty() {
        return false;
    }
    dispatch_post_edit_change_chain_property(
        obj,
        prop,
        chain,
        post_edit_change_chain_property_index(),
    )
}

/// The actual vtable dispatch, compiled only for the 32-bit Windows game
/// ABI; a stub elsewhere keeps off-game tests building (`uobject.h`:
/// `call_virtual_function` is `vftable[index]` invoked `__thiscall`).
#[cfg(all(windows, target_arch = "x86"))]
fn dispatch_post_edit_change_property(
    obj: *mut UObject,
    prop: *mut ZProperty,
    index: usize,
) -> bool {
    // The game's `FPropertyChangedEvent` (`UnType.h`), 20 bytes
    // on i686. Not the SDK's four-word spelling
    // (`fpropertychangeevent.h`): word 2 there is named
    // `member_property` where the engine reads `UBOOL bChangesTopology`,
    // and its shape omits the trailing `ArrayIndicesPerObject*` (a
    // handler calling `GetArrayIndex` would read garbage past the
    // struct). `ObjectIteratorIndex` uses the engine default of -1 ("out
    // of bounds/unused", `UnType.h`).
    #[repr(C)]
    struct FPropertyChangedEvent {
        property: *mut ZProperty,
        b_changes_topology: u32,
        change_type: u32,
        object_iterator_index: i32,
        array_indices_per_object: *const c_void,
    }
    // MSVC `__thiscall`: `this` in ECX, args on the stack, callee pops.
    // The stable-Rust stand-in is `__fastcall` (ECX + EDX + stack, callee
    // pops): ECX carries `this`, the dummy EDX slot is simply ignored by
    // the callee — same shim the C++ hooking world uses.
    type VirtualFn =
        unsafe extern "fastcall" fn(*mut c_void, usize, *mut FPropertyChangedEvent);

    let mut event = FPropertyChangedEvent {
        property: prop,
        b_changes_topology: 0,
        change_type: crate::flags::property_change_type::CHANGE_TYPE_UNSPECIFIED,
        object_iterator_index: -1,
        array_indices_per_object: std::ptr::null(),
    };
    // SAFETY: live object/property on the game thread; the slot comes from
    // the object's own vtable (same dispatch the SDK performs).
    unsafe {
        let vtable = *(obj as *const *const usize);
        if vtable.is_null() {
            return false;
        }
        let func = *vtable.add(index);
        if func == 0 {
            return false;
        }
        let f: VirtualFn = std::mem::transmute::<usize, VirtualFn>(func);
        f(obj as *mut c_void, 0, &mut event);
    }
    true
}

/// Non-Windows/64-bit hosts cannot perform the game's `__thiscall`.
#[cfg(not(all(windows, target_arch = "x86")))]
fn dispatch_post_edit_change_property(
    obj: *mut UObject,
    prop: *mut ZProperty,
    index: usize,
) -> bool {
    let _ = (obj, prop, index);
    false
}

/// The actual chain dispatch, compiled only for the 32-bit Windows game
/// ABI (same `__thiscall` shim as
/// [`dispatch_post_edit_change_property`]).
#[cfg(all(windows, target_arch = "x86"))]
fn dispatch_post_edit_change_chain_property(
    obj: *mut UObject,
    prop: *mut ZProperty,
    chain: &[*mut ZProperty],
    index: usize,
) -> bool {
    // Engine-shaped `FPropertyChangedChainEvent`: this crate's verified
    // `FPropertyChangedEvent` (see `dispatch_post_edit_change_property`)
    // plus the one trailing `PropertyChain` pointer.
    //
    // `FEditPropertyChain` mirrors `fpropertychangeevent.h`: a
    // doubly linked list of `{value, next, prev}` nodes with `head`,
    // `tail`, `size` and a zero-filled `dummy[64]` tail — the same
    // fabrication the C++ SDK ships (upstream's node order, upstream's
    // "zero-init seems to work well enough"). Nodes live in one Vec so
    // the links stay valid for the call.
    #[repr(C)]
    struct ChainNode {
        value: *mut ZProperty,
        next: *mut ChainNode,
        prev: *mut ChainNode,
    }
    #[repr(C)]
    struct FEditPropertyChain {
        head: *mut ChainNode,
        tail: *mut ChainNode,
        size: u32,
        _pad: [u8; 64],
    }
    #[repr(C)]
    struct FPropertyChangedChainEvent {
        property: *mut ZProperty,
        b_changes_topology: u32,
        change_type: u32,
        object_iterator_index: i32,
        array_indices_per_object: *const c_void,
        property_chain: *mut FEditPropertyChain,
    }
    type VirtualFn =
        unsafe extern "fastcall" fn(*mut c_void, usize, *mut FPropertyChangedChainEvent);

    let mut nodes: Vec<ChainNode> = chain
        .iter()
        .map(|&value| ChainNode {
            value,
            next: std::ptr::null_mut(),
            prev: std::ptr::null_mut(),
        })
        .collect();
    let base = nodes.as_mut_ptr();
    // SAFETY: `base.add(i)` stays inside `nodes`; links only point within
    // the allocation, which outlives the call below.
    unsafe {
        for i in 0..nodes.len() {
            if i > 0 {
                (*base.add(i)).prev = base.add(i - 1);
            }
            if i + 1 < nodes.len() {
                (*base.add(i)).next = base.add(i + 1);
            }
        }
    }
    let mut chain_struct = FEditPropertyChain {
        head: base,
        tail: unsafe { base.add(nodes.len() - 1) },
        size: nodes.len() as u32,
        _pad: [0; 64],
    };
    let mut event = FPropertyChangedChainEvent {
        property: prop,
        b_changes_topology: 0,
        change_type: crate::flags::property_change_type::CHANGE_TYPE_UNSPECIFIED,
        object_iterator_index: -1,
        array_indices_per_object: std::ptr::null(),
        property_chain: &mut chain_struct,
    };
    // SAFETY: live object/property on the game thread; the slot comes from
    // the object's own vtable (same dispatch the SDK performs).
    unsafe {
        let vtable = *(obj as *const *const usize);
        if vtable.is_null() {
            return false;
        }
        let func = *vtable.add(index);
        if func == 0 {
            return false;
        }
        let f: VirtualFn = std::mem::transmute::<usize, VirtualFn>(func);
        f(obj as *mut c_void, 0, &mut event);
    }
    true
}

/// Non-Windows/64-bit hosts cannot perform the game's `__thiscall`.
#[cfg(not(all(windows, target_arch = "x86")))]
fn dispatch_post_edit_change_chain_property(
    obj: *mut UObject,
    prop: *mut ZProperty,
    chain: &[*mut ZProperty],
    index: usize,
) -> bool {
    let _ = (obj, prop, chain, index);
    false
}
