//! Raw runtime bindings into `unrealsdk.dll`.
//!
//! The game process already has the SDK loaded, so the
//! module is resolved with `GetModuleHandleW` and every stable `extern "C"`
//! export is looked up with `GetProcAddress`.
//! A missing module degrades the table to `None`
//!

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

#[cfg(windows)]
use windows::core::{PCSTR, PCWSTR};
#[cfg(windows)]
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

use crate::logging::LogMessage;
use crate::types::{
    BoundFunction, FField, FFrame, FName, FText, GObjects, OffsetList, UClass, UFunction, UObject,
};

// ---------------------------------------------------------------------------
// fn-pointer types (one per exported unrealsdk function)
// ---------------------------------------------------------------------------

// `UNREALSDK_CAPI([[nodiscard]] bool, is_initialized)` — unrealsdk_main.cpp
pub type IsInitialized = unsafe extern "C" fn() -> bool;
// `UNREALSDK_CAPI([[nodiscard]] bool, is_console_ready)` — unrealsdk_main.cpp
pub type IsConsoleReady = unsafe extern "C" fn() -> bool;
// `UNREALSDK_CAPI([[nodiscard]] const GObjects*, gobjects)` — unrealsdk_main.cpp
pub type GObjectsFn = unsafe extern "C" fn() -> *const GObjects;
// `UNREALSDK_CAPI(void*, u_malloc, size_t len)` — unrealsdk_main.cpp
pub type UMalloc = unsafe extern "C" fn(len: usize) -> *mut c_void;
// `UNREALSDK_CAPI(void*, u_realloc, void* original, size_t len)` — unrealsdk_main.cpp
pub type URealloc = unsafe extern "C" fn(original: *mut c_void, len: usize) -> *mut c_void;
// `UNREALSDK_CAPI(void, u_free, void* data)` — unrealsdk_main.cpp
pub type UFree = unsafe extern "C" fn(data: *mut c_void);
// `UNREALSDK_CAPI(UObject*, construct_object, UClass*, UObject*, const FName*,
// uint64_t, UObject*)` — unrealsdk_main.cpp
pub type ConstructObject = unsafe extern "C" fn(
    cls: *mut UClass,
    outer: *mut UObject,
    name: *const FName,
    flags: u64,
    template_obj: *mut UObject,
) -> *mut UObject;
// `UNREALSDK_CAPI(UObject*, find_object, UClass*, const wchar_t*, size_t)` —
// unrealsdk_main.cpp
pub type FindObject =
    unsafe extern "C" fn(cls: *mut UClass, name: *const u16, name_size: usize) -> *mut UObject;
// `UNREALSDK_CAPI(UObject*, load_package, const wchar_t*, size_t, uint32_t)` —
// unrealsdk_main.cpp
pub type LoadPackage =
    unsafe extern "C" fn(name: *const u16, size: usize, flags: u32) -> *mut UObject;
// `UNREALSDK_CAPI(void, fname_init, FName*, const wchar_t*, uint32_t)` —
// unrealsdk_main.cpp
pub type FNameInit = unsafe extern "C" fn(name: *mut FName, s: *const u16, number: u32);
// `UNREALSDK_CAPI(void, fname_get_str, FName, const void**, size_t*, bool*)` —
// unrealsdk_main.cpp
pub type FNameGetStr =
    unsafe extern "C" fn(name: FName, s: *mut *const c_void, size: *mut usize, is_wide: *mut bool);
// `UNREALSDK_CAPI(void, fframe_step, FFrame*, UObject*, void*)` — unrealsdk_main.cpp
pub type FFrameStep =
    unsafe extern "C" fn(frame: *mut FFrame, obj: *mut UObject, param: *mut c_void);
// `UNREALSDK_CAPI(void, process_event, UObject*, UFunction*, void*)` —
// unrealsdk_main.cpp
pub type ProcessEvent =
    unsafe extern "C" fn(object: *mut UObject, function: *mut UFunction, params: *mut c_void);
