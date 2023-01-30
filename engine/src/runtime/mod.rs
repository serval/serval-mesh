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
    linker.func_wrap("serval", "invoke_capability", invoke_capability)?;

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

const INVOKE_CAPABILITY_ERROR_FAILED_TO_GET_MEMORY: i32 = -1;
const INVOKE_CAPABILITY_ERROR_FAILED_TO_READ_CAPABILITY_NAME: i32 = -2;
const INVOKE_CAPABILITY_ERROR_FAILED_TO_READ_DATA: i32 = -3;
const INVOKE_CAPABILITY_ERROR_FAILED_TO_WRITE_RESPONSE: i32 = -4;

///
/// Invokes the capability with the given name, passing along the given data payload and returning
/// the response from the capability.
///
fn invoke_capability<T>(
    mut caller: Caller<'_, T>,
    capability_name_ptr: u32, // should point to UTF-8 string data
    capability_name_len: u32,
    data_ptr: u32, // can point to anything at all
    data_len: u32,
) -> i32 {
    let Ok(memory) = get_memory_from_caller(&mut caller) else {
        return INVOKE_CAPABILITY_ERROR_FAILED_TO_GET_MEMORY;
    };
    let Ok(buf) = read_bytes(&caller, memory, capability_name_ptr, capability_name_len) else {
        eprintln!("Failed to read from capability_name_len");
        return INVOKE_CAPABILITY_ERROR_FAILED_TO_READ_CAPABILITY_NAME;
    };
    let capability_name = String::from_utf8_lossy(&buf);
    let Ok(data) = read_bytes(&caller, memory, data_ptr, data_len) else {
        eprintln!("Failed to read from data_ptr");
        return INVOKE_CAPABILITY_ERROR_FAILED_TO_READ_DATA;
    };

    let response = format!("Hello there! I can see that you tried to call the {capability_name} capability with {} bytes of data (to wit: {data:?}). Capabilities are not actually implemented yet, but this message did come from the host environment, so that's worth something, right?", data.len());
    let Ok(ptr) = write_bytes(&mut caller, &memory, response.as_bytes().to_vec()) else {
        return INVOKE_CAPABILITY_ERROR_FAILED_TO_WRITE_RESPONSE;
    };

    ptr as i32
}
