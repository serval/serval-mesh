use thiserror::Error;

// A starting point for our internal errors. We can break this up or
// rename things if we decide to. The goal here is to start using named
// errors internally in our libraries, while allowing applications to use
// anyhow for final error handling.

#[derive(Error, Debug)]
/// Error types used by internal serval libraries to communicate details about
/// errors specific to our implementation details.
pub enum ServalError {
    #[error("unable to find a free port >= `{0}`")]
    NoFreePorts(u16),

    #[error("unable to set up mdns")]
    MdnsError(#[from] mdns_sd::Error),
    //     #[error("an example of a more complex error type (expected {expected:?}, found {found:?})")]
    //     InvalidHeader { expected: String, found: String },
    //
    //     #[error("an error we have no more details about happened")]
    //     Unknown,
    #[error("binary terminated with non-zero exit code")]
    NonZeroExitCode(i32),
}
