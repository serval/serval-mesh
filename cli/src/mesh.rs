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
        if let Some((addr, identity)) = discover_rx.recv().await {
            let peer = PeerMetadata::from_identity(addr, identity.to_vec());
            if let Some(peer_address) = peer.address() {
                print!(
                    "✅ {} {} @ {peer_address}",
                    "JOINED:".blue(),
                    peer.instance_id()
                );
            } else {
                print!(
                    "⚠️ {} {} peer with no address",
                    "JOINED:".blue(),
                    peer.instance_id()
                );
            }
            println!(
                "; roles: {}",
                peer.roles()
                    .iter()
                    .map(|xs| xs.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
        }
        if let Some(addr) = depart_rx.recv().await {
            println!("❌ {} {}", "DEPARTED:".red(), addr);
        }
        sleep(Duration::from_secs(5)).await;
    }
    // on ctrl-C, clean up?
}
