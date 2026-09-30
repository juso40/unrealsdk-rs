//! Hook callbacks built as Rust pseudo-vtable structs.
//!
//! The SDK takes hook callbacks as `DLLSafeCallback&&` — a pointer to a
//! `{ Inner* }` holder whose `Inner` starts with a hand-built pseudo-vtable
//! `{destroy, call}` of plain C function pointers (`utils.h`). The
//! SDK move-constructs from our holder (`hook_manager.cpp`) and
//! later destroys the `Inner` through our `destroy` entry, so the vtable and
//! the layout beyond it (`user` cell) are ours to define — no C++ runtime on
//! our side.
//!
//! Containment: both shims are wrapped in `catch_unwind`. A panicking hook
//! logs and returns `false` (block nothing); `destroy` leaks rather than
//! unwinds. Nothing unwinds across into the SDK.
//!
//! Threading: hooks fire on the **game thread**. Never touch Python from a
//! hook.
//!
//! Ownership handshake (the load-bearing detail, see `add_hook`):
//! - success: the SDK moved our `Inner` out (`holder.inner` is nulled by
//!   `std::exchange`, `utils.h`) and owns it; removal destroys it
//!   through our `destroy` shim exactly once.
//! - failure (`false` = duplicate): the SDK moved nothing — `holder.inner`
//!   is still ours and we free it. The check is the null state itself, not
//!   blind trust.

use std::ffi::c_void;

use crate::error::{Error, Result};
use crate::objects::{
    encode_wide, find_field, get_property, object_name_text, set_property, struct_fields,
    PropValue, StructValue,
};
use crate::sys::sys;
use crate::types::{BoundFunction, Details, UObject, UStruct, ZProperty};

/// `hook_manager::Type` (`hook_manager.h`):
/// `enum class Type : uint8_t { PRE, POST, POST_UNCONDITIONAL }`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookType {
    Pre = 0,
    Post = 1,
    PostUnconditional = 2,
}

/// Borrowed view over hook [`Details`], valid for the duration of the
/// callback only. Typed access mirrors the SDK's `WrappedStruct::get/set`
/// and `PropertyProxy::get` (`wrapped_struct.h`, `property_proxy.h`):
/// arguments by name, the return value by read. Writing the return value
/// (`PropertyProxy::set`) is deliberately absent — it allocates value
/// storage behind a C++ `UnrealPointer` control block
/// (`unreal_pointer.h`) that Rust must not fabricate; block-and-return
/// needs a `property_proxy_set` export upstream.
pub struct DetailsView {
    raw: *mut Details,
}

impl DetailsView {
    /// `Details::obj` (`hook_manager.h`).
    pub fn obj(&self) -> Option<*mut UObject> {
        // SAFETY: borrowed for the callback duration.
        let obj = unsafe { (*self.raw).obj };
        if obj.is_null() {
            None
        } else {
            Some(obj)
        }
    }

    /// `Details::args` struct type (`WrappedStruct::type`,
    /// `wrapped_struct.h`).
    pub fn args_type(&self) -> Option<*const UStruct> {
        // SAFETY: plain pointer read, no C++ interaction.
        let args = unsafe { (*self.raw).args };
        if args.is_null() {
            return None;
        }
        let ty = unsafe { (*args).ty };
        if ty.is_null() {
            None
        } else {
            Some(ty)
        }
    }

    /// `Details::args` base address (`WrappedStruct::base`,
    /// `wrapped_struct.h`). Borrowed — never retain.
    pub fn args_base(&self) -> Option<*mut c_void> {
        // SAFETY: plain pointer read, no C++ interaction.
        let args = unsafe { (*self.raw).args };
        if args.is_null() {
            return None;
        }
        let ptr = unsafe { (*args).base.ptr };
        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    }

    /// `Details::ret` property (`PropertyProxy::prop`,
    /// `property_proxy.h`). During pre-hooks this is the unset value
    /// being overwritten; during post-hooks, the return value
    /// (`hook_manager.h`).
    pub fn ret_prop(&self) -> Option<*mut ZProperty> {
        // SAFETY: plain pointer read, no C++ interaction.
        let prop = unsafe { (*self.raw).ret.prop };
        if prop.is_null() {
            None
        } else {
            Some(prop)
        }
    }

