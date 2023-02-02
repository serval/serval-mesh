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

use anyhow::anyhow;
use cranelift_codegen_meta::isa::Isa;
use utils::structs::WasmResult;
use wasi_common::{
    pipe::{ReadPipe, WritePipe},
    I32Exit,
};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

pub mod errors;
mod runtime;
use crate::{errors::ServalEngineError, runtime::register_exports};

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
    pub fn new(extensions: HashMap<String, PathBuf>) -> Result<Self, ServalEngineError> {
        let engine = Engine::default();
        let mut linker: Linker<WasiCtx> = Linker::new(&engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| s)
            .map_err(ServalEngineError::EngineInitializationError)?;

        // Wire up our host functions (functionality that we want to expose to the jobs we run)
        register_exports(&mut linker).map_err(|_| {
            ServalEngineError::EngineInitializationError(anyhow!("Failed to register exports"))
        })?;

        Ok(Self {
            engine,
            linker,
            extensions,
        })
    }

    /// Run the passed-in WASM executable on the given input bytes.
    pub fn execute(
        &mut self,
        binary: &[u8],
        input: &[u8],
    ) -> Result<WasmResult, ServalEngineError> {
        let stdout = WritePipe::new_in_memory();
        let stderr = WritePipe::new_in_memory();

        let stdin = ReadPipe::from(input);
        let wasi = WasiCtxBuilder::new()
            .stdin(Box::new(stdin))
            .stdout(Box::new(stdout.clone()))
            .stderr(Box::new(stderr.clone()))
            .build();

        let mut store = Store::new(&self.engine, wasi);

        let module = Module::from_binary(&self.engine, binary)
            .map_err(ServalEngineError::ModuleLoadError)?;

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
            let ext_binary = &fs::read(filename)?[..];
            let ext_module = Module::from_binary(&self.engine, ext_binary)
                .map_err(ServalEngineError::ModuleLoadError)?;
            if let Err(err) = self.linker.module(&mut store, &ext_name, &ext_module) {
                let filename = filename.to_string_lossy();
                log::warn!("Error when trying to load extension {ext_name} from {filename}: {err}")
            };
        }

        // Note: Any functions we want to expose to the module must be registered with the linker
        // before the module itself, which we are about to do. I am leaving this note for future
        // spelunkers: calling `linker.func_wrap(...)` etc. at any point after the following line
        // will not work as you expect.
        self.linker
            .module(&mut store, "", &module)
            .map_err(ServalEngineError::EngineInitializationError)?;

        let default_export = self
            .linker
            .get_default(&mut store, "")
            .map_err(|_| ServalEngineError::DefaultExportUnavailable)?;
        let default_func = default_export
            .typed::<(), ()>(&store)
            .map_err(|_| ServalEngineError::InvalidDefaultExportFunctionSignature)?;
        let executed = default_func.call(&mut store, ());

        // We have to drop the store here or we'll be unable to consume data from the WritePipe. See wasmtime docs.
        drop(store);

        let outbytes: Vec<u8> = stdout
            .try_into_inner()
            .map_err(|_| ServalEngineError::StandardOutputReadError())?
            .into_inner();

        let errbytes: Vec<u8> = stderr
            .try_into_inner()
            .map_err(|_| ServalEngineError::StandardErrorReadError())?
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
                    // the WASM executable.
                    return Err(ServalEngineError::ExecutionError {
                        error: e,
                        stdout: outbytes,
                        stderr: errbytes,
                    });
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

    pub fn is_available() -> bool {
        // The cranelift code generator that underpins wasmtime doesn't support every architecture
        // under the sun; in particular, it doesn't support 32-bit ARM, which is a potentially
        // viable target for the Serval agent in general.
        Isa::from_arch(std::env::consts::ARCH).is_some()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn write_tests_please() {}
}
