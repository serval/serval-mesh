use mdns_sd::{ServiceDaemon, ServiceInfo};

use std::{collections::HashMap, net::Ipv4Addr};

use utils::addrs::my_ipv4_addrs;

pub async fn init_mdns(http_port: u16) -> anyhow::Result<()> {
    let mdns = ServiceDaemon::new().expect("Could not create mdns service daemon");
    let mut props: HashMap<String, String> = HashMap::new();
    props.insert(String::from("http_port"), http_port.to_string());

    // TODO: enumerate nd include IPv6 addresses
    let my_addrs: Vec<Ipv4Addr> = my_ipv4_addrs();

    let service_domain = "_serval_queue._tcp.local.";
    let service_name = "serval_queue";
    let service_hostname = "serval_queue.local.";
    let port = 3456;

    // Register a service.
    let service_info = ServiceInfo::new(
        service_domain,
        service_name,
        service_hostname,
        &my_addrs[..],
        port,
        Some(props),
    )?;

    mdns.register(service_info)?;
    log::info!("Advertising at {service_domain}");

    Ok(())
}
