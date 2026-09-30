//!  unrealsdk-rs: runtime bindings for the `unrealsdk.dll`s
//! `UNREALSDK_CAPI` export table.
//!
//! - [`sys`] — the raw `extern "C"` export table (`_unrealsdk_export__*`)
//! - [`types`] — `repr(C)` UE3 replicas plus the [`types::OffsetTable`]
//! - [`flags`] — some UE3 flag and enum sets
//! - [`mem`] — typed raw memory access (`Offset`-driven reads/writes).
//! - [`objects`] — the object and property API over [`sys`]: find/call/prop
//!   access with `u_free` RAII, [`Obj`] handles with dotted-path macros
//!   (`obj!`/`get!`/`set!`/`call!`), typed property values and marshalled
//!   function calls. Includes the [`objects::post_edit_change_property`]
//!   vtable dispatch.
//! - [`hooks`] — hook callbacks built as Rust pseudo-vtable structs.
//! - [`scan`] — signature-scanned, build-checked entry points into game
//!   code.
//! - [`logging`], [`commands`] — thin safe wrappers.
//! - [`error`] — the crate-wide [`Error`]/[`Result`]
//! - [`prelude`] — a simple import for most common features.
//!


pub mod commands;
pub mod error;
pub mod flags;
pub mod hooks;
pub mod logging;
pub mod mem;
pub mod objects;
pub mod prelude;
pub mod scan;
pub mod sys;
pub mod types;

mod state;

pub use objects::{Error, Obj, Result};
pub use state::{
    is_console_ready, is_initialized, load_offsets, sdk_version, sdk_version_string,
};
