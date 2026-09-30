//! One-stop imports for mod code: the object model, hooks, commands and
//! the error type, without the module walk.
//!
//! ```
//! use unrealsdk_rs::prelude::*;
//! ```

pub use crate::commands::{add_command, CommandHandle, CommandInvocation};
pub use crate::error::{Error, Result};
pub use crate::hooks::{
    add_hook, detour, inject_next_call, set_log_all_calls, DetailsView, HookHandle, HookType,
};
pub use crate::objects::refs::{find, find_in, ClassTag, ObjectRef};
pub use crate::objects::{
    construct_object, find_class, find_engine_class, find_function, find_object, is_kind_of,
    load_package, object_path_text, objects_of_class_name, CallOutcome, DelegateValue, Obj,
    PropValue, SoftObjectValue, StructValue, WeakObjectValue,
};
pub use crate::types::{UClass, UConst, UEnum, UFunction, UObject, UScriptStruct};
pub use crate::{call, get, is_console_ready, is_initialized, obj, set};
