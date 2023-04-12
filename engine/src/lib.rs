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
    fs::File,
    path::PathBuf,
};

use anyhow::anyhow;
use cranelift_codegen_meta::isa::Isa;
use extensions::ServalExtension;
use utils::structs::{Permission, WasmResult};
use wasi_common::{
    pipe::{ReadPipe, WritePipe},
    I32Exit,
};
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::{Dir, WasiCtx, WasiCtxBuilder};

pub mod errors;
pub mod extensions;
mod runtime;
use crate::{errors::ServalEngineError, runtime::register_exports};
use wasi_experimental_http_wasmtime::{HttpCtx, HttpState};

#[allow(missing_debug_implementations)]
#[derive(Clone)]
/// Make one of these to get a Wasm runner with the Serval glue.
pub struct ServalEngine {
    extensions: HashMap<String, ServalExtension>,
    engine: Engine,
    linker: Linker<WasiCtx>,
}

impl ServalEngine {
    /// Create a new serval engine.
    pub fn new(extensions: HashMap<String, ServalExtension>) -> Result<Self, ServalEngineError> {
        let mut config = Config::default();
        config.cache_config_load_default().map_err(|_| {
            ServalEngineError::EngineInitializationError(anyhow!(
                "Failed to load default cache config"
            ))
        })?;
        let engine = Engine::new(&config).map_err(|_| {
            ServalEngineError::EngineInitializationError(anyhow!("Failed to instantiate engine"))
        })?;
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

    /// Run the passed-in Wasm executable on the given input bytes.
    pub fn execute(
        &mut self,
        // WebAssembly module to execute
        wasm_module_bytes: &[u8],
        // Data to pass to WebAssembly as stdin
        stdin_bytes: &[u8],
        // List of elevated permissions for this execution run
        permissions: &[Permission],
    ) -> Result<WasmResult, ServalEngineError> {
        let stdout = WritePipe::new_in_memory();
        let stderr = WritePipe::new_in_memory();

        // Link the experimental HTTP support
        let allowed_http_hosts = if permissions.contains(&Permission::AllHttpHosts) {
            // todo: unclear whether we should actually support a wildcard like this
            vec!["insecure:allow-all".to_string()]
        } else {
            permissions
                .iter()
                .filter_map(|perm| match perm {
                    Permission::HttpHost(origin) => Some(origin.to_owned()),
                    _ => None,
                })
                .collect()
        };

        if !allowed_http_hosts.is_empty() {
            let http_state =
                HttpState::new().map_err(ServalEngineError::EngineInitializationError)?;
            http_state
                .add_to_linker(&mut self.linker, move |_| HttpCtx {
                    // todo: there's some confusion around whether
                    allowed_hosts: Some(allowed_http_hosts.clone()),
                    max_concurrent_requests: Some(42),
                })
                .map_err(ServalEngineError::EngineInitializationError)?;
        }

        let stdin = ReadPipe::from(stdin_bytes);
        let mut wasi_builder = WasiCtxBuilder::new()
            .stdin(Box::new(stdin))
            .stdout(Box::new(stdout.clone()))
            .stderr(Box::new(stderr.clone()));

        // Give the engine access to whichever parts of the file system are required
        // TODO: this list should be pulled from the job's manifest, and permissions should be
        // checked against the owner of the job in question and the configuration of this node (that
        // is, the list of file system locations that any job by any user can access should be
        // defined at the node level, and then the subset of those that any job by a specific user
        // can access should exist in our configuration store, and then the subset of those that a
        // specific job by that specific user should exist in the manifest for that job. Phew!
        log::info!("Job has the following permissions: {permissions:?}");

        if permissions.contains(&Permission::ProcRead) {
            let path = PathBuf::from("/proc");
            if !path.exists() {
                return Err(ServalEngineError::UnsupportedFeatureError);
            }

            let dir = Dir::from_std_file(File::open(&path)?);
            wasi_builder = wasi_builder.preopened_dir(dir, path).unwrap();
        }

        let mut store = Store::new(&self.engine, wasi_builder.build());

        log::info!("Module is {} bytes", wasm_module_bytes.len());

        let module = Module::from_binary(&self.engine, wasm_module_bytes)
            .map_err(ServalEngineError::ModuleLoadError)?;

        // Load any custom Wasm node features that the job requires (...and that we have)
        let required_modules = module
            .imports()
            .map(|import| import.module().to_string())
            // Everything that uses WASI is going to try to import wasi_snapshot_preview_1; that's
            // provided by wasmtime_wasi for us.
            .filter(|import| !import.starts_with("wasi_snapshot_"))
            // Our SDK functions are exported under the serval namespace; this is set up in the
            // register_exports function that we call in our constructor, above.
            .filter(|import| import != "serval")
            .collect::<HashSet<String>>();

        log::info!("Job wants the following extensions: {required_modules:?}");

        let allow_all_extensions = permissions.contains(&Permission::AllExtensions);
        for ext_name in required_modules {
            let Some(extension) = self.extensions.get(&ext_name) else {
                // We don't have an extension that matches the expected module name, which
                // means that there is a very good chance that the job will fail when we try to
                // run it. However, hope springs eternal, so let's keep going.
                log::warn!("Extension {ext_name} is not available on this node");
                continue;
            };

            if !allow_all_extensions
                && !permissions.contains(&Permission::Extension(ext_name.to_owned()))
            {
                return Err(ServalEngineError::ExtensionPermissionDenied(ext_name));
            }

            // TODO: implement permissions checking here at some point

            if let Err(err) = extension
                .module_for_engine(&self.engine)
                .map(|ext_module| self.linker.module(&mut store, &ext_name, &ext_module))
            {
                log::warn!("Error when trying to load extension {ext_name}: {err}")
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

        // Here we run the Wasm and trap any errors. We do not consider non-zero exit codes to be
        // an error in *executing* the Wasm, but instead to be information to be returned to the
        // caller.
        let code = match executed {
            Err(e) => {
                if let Some(exit) = e.downcast_ref::<I32Exit>() {
                    exit.0
                } else {
                    // This is a genuine error from the Wasm engine, not a non-zero exit code from the
                    // the Wasm executable.
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