    /// `Details::ret` value address, if any.
    pub fn ret_ptr(&self) -> Option<*mut c_void> {
        // SAFETY: plain pointer read, no C++ interaction.
        let ptr = unsafe { (*self.raw).ret.ptr.ptr };
        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    }

    /// `Details::func` (`hook_manager.h`): the called function bound to
    /// the same object — a plain copy of two pointers, safe to retain.
    pub fn func(&self) -> Option<BoundFunction> {
        // SAFETY: as above.
        let f = unsafe { (*self.raw).func };
        if f.func.is_null() || f.object.is_null() {
            return None;
        }
        Some(f)
    }

    /// Re-call the hooked function with a pre-filled params block
    /// (`bound_function_call_with_params`, `bound_function.cpp`).
    /// Callers avoiding recursion should call [`inject_next_call`] first
    /// (`hook_manager.h`).
    pub fn call_with_params(&mut self, params: *mut c_void) -> bool {
        let Some(f) = self.func() else {
            return false;
        };
        crate::objects::call_bound_function(&f, params)
    }

    /// Re-call the hooked function with the (possibly modified) hook args
    /// (`Details::args` is itself a valid params block,
    /// `hook_manager.h`). Call [`inject_next_call`] first to avoid
    /// re-entering this hook (`hook_manager.h`).
    pub fn recall(&mut self) -> bool {
        let Some(base) = self.args_base() else {
            return false;
        };
        self.call_with_params(base)
    }

    /// Read one named argument (the SDK's `WrappedStruct::get<T>`,
    /// `wrapped_struct.h`). `None` when the arg is absent or its property
    /// class is unreadable (see the coverage matrix in [`crate::objects`]).
    pub fn arg(&self, name: &str) -> Option<PropValue> {
        let prop = self.arg_prop(name)?;
        get_property(self.args_base()?, prop)
    }

    /// Write one named argument (the SDK's `WrappedStruct::set<T>`,
    /// `wrapped_struct.h`). Modifying args does not touch the real call
    /// arguments (`hook_manager.h`); it only affects [`Self::recall`].
    /// `false` on a missing arg or a value/property mismatch.
    pub fn set_arg(&self, name: &str, value: &PropValue) -> bool {
        let (Some(prop), Some(base)) = (self.arg_prop(name), self.args_base()) else {
            return false;
        };
        set_property(base, prop, value)
    }

    /// All readable arguments as a [`StructValue`] (name = the args
    /// function's name). Args whose property class is unreadable are
    /// omitted.
    pub fn args(&self) -> Option<StructValue> {
        let (ty, base) = (self.args_type()?, self.args_base()?);
        let mut fields = Vec::new();
        for f in struct_fields(ty as *const c_void) {
            if let Some(value) = get_property(base, f) {
                fields.push((object_name_text(f as *mut UObject), value));
            }
        }
        Some(StructValue {
            name: object_name_text(ty as *mut UObject),
            fields,
        })
    }

    /// The `ZProperty` of one named argument (`WrappedStruct` field
    /// lookup by exact name).
    pub fn arg_prop(&self, name: &str) -> Option<*mut ZProperty> {
        find_field(self.args_type()? as *const c_void, name)
    }

    /// Read the hooked call's return value (the SDK's
    /// `PropertyProxy::get`, `property_proxy.h`). Unset during pre-hooks
    /// and after blocked calls (`hook_manager.h`) — `None` then.
    ///
    /// The proxy resolves its value at `base + Offset_Internal`, exactly
    /// like the C++ `get_property` call in `PropertyProxy::get`.
    pub fn ret_value(&self) -> Option<PropValue> {
        get_property(self.ret_ptr()?, self.ret_prop()?)
    }
}

// ---------------------------------------------------------------------------
// Pseudo-vtable machinery (mirrors utils.h)
// ---------------------------------------------------------------------------

/// Our `Inner`: the pseudo-vtable pointer FIRST (`utils.h`), client data
/// after (`utils.h`).
#[repr(C)]
struct RawCallback {
    vftable: *const RawVTable,
    user: *mut CallbackCell,
}

const _: () = assert!(size_of::<RawCallback>() == 2 * size_of::<*mut c_void>());

const _: () = assert!(size_of::<RawVTable>() == 2 * size_of::<*mut c_void>());

