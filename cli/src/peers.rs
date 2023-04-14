// Finding a peer at most once, so we can build urls.

use std::net::SocketAddr;

use anyhow::Result;
use async_once_cell::OnceCell;
use serval_client::ServalApiClient;
use utils::mesh::{KaboodleMesh, PeerMetadata, ServalMesh, ServalRole};

static SERVAL_NODE_ADDR: OnceCell<SocketAddr> = async_once_cell::OnceCell::new();

async fn peer_http_addr() -> SocketAddr {
    *SERVAL_NODE_ADDR
        .get_or_init(async {
            maybe_find_peer("SERVAL_NODE_URL")
                .await
                .expect("unable to find any mesh peers!")
        })
        .await
}

pub async fn api_client() -> ServalApiClient {
    let addr = peer_http_addr().await;

    ServalApiClient::new_with_version(1, addr.to_string())
}

async fn discover_peer() -> Result<PeerMetadata> {
    let peer = utils::mesh::discover().await?;
    Ok(peer)
}

pub async fn create_mesh_peer() -> Result<ServalMesh> {
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let (interface, port) = utils::mesh::mesh_interface_and_port();

    let http_port = None;
    let metadata = PeerMetadata::new(
        format!("observer@{host}"), // todo: should this just be a UUID like it is for everyone else?
        http_port,
        vec![ServalRole::Observer],
        interface.ip(),
    );
    let mut mesh = ServalMesh::new(metadata, port, Some(interface)).await?;
    mesh.start().await?;
    Ok(mesh)
}

async fn maybe_find_peer(override_var: &str) -> Result<SocketAddr> {
    if let Some(override_addr) = std::env::var(override_var)
        .ok()
        .and_then(|override_url| override_url.parse::<SocketAddr>().ok())
    {
        return Ok(override_addr);
    }

    log::info!("Looking for any node on the peer network...");
    loop {
        let peer = discover_peer().await?; // todo: perhaps discover_peer() should not return Observers?
        if let Some(addr) = peer.http_address() {
            return Ok(addr);
        }
    }
}