// `UNREALSDK_CAPI(void, uconsole_output_text, const wchar_t*, size_t)` —
// unrealsdk_main.cpp
pub type UConsoleOutputText = unsafe extern "C" fn(s: *const u16, size: usize);
// `UNREALSDK_CAPI(wchar_t*, uobject_path_name, const UObject*, size_t&)` —
// unrealsdk_main.cpp
pub type UObjectPathName = unsafe extern "C" fn(obj: *const UObject, size: *mut usize) -> *mut u16;
// `UNREALSDK_CAPI(wchar_t*, ffield_path_name, const FField*, size_t&)` —
// unrealsdk_main.cpp
pub type FFieldPathName = unsafe extern "C" fn(obj: *const FField, size: *mut usize) -> *mut u16;
// `UNREALSDK_CAPI(void, ftext_as_culture_invariant, FText*, const wchar_t*,
// size_t)` — unrealsdk_main.cpp
pub type FTextAsCultureInvariant =
    unsafe extern "C" fn(text: *mut FText, s: *const u16, size: usize);
// `UNREALSDK_CAPI(void, fsoftobjectptr_assign, FSoftObjectPtr*, const UObject*)` —
// unrealsdk_main.cpp
pub type FSoftObjectPtrAssign = unsafe extern "C" fn(ptr: *mut c_void, obj: *const UObject);
// `UNREALSDK_CAPI(void, flazyobjectptr_assign, FLazyObjectPtr*, const UObject*)` —
// unrealsdk_main.cpp
pub type FLazyObjectPtrAssign = unsafe extern "C" fn(ptr: *mut c_void, obj: *const UObject);
// `UNREALSDK_CAPI(const offsets::OffsetList*, get_offsets)` — unrealsdk_main.cpp
pub type GetOffsets = unsafe extern "C" fn() -> *const OffsetList;
// `UNREALSDK_CAPI(UClass*, find_class_fname, const FName*)` — find_class.cpp
pub type FindClassFName = unsafe extern "C" fn(name: *const FName) -> *mut UClass;
// `UNREALSDK_CAPI(UClass*, find_class_cstr, const wchar_t*, size_t)` —
// find_class.cpp
pub type FindClassCStr = unsafe extern "C" fn(name: *const u16, name_size: usize) -> *mut UClass;
// `UNREALSDK_CAPI(void, bound_function_call_with_params, const BoundFunction*,
// void*)` — bound_function.cpp
pub type BoundFunctionCallWithParams =
    unsafe extern "C" fn(this: *const BoundFunction, params: *mut c_void);
// `UNREALSDK_CAPI(void, log_all_calls, bool)` — hook_manager.cpp
pub type LogAllCalls = unsafe extern "C" fn(should_log: bool);
// `UNREALSDK_CAPI(void, inject_next_call)` — hook_manager.cpp
pub type InjectNextCall = unsafe extern "C" fn();
// `UNREALSDK_CAPI(bool, add_hook, const wchar_t*, size_t, Type,
// const wchar_t*, size_t, DLLSafeCallback&&)` — hook_manager.cpp
// (`Type` is `enum class Type : uint8_t`, hook_manager.h)
pub type AddHook = unsafe extern "C" fn(
    func: *const u16,
    func_size: usize,
    ty: u8,
    identifier: *const u16,
    identifier_size: usize,
    callback: *const c_void,
) -> bool;
// `UNREALSDK_CAPI(bool, has_hook, ...)` — hook_manager.cpp
pub type HasHook = unsafe extern "C" fn(
    func: *const u16,
    func_size: usize,
    ty: u8,
    identifier: *const u16,
    identifier_size: usize,
) -> bool;
// `UNREALSDK_CAPI(bool, remove_hook, ...)` — hook_manager.cpp
pub type RemoveHook = unsafe extern "C" fn(
    func: *const u16,
    func_size: usize,
    ty: u8,
    identifier: *const u16,
    identifier_size: usize,
) -> bool;
// `UNREALSDK_CAPI(bool, add_command, const wchar_t*, size_t, DLLSafeCallback&&)` —
// commands.cpp
pub type AddCommand =
    unsafe extern "C" fn(cmd: *const u16, size: usize, callback: *const c_void) -> bool;
