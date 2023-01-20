use wasi_common::WasiCtx;
use wasmtime::{Caller, Linker};

use crate::runtime::helpers::{get_memory_from_caller, read_bytes, write_bytes};

mod helpers;

///
/// Registers all of our Serval-specific functions with the given Linker instance.
///
pub fn register_exports(linker: &mut Linker<WasiCtx>) -> Result<(), anyhow::Error> {
    // The first parameter to func_wrap is the name of the import namespace and the second is the
    // name of the function. The default namespace for WASM imports is "env". For example, this:
    // ```
    // linker.func_wrap("env", "add", |a: i32, b: i32| -> i32 { a + b })?;
    // ```
    // will define a function at `env::add`, which you can access in your WASM job under the name
    // "add" with the following extern block:
    // ```
    // extern "C" { fn add(a: i32, b: i32) -> i32; }
    // ```
    // If you'd like your function to be under a different namespace, define it like this...
    // ```
    // linker.func_wrap("foo", "add", |a: i32, b: i32| -> i32 { a + b })?;
    // ```
    // ...and import like this:
    // ```
    // #[link(wasm_import_module = "foo")]
    // extern "C" { fn add(a: i32, b: i32) -> i32; }
    // ```
    linker.func_wrap("serval", "add", add)?;

    // TODO: load custom capabilities and expose them, exact details TBD

    Ok(())
}

///
/// This solely exists to have a trivial function in the serval namespace that samples can easily
/// call to verify that things are working properly.
///
fn add(a: i32, b: i32) -> i32 {
    a + b
}
