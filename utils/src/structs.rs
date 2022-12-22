/// The results of running a WASM executable.
#[derive(Debug)]
pub struct WasmResult {
    /// The status code returned by the execution; 0 for normal termination.
    pub code: i32,
    /// Whatever the WASM executable wrote to stdout.
    pub stdout: Vec<u8>,
    /// Whatever the WASM executable wrote to stderr.
    pub stderr: Vec<u8>,
}
