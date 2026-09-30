//! Safe logging wrappers: redirect log messages to the SDK log

use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sys::sys;

/// `logging::Level`
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Invalid = 0,
    Misc = 1,
    DevWarning = 2,
    Info = 3,
    Warning = 4,
    Error = 5,
}

impl LogLevel {
    pub const DEFAULT_CONSOLE_LEVEL: LogLevel = LogLevel::Info;
    pub const MIN: LogLevel = LogLevel::Misc;
    pub const MAX: LogLevel = LogLevel::Error;
}

/// `logging::LogMessage` (`logging.h`)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LogMessage {
    pub unix_time_ms: u64,
    pub level: LogLevel,
    pub msg: *const c_char,
    pub msg_size: usize,
    pub location: *const c_char,
    pub location_size: usize,
    pub line: i32,
    pub thread_id: u32,
}

fn log_to_sdk_impl(level: LogLevel, msg: &str, location: &str, line: i32) -> bool {
    let Some(s) = sys() else { return false };
    let log = LogMessage {
        unix_time_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        level,
        msg: msg.as_ptr() as *const _,
        msg_size: msg.len(),
        location: location.as_ptr() as *const _,
        location_size: location.len(),
        line,
        thread_id: 0, // unrealsdk uses its own `GetCurrentThreadId`, we do not care
    };

    unsafe { (s.enqueue_log_msg)(&log) };
    true
}

/// Log one message into the SDK log for the given level (`enqueue_log_msg`).
pub fn log_to_sdk(level: LogLevel, msg: &str) -> bool {
    let loc = "unrealsdk-rs";
    let line = 0;

    log_to_sdk_impl(level, msg, loc, line)
}

/// `set_console_level` (`logging.cpp`)
pub fn set_console_level(level: LogLevel) -> bool {
    let Some(s) = sys() else { return false };
    // SAFETY: resolved export.
    unsafe { (s.set_console_level)(level as u8) }
}

/// Rust log callback: receives every SDK [`LogMessage`].
pub type LogCallback = Box<dyn Fn(&LogMessage) + Send + Sync>;

static SLOT: OnceLock<Mutex<Option<LogCallback>>> = OnceLock::new();
static REGISTERED: AtomicBool = AtomicBool::new(false);

fn slot() -> &'static Mutex<Option<LogCallback>> {
    SLOT.get_or_init(|| Mutex::new(None))
}

unsafe extern "C" fn trampoline(msg: *const LogMessage) {
    if msg.is_null() {
        return;
    }
    let _ = std::panic::catch_unwind(|| {
        let guard = slot().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cb) = guard.as_ref() {
            // SAFETY: the SDK guarantees `msg` lives at least for the callback.
            cb(unsafe { &*msg });
        }
    });
}

/// Run `cb` on every SDK log message (`add_callback`).
/// Replaces any previous Rust closure.
pub fn register_log_callback(cb: LogCallback) -> bool {
    {
        let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(cb);
    }
    if REGISTERED.swap(true, Ordering::SeqCst) {
        return sys().is_some();
    }
    let Some(s) = sys() else {
        REGISTERED.store(false, Ordering::SeqCst);
        return false;
    };

    unsafe { (s.add_callback)(trampoline as *const c_void) };
    true
}

/// Remove the Rust log tap (`remove_callback`).
/// `false` when nothing was registered.
pub fn unregister_log_callback() -> bool {
    {
        let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }
    if !REGISTERED.swap(false, Ordering::SeqCst) {
        return false;
    }
    let Some(s) = sys() else { return false };
    unsafe { (s.remove_callback)(trampoline as *const c_void) };
    true
}

// ---------------------------------------------------------------------------
// rust `log` -> unrealsdk `logging` bridge
// ---------------------------------------------------------------------------

struct SdkLogger;

fn sdk_level(level: log::Level) -> LogLevel {
    match level {
        log::Level::Error => LogLevel::Error,
        log::Level::Warn => LogLevel::Warning,
        log::Level::Info => LogLevel::Info,
        // Debug/Trace are development diagnostics: below the default
        // console level, always in the SDK log.
        log::Level::Debug | log::Level::Trace => LogLevel::Misc,
    }
}

impl log::Log for SdkLogger {
    /// Filtering is the facade's `set_max_level` plus the SDK's own console
    /// level; nothing extra to do here.
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let msg = match record.level() {
            // Messages carry their own prefixes; keep them as-is so console
            // output matches the caller's wording.
            log::Level::Error | log::Level::Warn | log::Level::Info => {
                format!("{}", record.args())
            }
            log::Level::Debug | log::Level::Trace => {
                format!("{} [{}]", record.args(), record.target())
            }
        };
        let (loc, line) = (
            record.file().unwrap_or("unrealsdk-rs"),
            record.line().unwrap_or(0),
        );
        log_to_sdk_impl(sdk_level(record.level()), &msg, loc, line.cast_signed());
    }

    fn flush(&self) {}
}

/// Install the rust [`log`] → unrealsdk-logging bridge
///
/// Returns `true` when this bridge is the process-wide logger.
pub fn install_log_bridge() -> bool {
    static INIT: OnceLock<bool> = OnceLock::new();
    *INIT.get_or_init(|| {
        static LOGGER: SdkLogger = SdkLogger;
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Debug);
            true
        } else {
            log_to_sdk(
                LogLevel::DevWarning,
                "unrealsdk-rs: log bridge not installed (another global logger is already \
                 registered); log:: lines go through that one",
            );
            false
        }
    })
}
