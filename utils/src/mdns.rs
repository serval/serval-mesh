use mdns_sd::{ServiceDaemon, ServiceInfo};

use std::{collections::HashMap, net::Ipv4Addr};

use crate::errors::ServalError;
use crate::networking::my_ipv4_addrs;

/// Advertise a service with the given name over MDNS.
pub fn advertise_service(
    service_name: &str,
    port: u16,
    props: Option<HashMap<String, String>>,
) -> Result<(), ServalError> {
    let mdns = ServiceDaemon::new()?;

    // TODO: enumerate and include IPv6 addresses
    let my_addrs: Vec<Ipv4Addr> = my_ipv4_addrs();

    let service_domain = format!("_{service_name}._tcp.local.");
    let service_hostname = format!("{service_name}.local.");

    log::info!("Advertising {service_name}; domain={service_domain} port={port} props={props:?}");

    // Register our service
    let service_info = ServiceInfo::new(
        &service_domain,
        service_name,
        &service_hostname,
        &my_addrs[..],
        port,
        props,
    )?;

    mdns.register(service_info)?;

    Ok(())
}
