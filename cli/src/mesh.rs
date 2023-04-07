use owo_colors::OwoColorize;
use tokio::time::sleep;

use std::time::Duration;

use utils::mesh::{KaboodlePeer, PeerMetadata};

pub async fn monitor_mesh() -> anyhow::Result<()> {
    println!("Monitoring mesh ...");
    let mut mesh = super::peers::create_mesh_peer().await?;
    let mut discover_rx = mesh
        .discover_peers()
        .expect("Unable to get arrivals channel!");
    let mut depart_rx = mesh
        .discover_departures()
        .expect("Unable to get departures channel!");

    loop {
        while let Some((addr, identity)) = discover_rx.recv().await {
            let peer = PeerMetadata::from_identity(addr.ip(), identity.to_vec());
            print!("✅ {} {} @ {addr}", "JOINED:".blue(), peer.instance_id(),);
            if !peer.roles().is_empty() {
                print!(
                    "; roles: {}",
                    peer.roles()
                        .iter()
                        .map(|xs| xs.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                );
            }
            if let Some(http_addr) = peer.http_address() {
                print!("; http port: {}", http_addr.port());
            }
            println!();
        }
        while let Some(addr) = depart_rx.recv().await {
            println!("❌ {} {}", "DEPARTED:".red(), addr);
        }
        sleep(Duration::from_secs(1)).await;
    }
    // on ctrl-C, clean up?
}
