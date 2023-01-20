#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use utils::errors::ServalError;
use utils::structs::WasmResult;
use wasi_common::{
    pipe::{ReadPipe, WritePipe},
    I32Exit,
};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

#[allow(missing_debug_implementations)]
#[derive(Clone)]
/// Make one of these to get a WASM runner with the Serval glue.
pub struct ServalEngine {
    engine: Engine,
    linker: Linker<WasiCtx>,
}

impl ServalEngine {
    /// Create a new serval engine. There is nothing to configure.
    pub fn new() -> anyhow::Result<Self> {
        let engine = Engine::default();
        let mut linker: Linker<WasiCtx> = Linker::new(&engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| s)?;

        // Wire up our host functions (e.g. functionality that we want to expose to the jobs we
        // run). The first parameter to func_wrap is the name of the import namespace and the
        // second is the name of the function. The default namespace for WASM imports is "env".
        // For example, this:
        // ```
        // linker.func_wrap("env", "add", |a: i32, b: i32| -> i32 { a + b })?;
        // ```
        // will define a function at `env::add`, which you can access in your WASM job under the
        // name "add" with the following extern block:
        // ```
        // extern "C" { fn add(a: i32, b: i32) -> i32; }
        // ```
        // If you'd like your function to be under a different namespace, define it like this...
        // ```
        // linker.func_wrap("foo", "add", |a: i32, b: i32| -> i32 { a + b })?;
        // ```
        // ...and import like this:
        // ```
        // #[link(wasm_import_module = "serval")]
        // extern "C" { fn add(a: i32, b: i32) -> i32; }
        // ```

        // This exists solely so there's *something* for jobs to import, just so ensure the
        // mechanisms all work.
        linker.func_wrap("serval", "add", |a: i32, b: i32| -> i32 { a + b })?;

        Ok(Self { engine, linker })
    }

    /// Run the passed-in WASM executable on the given input bytes.
    pub fn execute(&mut self, binary: &[u8], input: &[u8]) -> Result<WasmResult, ServalError> {
        let stdout = WritePipe::new_in_memory();
        let stderr = WritePipe::new_in_memory();

        let stdin = ReadPipe::from(input);
        let wasi = WasiCtxBuilder::new()
            .stdin(Box::new(stdin))
            .stdout(Box::new(stdout.clone()))
            .stderr(Box::new(stderr.clone()))
            .build();

        let mut store = Store::new(&self.engine, wasi);
        let module = Module::from_binary(&self.engine, binary)?;
        self.linker.module(&mut store, "", &module)?;

        let executed = self
            .linker
            .get_default(&mut store, "")?
            .typed::<(), ()>(&store)?
            .call(&mut store, ());

        // We have to drop the store here or we'll be unable to consume data from the WritePipe. See wasmtime docs.
        drop(store);

        let outbytes: Vec<u8> = stdout
            .try_into_inner()
            .map_err(|_err| anyhow::Error::msg("failed to read stdout from the engine results"))?
            .into_inner();

        let errbytes: Vec<u8> = stderr
            .try_into_inner()
            .map_err(|_err| anyhow::Error::msg("failed to read stdout from the engine results"))?
            .into_inner();

        // Here we run the WASM and trap any errors. We do not consider non-zero exit codes to be
        // an error in *executing* the WASM, but instead to be information to be returned to the
        // caller.
        let code = match executed {
            Err(e) => {
                if let Some(exit) = e.downcast_ref::<I32Exit>() {
                    exit.0
                } else {
                    // This is a genuine error from the WASM engine, not a non-zero exit code from the
                    // the WASM executable. We report this as -1. Your improvements to this signaling
                    // method welcome.
                    -1
                }
            }
            Ok(_) => 0,
        };

        let result = WasmResult {
            code,
            stdout: outbytes,
            stderr: errbytes,
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn write_tests_please() {}
}