/// The hand-built pseudo-vtable (`utils.h`): plain C fn pointers
/// (never a real C++ vtable — MSVC/clang layouts differ, `utils.h`).
#[repr(C)]
struct RawVTable {
    destroy: unsafe extern "C" fn(this: *mut RawCallback),
    call: unsafe extern "C" fn(this: *mut RawCallback, details: *mut Details) -> bool,
}

struct CallbackCell {
    cb: Box<dyn Fn(&mut DetailsView) -> bool + Send>,
}

/// The `DLLSafeCallback` temporary (`utils.h`): `{ Inner* inner }`,
/// passed **by address** as the `DLLSafeCallback&&` parameter
/// (`hook_manager.cpp`).
#[repr(C)]
struct CallbackHolder {
    inner: *mut RawCallback,
}

const _: () = assert!(size_of::<CallbackHolder>() == size_of::<*mut c_void>());

unsafe extern "C" fn destroy_shim(this: *mut RawCallback) {
    if this.is_null() {
        return;
    }
    // A panicking destructor must still not unwind: leak instead.
    // (`AssertUnwindSafe`: never resumed, only dropped.)
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `this` was built by `Box::new` below and is destroyed
        // through this shim exactly once (SDK `~DLLSafeCallback`,
        // `utils.h`); reclaim both boxes.
        unsafe {
            let raw = Box::from_raw(this);
            let _ = Box::from_raw(raw.user);
        }
    }));
}

unsafe extern "C" fn call_shim(this: *mut RawCallback, details: *mut Details) -> bool {
    if this.is_null() || details.is_null() {
        return false;
    }
    // Panic → log + block nothing (`false`), matching the SDK's own
    // exception guard around callbacks (`hook_manager.cpp`).
    // `AssertUnwindSafe`: we never resume, only drop the payload, so a
    // closure with interior mutability cannot observe inconsistent state.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: live for the call; the SDK holds the `Inner` while calling
        // through its vtable (`utils.h`).
        let cell = unsafe { &*((*this).user as *const CallbackCell) };
        let mut view = DetailsView { raw: details };
        (cell.cb)(&mut view)
    }));
    match result {
        Ok(block) => block,
        Err(_) => {
            log::error!("unrealsdk-rs: hook callback panicked; not blocking");
            false
        }
    }
}

static VTABLE: RawVTable = RawVTable {
    destroy: destroy_shim,
    call: call_shim,
};

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// A registered hook. Dropping it removes the hook (`remove_hook`); removal
/// is best-effort and `None`-safe (SDK teardown just skips the call).
#[derive(Debug)]
pub struct HookHandle {
    func: Vec<u16>,
    ty: HookType,
    id: Vec<u16>,
    active: bool,
}

impl HookHandle {
    /// Remove the hook now; consumes the handle (Drop becomes a no-op).
    /// Returns the SDK's result (`false` = already gone / off-game).
    pub fn remove(mut self) -> bool {
        self.remove_inner()
    }

    /// Whether this handle still owns a live registration.
    pub fn is_active(&self) -> bool {
        self.active
    }

    fn remove_inner(&mut self) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        let Some(s) = sys() else { return false };
        // SAFETY: resolved export; buffers alive for the call.
        unsafe {
            (s.remove_hook)(
                self.func.as_ptr(),
                self.func.len(),
                self.ty as u8,
                self.id.as_ptr(),
                self.id.len(),
            )
        }
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        let _ = self.remove_inner();
    }
}