// `UNREALSDK_CAPI(bool, has_command, const wchar_t*, size_t)` — commands.cpp
pub type HasCommand = unsafe extern "C" fn(cmd: *const u16, size: usize) -> bool;
// `UNREALSDK_CAPI(bool, remove_command, const wchar_t*, size_t)` — commands.cpp
pub type RemoveCommand = unsafe extern "C" fn(cmd: *const u16, size: usize) -> bool;
// `UNREALSDK_CAPI(void, enqueue_log_msg, const LogMessage*)` — logging.cpp
pub type EnqueueLogMsg = unsafe extern "C" fn(log: *const LogMessage);
// `UNREALSDK_CAPI(bool, set_console_level, Level)` — logging.cpp
// (`Level` is `enum class Level : uint8_t`, logging.h)
pub type SetConsoleLevel = unsafe extern "C" fn(level: u8) -> bool;
// `UNREALSDK_CAPI(void, add_callback, log_callback)` — logging.cpp
// (`log_callback = void(*)(const LogMessage*)`, logging.h)
pub type AddLogCallback = unsafe extern "C" fn(callback: *const c_void);
// `UNREALSDK_CAPI(void, remove_callback, log_callback)` — logging.cpp
pub type RemoveLogCallback = unsafe extern "C" fn(callback: *const c_void);
// `UNREALSDK_CAPI(bool, config_get_bool, const char*, size_t, bool*)` —
// config.cpp (`path` is a byte/TOML path, not wide)
pub type ConfigGetBool =
    unsafe extern "C" fn(path: *const c_char, path_size: usize, value: *mut bool) -> bool;
// `UNREALSDK_CAPI(bool, config_get_int, const char*, size_t, int64_t*)` —
// config.cpp
pub type ConfigGetInt =
    unsafe extern "C" fn(path: *const c_char, path_size: usize, value: *mut i64) -> bool;
// `UNREALSDK_CAPI(bool, config_get_str, const char*, size_t, const char**,
// size_t*)` — config.cpp. The returned pointer borrows the SDK's merged
// config storage (a `std::string_view` into it): copy the bytes out
// immediately, never free or retain the pointer.
pub type ConfigGetStr = unsafe extern "C" fn(
    path: *const c_char,
    path_size: usize,
    value: *mut *const c_char,
    value_size: *mut usize,
) -> bool;
// `UNREALSDK_CAPI(uint32_t, get_version)` — version.cpp
pub type GetVersion = unsafe extern "C" fn() -> u32;
// `UNREALSDK_CAPI(const char*, get_version_str)` — version.cpp
pub type GetVersionStr = unsafe extern "C" fn() -> *const c_char;
// `UNREALSDK_CAPI(bool, detour, uintptr_t, void*, void**, const char*, size_t)` —
// memory.cpp
pub type Detour = unsafe extern "C" fn(
    addr: usize,
    detour_func: *mut c_void,
    original_func: *mut *mut c_void,
    name: *const c_char,
    name_size: usize,
) -> bool;

