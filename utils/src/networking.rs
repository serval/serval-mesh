use if_addrs::{IfAddr, Ifv4Addr};

use std::net::Ipv4Addr;
use std::net::TcpListener;

use crate::errors::ServalError;

/// Get all non-loopback ipv4 addresses for this host.
pub fn my_ipv4_addrs() -> Vec<Ipv4Addr> {
    my_ipv4_interfaces().iter().map(|i| i.ip).collect()
}

/// An implementation detail of my_ipv4_addrs
fn my_ipv4_interfaces() -> Vec<Ifv4Addr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| {
            if i.is_loopback() {
                None
            } else {
                match i.addr {
                    IfAddr::V4(ifv4) => Some(ifv4),
                    _ => None,
                }
            }
        })
        .collect()
}

/// Find the nearest free port to the starting point.
pub fn find_nearest_port(base_port: u16) -> Result<u16, ServalError> {
    for port in base_port..=u16::MAX {
        if TcpListener::bind(format!("0.0.0.0:{port}")).is_ok() {
            return Ok(port);
        }
    }

    // 2022 Mark bets Future Mark $1 that this line of code will never be executed by any
    // computer ever.
    Err(ServalError::NoFreePorts(base_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_port_case() {
        let result = find_nearest_port(8000).expect("we should find a port");
        assert!(result >= 8000, "found port should be >= 8000");
        let result = find_nearest_port(u16::MAX).expect("we should still find a port");
        assert_eq!(result, u16::MAX, "found port should be the max");
    }

    #[test]
    fn ip_addresses_exist() {
        let result = my_ipv4_interfaces();
        assert!(
            !result.is_empty(),
            "we should always get at least one ipv4 address"
        );
    }
}
