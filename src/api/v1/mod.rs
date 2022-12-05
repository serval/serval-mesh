// follow this pattern for endpoint groups
// pub mod <filename>;
// pub use <filename>::*;

/// Respond to ping. Useful for liveness checks.
pub async fn ping() -> &'static str {
    "pong"
}