/// `add_hook` (`hook_manager.cpp`). The callback returns `true` to
/// **block** the hooked function (pre-hooks only; ignored for post,
/// `hook_manager.h`). Errors: [`Error::NotInGame`], [`Error::EmptyName`],
/// [`Error::Duplicate`], or a bare [`Error::Failed`].
pub fn add_hook(
    func: &str,
    ty: HookType,
    id: &str,
    cb: Box<dyn Fn(&mut DetailsView) -> bool + Send>,
) -> Result<HookHandle> {
    if func.is_empty() || id.is_empty() {
        return Err(Error::EmptyName("hook func and id".to_owned()));
    }
    let Some(s) = sys() else {
        return Err(Error::NotInGame);
    };
    let func_wide = encode_wide(func);
    let id_wide = encode_wide(id);

    let cell = Box::new(CallbackCell { cb });
    let raw = Box::into_raw(Box::new(RawCallback {
        vftable: &VTABLE,
        // SAFETY: reclaimed by `destroy_shim` exactly once.
        user: Box::into_raw(cell),
    }));
    let holder = CallbackHolder { inner: raw };
    // SAFETY: resolved export; `holder` is our stack temporary whose address
    // serves as the `DLLSafeCallback&&` (the SDK nulls `inner` through the
    // pointer on success); string buffers alive for the call.
    let added = unsafe {
        (s.add_hook)(
            func_wide.as_ptr(),
            func_wide.len(),
            ty as u8,
            id_wide.as_ptr(),
            id_wide.len(),
            &holder as *const CallbackHolder as *const c_void,
        )
    };
    if holder.inner.is_null() {
        // The SDK moved our `Inner` out and owns it now.
        if added {
            return Ok(HookHandle {
                func: func_wide,
                ty,
                id: id_wide,
                active: true,
            });
        }
        // Paranoia: nulled but reported failure — ownership already crossed.
        return Err(Error::Failed("add_hook failed".to_owned()));
    }
    // Not taken: free our own boxes (rebuild in reverse order of creation).
    // SAFETY: `holder.inner == raw`, untouched by the callee.
    unsafe {
        let raw = Box::from_raw(holder.inner);
        let _ = Box::from_raw(raw.user);
    }
    Err(Error::Duplicate(format!("hook '{func}' ({id})")))
}

/// `has_hook` (`hook_manager.cpp`).
pub fn has_hook(func: &str, ty: HookType, id: &str) -> bool {
    let Some(s) = sys() else { return false };
    if func.is_empty() || id.is_empty() {
        return false;
    }
    let func_wide = encode_wide(func);
    let id_wide = encode_wide(id);
    // SAFETY: resolved export.
    unsafe {
        (s.has_hook)(
            func_wide.as_ptr(),
            func_wide.len(),
            ty as u8,
            id_wide.as_ptr(),
            id_wide.len(),
        )
    }
}

/// `inject_next_call` (`hook_manager.cpp`): the next Unreal call on
/// this thread ignores hooks (recursion guard for re-calling from a hook).
/// Returns `false` off-game (nothing injected).
pub fn inject_next_call() -> bool {
    let Some(s) = sys() else { return false };
    // SAFETY: resolved export.
    unsafe { (s.inject_next_call)() };
    true
}

/// `log_all_calls` (`hook_manager.cpp`): TSV-debug every Unreal call
/// (short bursts only). Returns `false` off-game.
pub fn set_log_all_calls(should_log: bool) -> bool {
    let Some(s) = sys() else { return false };
    // SAFETY: resolved export.
    unsafe { (s.log_all_calls)(should_log) };
    true
}

/// `detour` (`memory.cpp`): install `detour_fn` as a minhook trampoline at
/// `addr` (typically an address from [`crate::scan::GameFn`]) and return
/// the original function as `F`. `None` on a null/zero target, off-game,
/// or when the loaded SDK lacks the (optional) `detour` export. The
/// trampoline stays installed for the life of the process (upstream ships
/// no removal export).
///
/// # Safety
/// `detour_fn` must be a valid function pointer with the target's exact
/// signature and calling convention; `F` likewise for the original. This
/// is raw game-code hooking — no containment, no type checking.
pub unsafe fn detour<F: Copy>(addr: usize, detour_fn: *const c_void, name: &str) -> Option<F> {
    if addr == 0 || detour_fn.is_null() {
        return None;
    }
    let s = sys()?;
    let detour = s.detour?;
    let mut original: *mut c_void = std::ptr::null_mut();
    // SAFETY: resolved export; `name` alive for the call; `original` is a
    // valid out-param.
    let ok = unsafe {
        detour(
            addr,
            detour_fn as *mut c_void,
            &mut original,
            name.as_ptr() as *const std::ffi::c_char,
            name.len(),
        )
    };
    if !ok || original.is_null() {
        return None;
    }
    // SAFETY: caller contract (F matches the original's signature); fn
    // pointers and `usize` share a size.
    Some(unsafe { std::mem::transmute_copy(&original) })
}