/// The resolved `unrealsdk.dll` export table. All fields are `unsafe
/// extern "C"` fn pointers; call only when [`sys`] returned `Some`.
///
/// The `Option` fields are optional exports (older/newer SDK builds may
/// lack them); everything else is required and its absence degrades the
/// whole table to `None` (see [`bind_error`]).
pub struct Sys {
    pub is_initialized: IsInitialized,
    pub is_console_ready: IsConsoleReady,
    pub gobjects: GObjectsFn,
    pub u_malloc: UMalloc,
    pub u_realloc: URealloc,
    pub u_free: UFree,
    pub construct_object: ConstructObject,
    pub find_object: FindObject,
    pub load_package: LoadPackage,
    pub fname_init: FNameInit,
    pub fname_get_str: FNameGetStr,
    pub fframe_step: FFrameStep,
    pub process_event: ProcessEvent,
    pub uconsole_output_text: UConsoleOutputText,
    pub uobject_path_name: UObjectPathName,
    pub ftext_as_culture_invariant: FTextAsCultureInvariant,
    pub get_offsets: GetOffsets,
    pub find_class_fname: FindClassFName,
    pub find_class_cstr: FindClassCStr,
    pub bound_function_call_with_params: BoundFunctionCallWithParams,
    pub log_all_calls: LogAllCalls,
    pub inject_next_call: InjectNextCall,
    pub add_hook: AddHook,
    pub has_hook: HasHook,
    pub remove_hook: RemoveHook,
    pub add_command: AddCommand,
    pub has_command: HasCommand,
    pub remove_command: RemoveCommand,
    pub enqueue_log_msg: EnqueueLogMsg,
    pub set_console_level: SetConsoleLevel,
    pub add_callback: AddLogCallback,
    pub remove_callback: RemoveLogCallback,
    /// `ffield_path_name` (optional, no safe wrapper yet).
    pub ffield_path_name: Option<FFieldPathName>,
    /// `fsoftobjectptr_assign` (optional, BL3+ upstream support only — the
    /// WILLOW hook throws `version_error`, so never call this there).
    pub fsoftobjectptr_assign: Option<FSoftObjectPtrAssign>,
    /// `flazyobjectptr_assign` (optional, as [`Sys::fsoftobjectptr_assign`]).
    pub flazyobjectptr_assign: Option<FLazyObjectPtrAssign>,
    /// `config_get_bool` (optional).
    pub config_get_bool: Option<ConfigGetBool>,
    /// `config_get_int` (optional).
    pub config_get_int: Option<ConfigGetInt>,
    /// `config_get_str` (optional). Borrowed return, see [`ConfigGetStr`].
    pub config_get_str: Option<ConfigGetStr>,
    /// `get_version` (optional on very old builds).
    pub get_version: Option<GetVersion>,
    /// `get_version_str` (optional on very old builds).
    pub get_version_str: Option<GetVersionStr>,
    /// `detour` (optional).
    pub detour: Option<Detour>,
}

static SYS: OnceLock<Option<Sys>> = OnceLock::new();
static BIND_ERROR: OnceLock<Option<&'static str>> = OnceLock::new();

/// The resolved `unrealsdk.dll` table, or `None` off-game (missing module,
/// missing required export, or an incompatible SDK major version). Never
/// panics; safe to call from any thread.
pub fn sys() -> Option<&'static Sys> {
    SYS.get_or_init(load).as_ref()
}

/// Why [`sys`] is `None`: the first required export that failed to resolve,
/// or `"unrealsdk.dll"` when the module itself is absent (off-game).
/// `None` when the table is bound. Answers the "why is everything
/// `NotInGame`?" question that an all-or-nothing bind would otherwise hide.
pub fn bind_error() -> Option<&'static str> {
    match SYS.get() {
        None => None, // not attempted yet
        Some(None) => BIND_ERROR
            .get()
            .copied()
            .flatten()
            .or(Some("unrealsdk.dll")),
        Some(Some(_)) => None,
    }
}

#[cfg(windows)]
/// Bind one resolved export to its typed `Sys` field — `T` is the field's
/// function-pointer type (each alias above carries its signature).
///
/// # Safety
/// `ptr` must be a live export with the exact signature `T` describes.
unsafe fn as_fn<T: Copy>(ptr: *const c_void) -> T {
    assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<usize>());
    // SAFETY: caller contract (signature verified per export, see `load`);
    // function pointers and `usize` share a size (asserted above).
    unsafe { std::mem::transmute_copy::<*const c_void, T>(&ptr) }
}

