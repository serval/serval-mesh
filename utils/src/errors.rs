use thiserror::Error;

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

    // A conversion for anyhow::Error
    #[error("anyhow::Error: {0}")]
    AnyhowError(#[from] anyhow::Error),

    /// The caller has attempted to load an object from the blob store with an invalid address.
    #[error("blob address is not a valid hex representation of a sha256 hash `{0}`")]
    BlobAddressInvalid(String),

    /// This blob was not found.
    #[error("no blob found at address `{0}`")]
    BlobAddressNotFound(String),

    /// The agent was unable to store data, despite advertising a storage role.
    #[error("unable to store data: `{0}`")]
    StorageError(String),

    /// This job has no metadata
    #[error("no metadata for job `{0}`")]
    ManifestNotFound(String),

    /// Invalid role string.
    #[error("not a valid role `{0}`")]
    InvalidRole(String),

    /// A conversion for std:io:Error
    #[error("std::io::Error: {0}")]
    IoError(#[from] std::io::Error),

    /// The searched-for mdns service could not be found.
    #[error("mdns service was not found before timeout")]
    ServiceNotFound,

    /// Translation for errors from reqwest.
    #[error("reqwest::Error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    /// Translation for errors from cacache
    #[error("cacache::Error: {0}")]
    CacacheError(#[from] cacache::Error),

    /// Translation for serialization errors from toml.
    #[error("toml::ser::Error: {0}")]
    TomlSerializationError(#[from] toml::ser::Error),

    /// Translation for deserialization errors from toml.
    #[error("toml::de::Error: {0}")]
    TomlDeserializationError(#[from] toml::de::Error),

    /// Translation for errors from ssri.
    #[error("ssri::Error: {0}")]
    SsriError(#[from] ssri::Error),

    #[error("Manifest with a relative binary path was passed to Manifest::from_string; only absolute paths are supported here")]
    RelativeBinaryPathInManifestError,
}

use axum::http::StatusCode;
use axum::response::IntoResponse;

impl IntoResponse for ServalError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            ServalError::AbnormalWasmExit { result: _ } => {
                // We probably shouldn't be responding with this error directly ever,
                // but we provide an implementation just in case. The assumption here is
                // that the WASM executable was bad in some way.
                StatusCode::BAD_REQUEST
            }
            ServalError::BlobAddressInvalid(_) => StatusCode::BAD_REQUEST,
            ServalError::BlobAddressNotFound(_) => StatusCode::NOT_FOUND,
            ServalError::IoError(_) => StatusCode::NOT_FOUND,
            ServalError::ServiceNotFound => StatusCode::NOT_FOUND,
            // Catch-all for anything we don't want to add specific status codes for.
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}
