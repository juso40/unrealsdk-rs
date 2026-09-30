//! Typed object handles: [`ObjectRef<T>`] over the raw pointer API.
//!
//! The Rust stand-in for the SDK's `ClassTraits<T>` layer
//! (`class_traits.h`): `find::<UFunction>("Engine.Foo:Bar")` resolves the
//! path once and checks the result really is a `UFunction`, then carries
//! the type in signatures. The `T` tags are the opaque handle types from
//! [`crate::types`]; nothing is dereferenced through them.

use core::marker::PhantomData;

use crate::types::{UClass, UConst, UEnum, UFunction, UObject, UScriptStruct};

use super::find::{find_class, find_object, is_kind_of, object_class_name, object_outer};
use super::{Error, Obj, Result};

/// Tags an engine class for [`ObjectRef`] (`ClassTraits<T>::NAME`,
/// `class_traits.h`). Implemented on the opaque handle types in
/// [`crate::types`].
pub trait ClassTag {
    /// The engine class name ("Class", "Function", ...).
    const CLASS: &'static str;
}

impl ClassTag for UObject {
    const CLASS: &'static str = "Object";
}
impl ClassTag for UClass {
    const CLASS: &'static str = "Class";
}
impl ClassTag for UFunction {
    const CLASS: &'static str = "Function";
}
impl ClassTag for UScriptStruct {
    const CLASS: &'static str = "ScriptStruct";
}
impl ClassTag for UEnum {
    const CLASS: &'static str = "Enum";
}
impl ClassTag for UConst {
    const CLASS: &'static str = "Const";
}

/// A typed handle to a live engine object of class `T` (or derived).
/// `Copy`, like [`Obj`]: it wraps the engine pointer and keeps nothing
/// alive (GC survival is `RF_*`'s job, see [`crate::flags::object_flags`]).
///
/// ```
/// # use unrealsdk_rs::objects::refs::{find, ObjectRef};
/// # use unrealsdk_rs::types::UClass;
/// # fn demo() -> unrealsdk_rs::objects::Result<()> {
/// let cls: ObjectRef<UClass> = find("Engine.Texture2D")?;
/// println!("{}", cls.name()?);
/// # Ok(())
/// # }
/// ```
pub struct ObjectRef<T: ClassTag> {
    ptr: *mut UObject,
    _ty: PhantomData<fn() -> T>,
}

impl<T: ClassTag> core::fmt::Debug for ObjectRef<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ObjectRef<{}>({:p})", T::CLASS, self.ptr)
    }
}

impl<T: ClassTag> PartialEq for ObjectRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T: ClassTag> Eq for ObjectRef<T> {}

impl<T: ClassTag> Clone for ObjectRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ClassTag> Copy for ObjectRef<T> {}

impl<T: ClassTag> ObjectRef<T> {
    /// Wrap a pointer already known to hold a `T` (no class check).
    /// [`Error::NullObject`] on null.
    pub fn unchecked(ptr: *mut UObject) -> Result<Self> {
        if ptr.is_null() {
            return Err(Error::NullObject);
        }
        Ok(Self {
            ptr,
            _ty: PhantomData,
        })
    }

    /// Wrap a pointer and verify the object's class chain contains
    /// [`ClassTag::CLASS`] (the SDK's `validate_type` check,
    /// `class_name.h` — this returns [`Error::TypeMismatch`] where the
    /// C++ throws). [`Error::NotInGame`] off-game.
    pub fn new(ptr: *mut UObject) -> Result<Self> {
        let r = Self::unchecked(ptr)?;
        let want = find_class(T::CLASS).ok_or(Error::NotInGame)?;
        match is_kind_of(ptr, want) {
            Some(true) => Ok(r),
            Some(false) => Err(Error::TypeMismatch {
                expected: T::CLASS.to_owned(),
                got: object_class_name(ptr),
            }),
            None => Err(Error::NotInGame),
        }
    }

    /// The raw engine pointer, typed as `T`.
    pub fn raw(self) -> *mut T {
        self.ptr as *mut T
    }

    /// The handle as an untyped [`Obj`].
    pub fn as_obj(self) -> Obj {
        Obj(self.ptr)
    }

    /// Object name.
    pub fn name(self) -> Result<String> {
        self.as_obj().name()
    }

    /// Full path name (`Outer.Outer.Name`).
    pub fn path(self) -> Result<String> {
        self.as_obj().path()
    }

    /// The object's class as a typed handle.
    pub fn class(self) -> Result<ObjectRef<UClass>> {
        self.as_obj().class()?.try_into()
    }

    /// The object's outer, if any.
    pub fn outer(self) -> Option<ObjectRef<UObject>> {
        ObjectRef::unchecked(object_outer(self.ptr)?).ok()
    }
}

impl<T: ClassTag> From<ObjectRef<T>> for Obj {
    fn from(r: ObjectRef<T>) -> Self {
        Obj(r.ptr)
    }
}

impl<T: ClassTag> TryFrom<Obj> for ObjectRef<T> {
    type Error = Error;
    fn try_from(o: Obj) -> Result<Self> {
        ObjectRef::new(o.raw())
    }
}

/// Find an object by full path and verify its class
/// ([`find_object`], then [`ObjectRef::new`]).
/// [`Error::NotInGame`] off-game, [`Error::NotFound`] when absent,
/// [`Error::TypeMismatch`] when the class is wrong.
pub fn find<T: ClassTag>(path: &str) -> Result<ObjectRef<T>> {
    find_in(std::ptr::null_mut(), path)
}

/// [`find`], constrained to the class `class` (or derived) — the
/// `find_object(UClass*, ...)` overload, `unrealsdk.h`. A null `class`
/// searches all classes.
pub fn find_in<T: ClassTag>(class: *mut UClass, path: &str) -> Result<ObjectRef<T>> {
    if crate::sys::sys().is_none() {
        return Err(Error::NotInGame);
    }
    let obj = find_object(Some(class), path).ok_or_else(|| Error::NotFound(path.to_owned()))?;
    ObjectRef::new(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_and_off_game_degrade() {
        assert_eq!(ObjectRef::<UClass>::unchecked(std::ptr::null_mut()), Err(Error::NullObject));
        assert_eq!(find::<UEnum>("Transient.Nothing"), Err(Error::NotInGame));
    }

    #[test]
    fn class_names_match_uobject_class_names() {
        assert_eq!(<UClass as ClassTag>::CLASS, "Class");
        assert_eq!(<UFunction as ClassTag>::CLASS, "Function");
        assert_eq!(<UScriptStruct as ClassTag>::CLASS, "ScriptStruct");
        assert_eq!(<UEnum as ClassTag>::CLASS, "Enum");
        assert_eq!(<UConst as ClassTag>::CLASS, "Const");
    }
}
