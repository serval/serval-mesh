use thiserror::Error;
use wasi_common;

use crate::structs::WasmResult;

// A starting point for our internal errors. We can break this up or
// rename things if we decide to. The goal here is to start using named
// errors internally in our libraries, while allowing applications to use
// anyhow for final error handling.

#[derive(Error, Debug)]
/// Error types used by internal serval libraries to communicate details about
/// errors specific to our implementation details.
pub enum ServalError {
    /// If this ever happens, Mark owes you a quarter.
    #[error("unable to find a free port >= `{0}`")]
    NoFreePorts(u16),

    /// MDNS failed for some reason. We wrap up the MDNS library's error here.
    #[error("unable to set up mdns")]
    MdnsError(#[from] mdns_sd::Error),

    //     #[error("an example of a more complex error type (expected {expected:?}, found {found:?})")]
    //     InvalidHeader { expected: String, found: String },
    //
    //     #[error("an error we have no more details about happened")]
    //     Unknown,
    #[error("the WASM executable terminated abnormally; code={}", result.code)]
    AbnormalWasmExit { result: WasmResult },

    /// The WASMtime engine responded with an error.
    #[error("wasmtime engine error")]
    WasmEngineError(#[from] wasi_common::Error),

    /// The caller has attempted to load an object from the blob store with an invalid address.
    #[error("blob address is not a valid hex representation of a sha256 hash `{0}`")]
    BlobAddressInvalid(String),

    /// This blob was not found.
    #[error("no blob found at address `{0}`")]
    BlobAddressNotFound(String),

    /// A conversion for std:io:Error
    #[error("io error")]
    IoError(#[from] std::io::Error),
}
