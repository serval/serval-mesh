#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};
use utils::errors::ServalError;
use utils::structs::WasmResult;
use wasi_common::{
    pipe::{ReadPipe, WritePipe},
    I32Exit,
};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

mod runtime;
use crate::runtime::register_exports;

#[allow(missing_debug_implementations)]
#[derive(Clone)]
/// Make one of these to get a WASM runner with the Serval glue.
pub struct ServalEngine {
    extensions: HashMap<String, PathBuf>,
    engine: Engine,
    linker: Linker<WasiCtx>,
}

impl ServalEngine {
    /// Create a new serval engine.
    pub fn new(extensions: HashMap<String, PathBuf>) -> anyhow::Result<Self> {
        let engine = Engine::default();
        let mut linker: Linker<WasiCtx> = Linker::new(&engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| s)?;

        // Wire up our host functions (functionality that we want to expose to the jobs we run)
        register_exports(&mut linker)?;

        Ok(Self {
            engine,
            linker,
            extensions,
        })
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

        // Load any custom WASM node features that the job requires (...and that we have)
        let required_modules = module
            .imports()
            .map(|import| import.module().to_string())
            // Everything that uses WASI is going to try to import wasi_snapshot_preview_1; that's
            // provided by wasmtime_wasi for us.
            .filter(|import| !import.starts_with("wasi_snapshot_"))
            // Our SDK functions are exported under the serval namespace; this is set up in the
            // register_exports function that we call in our constructor, above.
            .filter(|import| import != "serval");
        let required_modules: HashSet<String> = HashSet::from_iter(required_modules);

        log::info!("Job wants the following extensions: {required_modules:?}");

        for ext_name in required_modules {
            let Some(filename) = self.extensions.get(&ext_name) else {
                // We don't have an extension that matches the expected module name, which
                // means that there is a very good chance that the job will fail when we try to
                // run it. However, hope springs eternal, so let's keep going.
                log::warn!("Extension {ext_name} is not available on this node");
                continue;
            };
            let ext_module = Module::from_binary(&self.engine, &fs::read(filename)?[..])?;
            if let Err(err) = self.linker.module(&mut store, &ext_name, &ext_module) {
                let filename = filename.to_string_lossy();
                log::warn!("Error when trying to load extension {ext_name} from {filename}: {err}")
            };
        }

        // Note: Any functions we want to expose to the module must be registered with the linker
        // before the module itself, which we are about to do. I am leaving this note for future
        // spelunkers: calling `linker.func_wrap(...)` etc. at any point after the following line
        // will not work as you expect.
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
