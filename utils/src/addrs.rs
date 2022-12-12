use if_addrs::{IfAddr, Ifv4Addr};
use std::net::Ipv4Addr;

pub fn my_ipv4_addrs() -> Vec<Ipv4Addr> {
    my_ipv4_interfaces().iter().map(|i| i.ip).collect()
}

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
