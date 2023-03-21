use thiserror::Error;
use wasmtime::MemoryAccessError;

#[derive(Error, Debug)]
pub enum ServalEngineError {
    #[error("Failed to get the default export of the binary")]
    DefaultExportUnavailable,

    #[error("Error initializing engine: {0}")]
    EngineInitializationError(anyhow::Error),

    #[error("Error executing binary")]
    ExecutionError {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        error: anyhow::Error,
    },

    #[error("The binary's default export does not match the expected function signature")]
    InvalidDefaultExportFunctionSignature,

    #[error("Guest alloc function did not return a valid pointer")]
    InteropAllocFailed,

    #[error("Guest does not export an alloc function")]
    InteropAllocUnavailable,

    #[error("Reading or writing from guest memory failed")]
    InteropMemoryAccessError(MemoryAccessError),

    #[error("std::io::Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to load WASM module")]
    ModuleLoadError(anyhow::Error),

    #[error("Error reading bytes from stderr pipe")]
    StandardErrorReadError(),

    #[error("Error reading bytes from stdout pipe")]
    StandardOutputReadError(),

    #[error("Host platform does not support a required feature")]
    UnsupportedFeatureError,

    #[error("Job does not have permission to use extension '{0}'")]
    ExtensionPermissionDenied(String),
}
