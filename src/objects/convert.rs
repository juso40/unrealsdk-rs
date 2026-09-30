//! `PropValue` <-> typed conversions (`Obj::get`, `Obj::set`, `call!` args).

use crate::types::UObject;

use super::{
    mismatch, DelegateValue, Error, Obj, PropValue, Result, SoftObjectValue, StructValue,
    WeakObjectValue,
};

// ---------------------------------------------------------------------------
// PropValue <-> typed conversions (`Obj::get`, `Obj::set`, `call!` args)
// ---------------------------------------------------------------------------

impl From<i32> for PropValue {
    fn from(v: i32) -> Self {
        PropValue::Int(v)
    }
}
impl From<f32> for PropValue {
    fn from(v: f32) -> Self {
        PropValue::Float(v)
    }
}
impl From<bool> for PropValue {
    fn from(v: bool) -> Self {
        PropValue::Bool(v)
    }
}
impl From<u8> for PropValue {
    fn from(v: u8) -> Self {
        PropValue::Byte(v)
    }
}
impl From<String> for PropValue {
    fn from(v: String) -> Self {
        PropValue::String(v)
    }
}
impl From<&str> for PropValue {
    fn from(v: &str) -> Self {
        PropValue::String(v.to_owned())
    }
}
impl From<*mut UObject> for PropValue {
    fn from(v: *mut UObject) -> Self {
        PropValue::Object(v)
    }
}
impl From<Obj> for PropValue {
    fn from(v: Obj) -> Self {
        PropValue::Object(v.raw())
    }
}
impl From<StructValue> for PropValue {
    fn from(v: StructValue) -> Self {
        PropValue::Struct(v)
    }
}
impl From<Vec<PropValue>> for PropValue {
    fn from(v: Vec<PropValue>) -> Self {
        PropValue::Array(v)
    }
}
impl From<DelegateValue> for PropValue {
    fn from(v: DelegateValue) -> Self {
        PropValue::Delegate(v)
    }
}
impl From<Vec<DelegateValue>> for PropValue {
    fn from(v: Vec<DelegateValue>) -> Self {
        PropValue::MulticastDelegate(v)
    }
}
impl From<WeakObjectValue> for PropValue {
    fn from(v: WeakObjectValue) -> Self {
        PropValue::WeakObject(v)
    }
}
impl From<SoftObjectValue> for PropValue {
    fn from(v: SoftObjectValue) -> Self {
        PropValue::SoftObject(v)
    }
}

impl TryFrom<PropValue> for f32 {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Float(x) => Ok(x),
            other => Err(mismatch("f32 (FloatProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for bool {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Bool(x) => Ok(x),
            other => Err(mismatch("bool (BoolProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for i32 {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Int(x) => Ok(x),
            PropValue::Byte(x) => Ok(i32::from(x)),
            PropValue::Enum { value, .. } => {
                i32::try_from(value).map_err(|_| Error::TypeMismatch {
                    expected: "i32".to_owned(),
                    got: format!("Enum({value})"),
                })
            }
            other => Err(mismatch("i32 (IntProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for i64 {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Int(x) => Ok(i64::from(x)),
            PropValue::Byte(x) => Ok(i64::from(x)),
            PropValue::Enum { value, .. } => Ok(value),
            other => Err(mismatch("i64", &other)),
        }
    }
}

impl TryFrom<PropValue> for u8 {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Byte(x) => Ok(x),
            PropValue::Enum { value, .. } => {
                u8::try_from(value).map_err(|_| Error::TypeMismatch {
                    expected: "u8".to_owned(),
                    got: format!("Enum({value})"),
                })
            }
            other => Err(mismatch("u8 (ByteProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for String {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::String(s) | PropValue::Name(s) => Ok(s),
            other => Err(mismatch("String (Str/NameProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for *mut UObject {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Object(o) => Ok(o),
            other => Err(mismatch("object (ObjectProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for Obj {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Object(o) => Obj::new(o),
            other => Err(mismatch("object (ObjectProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for StructValue {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Struct(s) => Ok(s),
            other => Err(mismatch("struct (StructProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for Vec<PropValue> {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Array(a) | PropValue::StaticArray(a) => Ok(a),
            other => Err(mismatch("array (ArrayProperty / static array)", &other)),
        }
    }
}

impl TryFrom<PropValue> for DelegateValue {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::Delegate(d) => Ok(d),
            other => Err(mismatch("delegate (DelegateProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for Vec<DelegateValue> {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::MulticastDelegate(d) => Ok(d),
            other => Err(mismatch(
                "delegate list (MulticastDelegateProperty)",
                &other,
            )),
        }
    }
}

impl TryFrom<PropValue> for WeakObjectValue {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::WeakObject(w) => Ok(w),
            other => Err(mismatch("weak object (WeakObjectProperty)", &other)),
        }
    }
}

impl TryFrom<PropValue> for SoftObjectValue {
    type Error = Error;
    fn try_from(v: PropValue) -> Result<Self> {
        match v {
            PropValue::SoftObject(s) => Ok(s),
            other => Err(mismatch("soft object (SoftObjectProperty)", &other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_round_trip() {
        assert_eq!(f32::try_from(PropValue::Float(1.5)), Ok(1.5));
        assert_eq!(i32::try_from(PropValue::Byte(7)), Ok(7));
        assert_eq!(i64::try_from(PropValue::Enum { enum_name: "E".into(), value: -3 }), Ok(-3));
        assert_eq!(String::try_from(PropValue::Name("Foo".into())), Ok("Foo".to_owned()));
        assert_eq!(PropValue::from(2.0f32), PropValue::Float(2.0));
        assert_eq!(PropValue::from("x"), PropValue::String("x".to_owned()));
        let weak = WeakObjectValue {
            object_index: 3,
            object_serial: 7,
        };
        assert_eq!(WeakObjectValue::try_from(PropValue::from(weak)), Ok(weak));
        assert_eq!(
            Vec::<PropValue>::try_from(PropValue::StaticArray(vec![PropValue::Int(1)])),
            Ok(vec![PropValue::Int(1)])
        );
        assert!(matches!(
            f32::try_from(PropValue::Bool(true)),
            Err(Error::TypeMismatch { .. })
        ));
    }

}
