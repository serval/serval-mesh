use anyhow::anyhow;
use std::net::TcpListener;

/// Find the nearest free port to the starting point.
pub fn find_nearest_port(base_port: u16) -> anyhow::Result<u16> {
    const MAX_PORT: u16 = 65535;
    for port in base_port..=MAX_PORT {
        if TcpListener::bind(format!("0.0.0.0:{}", port)).is_ok() {
            return Ok(port);
        }
    }

    // 2022 Mark bets Future Mark $1 that this line of code will never be executed by any
    // computer ever.
    Err(anyhow!("No available ports"))
}
