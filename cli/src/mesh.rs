use std::io::{stdin, Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use owo_colors::OwoColorize;
use tokio::time::sleep;
use utils::mesh::{KaboodlePeer, PeerMetadata};

pub async fn monitor_mesh() -> anyhow::Result<()> {
    println!(
        "Monitoring mesh ... {}",
        "(Press enter at any time to see a list of known peer latencies.)".blue()
    );
    let stdin_rx = spawn_stdin_reader();
    let mut mesh = super::peers::create_mesh_peer().await?;
    let mut discover_rx = mesh
        .discover_peers()
        .expect("Unable to get arrivals channel!");
    let mut depart_rx = mesh
        .discover_departures()
        .expect("Unable to get departures channel!");

    loop {
        while let Ok((addr, identity)) = discover_rx.try_recv() {
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
        while let Ok(addr) = depart_rx.try_recv() {
            println!("❌ {} {}", "DEPARTED:".red(), addr);
        }
        if stdin_rx.try_recv().is_ok() {
            let latencies = mesh.peer_latencies().await;
            if !latencies.is_empty() {
                println!("{}", "LATENCIES:".blue());
            }
            for (peer, latency) in latencies.into_iter() {
                let latency = format!("{:.2} ms", latency.as_micros() as f64 / 1000.0);
                println!(
                    "⏲️  {latency} to {} @ {}",
                    peer.instance_id(),
                    peer.address(),
                );
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
    // on ctrl-C, clean up?
}

fn spawn_stdin_reader() -> Receiver<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut character = [0];
        loop {
            while stdin().read(&mut character).is_err() {
                // loop until we actually read something
            }

            std::io::stdout().flush().unwrap();
            let _ = tx.send(());
        }
    });

    rx
}
