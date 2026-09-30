# unrealsdk-rs

Rust bindings to the [unrealsdk](https://github.com/bl-sdk/unrealsdk) C API (WILLOW flavour: BL1/BL2/TPS/AoDK).   

*This crate is not affiliated with the `unrealsdk` nor is it a replacement for the `unrealsdk`.*

## Should I use this?

Probably not. 

## Why does this exist?

I want to learn rust and I want to mod BL2… So why not combine both?  
That's why I created this crate which allows writing (hopefully safe) rust code that can interact with the `unrealsdk.dll`.

## How to use it

This crate resolves the `UNREALSDK_CAPI` export table out of the `unrealsdk.dll` which is already loaded by the game, so your crate compiles with plain `cargo build`.

### From a Rust crate

Your artifact is a `cdylib` or `dylib` that the host loads into the game
process. Add the dependency and build for the game target, 32-bit Windows.

```toml
[dependencies]
unrealsdk-rs = { git = "https://github.com/juso40/unrealsdk-rs", tag = "v0.1.0" }
```

```sh
cargo build --target i686-pc-windows-gnullvm --release
```

```rust
use unrealsdk_rs as unrealsdk;
use unrealsdk::{get, set};
use unrealsdk::objects::{self, Obj};

// The host initializes the SDK on its own thread. Wait for it.
while !unrealsdk::is_initialized() {
    std::thread::sleep(std::time::Duration::from_millis(50));
}

let obj = Obj::new(objects::find_object(None, "Transient.some_object").unwrap()).unwrap();
let scale = get!(obj, DrawScale as f32).unwrap();
set!(obj, DrawScale, scale * 2.0f32).unwrap();
```


### In a PythonSDK mod (the `unrealsdk` Python module)

Your mod is a Python package and the `unrealsdk` Python module wraps the same
`unrealsdk.dll` this crate binds. Python and Rust share one SDK instance, and
neither initializes it. Put the engine work in a pyo3 extension module shipped
inside the package.

```
mymod/
  __init__.py      # the mod entry, imports _native
  _native.pyd      # your Rust cdylib
native/
  Cargo.toml
  src/lib.rs
```

```toml
# native/Cargo.toml
[lib]
name = "_native"
crate-type = ["cdylib"]

[dependencies]
pyo3 = { version = "0.26", features = ["extension-module"] }
unrealsdk-rs = { git = "https://github.com/juso40/unrealsdk-rs", tag = "v0.1.0" }
```

```rust
// native/src/lib.rs
use pyo3::prelude::*;
use unrealsdk_rs as unrealsdk;
use unrealsdk::objects::{self, Obj};

/// Find an object and read one float property, in a single Python call.
#[pyfunction]
fn get_float(obj_path: &str, prop_name: &str) -> Option<f32> {
    let obj = Obj::new(objects::find_object(None, obj_path)?).ok()?;
    obj.get::<f32>(prop_name).ok()
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(get_float, m)?)?;
    Ok(())
}
```

```python
# mymod/__init__.py
import unrealsdk
from . import _native

scale = _native.get_float("Transient.some_object", "DrawScale")
print(f"DrawScale = {scale}")
```

Build with `cargo build --target i686-pc-windows-gnullvm --release`, then
rename `target/i686-pc-windows-gnullvm/release/_native.dll` to `_native.pyd`
and drop it next to `__init__.py`. A `.pyd` is a CPython extension, so it must
match the interpreter the game's PythonSDK embeds, including the 32-bit build.
pyo3 targets CPython 3.x.

PythonSDK has already initialized the SDK by the time your mod is imported, so
`is_initialized()` is true from the first line and no wait loop is needed.

Keep the traffic going one way, Python to Rust. Python-level hooks registered
through the `unrealsdk` module run on the game thread with the GIL held, so
calling `_native` from them is safe in both directions. The reverse is not:
the callbacks this crate registers (`add_hook`, `add_command`)
run without the GIL. Calling Python from them is not safe.
