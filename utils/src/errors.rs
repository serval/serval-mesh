use thiserror::Error;

use crate::structs::WasmResult;

// A starting point for our internal errors. We can break this up or
// rename things if we decide to. The goal here is to start using named
// errors internally in our libraries, while allowing applications to use
// anyhow for final error handling.

/// An alias for an oft-used result type.
pub type ServalResult<T> = Result<T, ServalError>;

#[derive(Error, Debug)]
/// Error types used by internal serval libraries to communicate details about
/// errors specific to our implementation details.
pub enum ServalError {
    /// If this ever happens, Mark owes you a quarter.
    #[error("unable to find a free port >= `{0}`")]
    NoFreePorts(u16),

    #[error("the WASM executable terminated abnormally; code={}", result.code)]
    AbnormalWasmExit { result: WasmResult },

    // A conversion for anyhow::Error
    #[error("anyhow::Error: {0}")]
    AnyhowError(#[from] anyhow::Error),

    /// The caller has attempted to load an object from the blob store with an invalid address.
    #[error("blob address is not a valid SRI string `{0}`")]
    BlobAddressInvalid(String),

    /// This blob was not found.
    #[error("no blob found at address `{0}`")]
    BlobAddressNotFound(String),

    /// The agent was unable to store data, despite advertising a storage role.
    #[error("unable to store data: `{0}`")]
    StorageError(String),

    #[error("data not found; sri: `{0}`")]
    DataNotFound(String),

    /// This job has no metadata
    #[error("no manifest found for task `{0}`")]
    ManifestNotFound(String),

    /// Could not locate the named executable
    #[error("no data found for executable `{0}`")]
    ExecutableNotFound(String),

    /// Invalid role string.
    #[error("not a valid role `{0}`")]
    InvalidRole(String),

    /// A conversion for std:io:Error
    #[error("std::io::Error: {0}")]
    IoError(#[from] std::io::Error),

    /// The searched-for service could not be found.
    #[error("service was not found before timeout")]
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

    /// Several translations for errors from the aws sdk.
    #[error("aws_sdk_s3::error::SdkError: {0}")]
    S3SHeadError(
        #[from] aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::head_object::HeadObjectError>,
    ),

    #[error("aws_sdk_s3::error::SdkError: {0}")]
    S3GetError(
        #[from] aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::get_object::GetObjectError>,
    ),

    #[error("aws_sdk_s3::error::SdkError: {0}")]
    S3BytestreamError(#[from] aws_sdk_s3::primitives::ByteStreamError),

    #[error("std::string::FromUtf8Error: {0}")]
    S3Utf8Error(#[from] std::string::FromUtf8Error),

    #[error("Manifest with a relative binary path was passed to Manifest::from_string; only absolute paths are supported here")]
    RelativeBinaryPathInManifestError,

    #[error("Manifest contains an invalid job name: {0}")]
    InvalidManifestName(String),
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