#[cfg(windows)]
fn load() -> Option<Sys> {
    // SAFETY: GetModuleHandleW only looks up the handle.
    // Every export is mapped to the signature of its type alias above.
    // A missing required export results in `None`, recorded in `BIND_ERROR`.
    unsafe {
        let name: Vec<u16> = "unrealsdk.dll\0".encode_utf16().collect();
        let module = match GetModuleHandleW(PCWSTR(name.as_ptr())) {
            Ok(m) => m,
            Err(_) => {
                let _ = BIND_ERROR.set(Some("unrealsdk.dll"));
                return None;
            }
        };

        // Look up one export; `None` if absent.
        let export = |name: &str| -> Option<*const c_void> {
            let name = std::ffi::CString::new(name).ok()?;
            let symbol = GetProcAddress(module, PCSTR(name.as_ptr() as *const u8));
            symbol.map(|f| f as *const c_void)
        };
        // Required export: record the first missing name and bail.
        macro_rules! bind {
            ($name:ident) => {{
                match export(concat!("_unrealsdk_export__", stringify!($name))) {
                    Some(ptr) => as_fn(ptr),
                    None => {
                        let _ = BIND_ERROR.set(Some(stringify!($name)));
                        log::error!(
                            "unrealsdk-rs: missing required export `{}` — \
                             is unrealsdk.dll an incompatible build?",
                            stringify!($name)
                        );
                        return None;
                    }
                }
            }};
        }
        // Optional export: `None` when absent, the feature degrades alone.
        macro_rules! bind_opt {
            ($name:ident) => {{
                match export(concat!("_unrealsdk_export__", stringify!($name))) {
                    Some(ptr) => Some(as_fn(ptr)),
                    None => {
                        log::debug!(
                            "unrealsdk-rs: optional export `{}` absent",
                            stringify!($name)
                        );
                        None
                    }
                }
            }};
        }

        let sys = Sys {
            is_initialized: bind!(is_initialized),
            is_console_ready: bind!(is_console_ready),
            gobjects: bind!(gobjects),
            u_malloc: bind!(u_malloc),
            u_realloc: bind!(u_realloc),
            u_free: bind!(u_free),
            construct_object: bind!(construct_object),
            find_object: bind!(find_object),
            load_package: bind!(load_package),
            fname_init: bind!(fname_init),
            fname_get_str: bind!(fname_get_str),
            fframe_step: bind!(fframe_step),
            process_event: bind!(process_event),
            uconsole_output_text: bind!(uconsole_output_text),
            uobject_path_name: bind!(uobject_path_name),
            ftext_as_culture_invariant: bind!(ftext_as_culture_invariant),
            get_offsets: bind!(get_offsets),
            find_class_fname: bind!(find_class_fname),
            find_class_cstr: bind!(find_class_cstr),
            bound_function_call_with_params: bind!(bound_function_call_with_params),
            log_all_calls: bind!(log_all_calls),
            inject_next_call: bind!(inject_next_call),
            add_hook: bind!(add_hook),
            has_hook: bind!(has_hook),
            remove_hook: bind!(remove_hook),
            add_command: bind!(add_command),
            has_command: bind!(has_command),
            remove_command: bind!(remove_command),
            enqueue_log_msg: bind!(enqueue_log_msg),
            set_console_level: bind!(set_console_level),
            add_callback: bind!(add_callback),
            remove_callback: bind!(remove_callback),
            ffield_path_name: bind_opt!(ffield_path_name),
            fsoftobjectptr_assign: bind_opt!(fsoftobjectptr_assign),
            flazyobjectptr_assign: bind_opt!(flazyobjectptr_assign),
            config_get_bool: bind_opt!(config_get_bool),
            config_get_int: bind_opt!(config_get_int),
            config_get_str: bind_opt!(config_get_str),
            get_version: bind_opt!(get_version),
            get_version_str: bind_opt!(get_version_str),
            detour: bind_opt!(detour),
        };

        Some(sys)
    }
}

/// Maybe we support Linux in the future?
/// But for now only here that my IDE doesn't freak out.
#[cfg(not(windows))]
fn load() -> Option<Sys> {
    let _ = BIND_ERROR.set(Some("unrealsdk.dll"));
    None
}
