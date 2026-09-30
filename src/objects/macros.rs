//! Dotted-path macros: plain `macro_rules!`, expanding to `Obj` chains.

// ---------------------------------------------------------------------------
// Dotted-path macros (plain macro_rules!, expanding to the chains above)
// ---------------------------------------------------------------------------

/// Walk object-property hops along a dotted path to an object.
///
/// `obj!(o, A.B.C)` expands to `o.child("A")?.child("B")?.child("C")`,
/// a `Result<Obj>`. Idents only (no keyword segments).
#[macro_export]
macro_rules! obj {
    ($base:expr, $($seg:ident).+ $(,)?) => {{
        $crate::obj!(@go $base, $($seg),+)
    }};
    (@go $acc:expr, $last:ident) => {
        $acc.child(::core::stringify!($last))
    };
    (@go $acc:expr, $head:ident, $($rest:ident),+) => {
        $crate::obj!(@go $acc.child(::core::stringify!($head))?, $($rest),+)
    };
}

/// Read a property along a dotted path.
///
/// `get!(o, A.B.Health)` reads a `PropValue`; `get!(o, A.B.Health as f32)`
/// reads and converts. Expands to a `Result`.
#[macro_export]
macro_rules! get {
    ($base:expr, $($seg:ident).+ as $ty:ty $(,)?) => {{
        $crate::get!(@ty $ty, $base, $($seg),+)
    }};
    ($base:expr, $($seg:ident).+ $(,)?) => {{
        $crate::get!(@go $base, $($seg),+)
    }};
    (@ty $ty:ty, $acc:expr, $last:ident) => {
        $acc.get::<$ty>(::core::stringify!($last))
    };
    (@ty $ty:ty, $acc:expr, $head:ident, $($rest:ident),+) => {
        $crate::get!(@ty $ty, $acc.child(::core::stringify!($head))?, $($rest),+)
    };
    (@go $acc:expr, $last:ident) => {
        $acc.get_prop(::core::stringify!($last))
    };
    (@go $acc:expr, $head:ident, $($rest:ident),+) => {
        $crate::get!(@go $acc.child(::core::stringify!($head))?, $($rest),+)
    };
}

/// Write a property along a dotted path.
///
/// `set!(o, A.B.Health, 2.0f32)` expands to
/// `o.child("A")?.child("B")?.set("Health", 2.0f32)`, a `Result<()>`.
#[macro_export]
macro_rules! set {
    ($base:expr, $($seg:ident).+, $val:expr $(,)?) => {{
        $crate::set!(@go $base, $val, $($seg),+)
    }};
    (@go $acc:expr, $val:expr, $last:ident) => {
        $acc.set(::core::stringify!($last), $val)
    };
    (@go $acc:expr, $val:expr, $head:ident, $($rest:ident),+) => {
        $crate::set!(@go $acc.child(::core::stringify!($head))?, $val, $($rest),+)
    };
}

/// Call a function along a dotted path with positional args (each arg goes
/// through `Into<PropValue>`).
///
/// `call!(o, A.B.Fire, 1i32, "x")` expands to
/// `o.child("A")?.child("B")?.call("Fire", &[1i32.into(), "x".into()])`,
/// a `Result<CallOutcome>`.
#[macro_export]
macro_rules! call {
    ($base:expr, $($seg:ident).+ $(, $arg:expr)* $(,)?) => {{
        $crate::call!(@go $base, [$($crate::call!(@arg $arg)),*], $($seg),+)
    }};
    (@arg $arg:expr) => {
        ::core::convert::Into::<$crate::objects::PropValue>::into($arg)
    };
    (@go $acc:expr, $args:expr, $last:ident) => {
        $acc.call(::core::stringify!($last), &$args)
    };
    (@go $acc:expr, $args:expr, $head:ident, $($rest:ident),+) => {
        $crate::call!(@go $acc.child(::core::stringify!($head))?, $args, $($rest),+)
    };
}

#[cfg(test)]
mod tests {
    use crate::objects::{Error, Obj, PropValue, Result};

    fn null_obj() -> Obj {
        Obj(std::ptr::null_mut())
    }
    // The macros expand to `?` chains; these compile-check the shapes and
    // assert the off-game degradation contract (null object -> NullObject).
    fn walk(o: Obj) -> Result<Obj> {
        obj!(o, A.B)
    }
    fn read_typed(o: Obj) -> Result<f32> {
        get!(o, A.B.Health as f32)
    }
    fn read_raw(o: Obj) -> Result<PropValue> {
        get!(o, Health)
    }
    fn write(o: Obj) -> Result<()> {
        set!(o, A.B.Health, 2.0f32)
    }
    fn invoke(o: Obj) -> Result<Option<PropValue>> {
        Ok(call!(o, A.B.Fire, 1i32, "x")?.ret)
    }

    #[test]
    fn macros_expand_and_degrade_off_game() {
        // Off-game `live()` reports `NotInGame` first; in-game a null wrap
        // is `NullObject`. Either way every shape degrades to an `Err`.
        let o = null_obj();
        let degraded =
            |r: Result<()>| assert!(matches!(r, Err(Error::NotInGame | Error::NullObject)));
        assert!(matches!(walk(o), Err(Error::NotInGame | Error::NullObject)));
        assert!(matches!(read_typed(o), Err(Error::NotInGame | Error::NullObject)));
        assert!(matches!(read_raw(o), Err(Error::NotInGame | Error::NullObject)));
        degraded(write(o));
        assert!(matches!(invoke(o), Err(Error::NotInGame | Error::NullObject)));
    }
}
