//! The crate's single error type.
//!
//! Every fallible entry point returns [`Result<T>`]: the object layer's
//! typed conversions, hook and command registration, and (eventually) the
//! rest. Nothing in the crate panics on bad input; it returns [`Error`].

use std::fmt;

/// Errors from [`crate::objects::Obj`] operations and SDK registrations.
/// Nothing here panics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// No resolved SDK table (off-game, or the SDK went away).
    NotInGame,
    /// A null object on the path (missing child, unbound reference).
    NullObject,
    /// No property/function/object under this name.
    NotFound(String),
    /// The value does not fit the property, or the property does not
    /// convert to the requested type.
    TypeMismatch { expected: String, got: String },
    /// A property class this crate cannot handle (see the coverage
    /// matrix in [`crate::objects`]).
    Unsupported(String),
    /// Wrong number of call arguments (after trailing optional params).
    ArgCount {
        function: String,
        expected: usize,
        got: usize,
    },
    /// The game allocator returned null.
    AllocFailed,
    /// An identical hook or command is already registered.
    Duplicate(String),
    /// An empty hook or command name (the payload names what was empty).
    EmptyName(String),
    /// The SDK reported a bare failure with no detail.
    Failed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotInGame => {
                write!(f, "unrealsdk is not available (off-game or unloaded)")
            }
            Error::NullObject => write!(f, "null object on path"),
            Error::NotFound(what) => write!(f, "not found: {what}"),
            Error::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::ArgCount {
                function,
                expected,
                got,
            } => write!(f, "{function}: expects {expected} args, got {got}"),
            Error::AllocFailed => write!(f, "engine allocator returned null"),
            Error::Duplicate(what) => write!(f, "already registered: {what}"),
            Error::EmptyName(what) => write!(f, "{what} must be non-empty"),
            Error::Failed(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {}

/// Fallible object operation (never panics).
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_the_problem() {
        assert_eq!(
            Error::NotInGame.to_string(),
            "unrealsdk is not available (off-game or unloaded)"
        );
        assert_eq!(
            Error::Duplicate("hook 'F' (i)".to_owned()).to_string(),
            "already registered: hook 'F' (i)"
        );
        assert_eq!(
            Error::EmptyName("command name".to_owned()).to_string(),
            "command name must be non-empty"
        );
        assert_eq!(Error::Failed("add_hook failed".to_owned()).to_string(), "add_hook failed");
    }
}
