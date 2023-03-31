// Finding a peer at most once, so we can build urls.

use anyhow::{anyhow, Result};
use async_once_cell::OnceCell;
use tokio::time::sleep;
use utils::mesh::{KaboodleMesh, PeerMetadata, ServalMesh, ServalRole};

use std::{net::SocketAddr, time::Duration};

static SERVAL_NODE_ADDR: OnceCell<SocketAddr> = async_once_cell::OnceCell::new();

async fn base_url() -> SocketAddr {
    *SERVAL_NODE_ADDR
        .get_or_init(async {
            maybe_find_peer(None, "SERVAL_NODE_URL")
                .await
                .expect("unable to find any mesh peers!")
        })
        .await
}

// Convenience function to build urls repeatably.
pub async fn build_url(path: String, version: Option<&str>) -> String {
    let baseurl = base_url().await;
    if let Some(v) = version {
        format!("http://{baseurl}/v{v}/{path}")
    } else {
        format!("http://{baseurl}/{path}")
    }
}

pub async fn create_mesh_peer() -> Result<ServalMesh> {
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let (port, interface) = utils::mesh::mesh_interface_and_port();

    let metadata = PeerMetadata::new(
        format!("client@{host}"),
        None,
        vec![ServalRole::Client],
        None,
    );
    let mut mesh = ServalMesh::new(metadata, interface, port).await?;
    mesh.start().await?;
    Ok(mesh)
}

async fn maybe_find_peer(role: Option<&ServalRole>, override_var: &str) -> Result<SocketAddr> {
    if let Ok(override_url) = std::env::var(override_var) {
        if let Ok(override_addr) = override_url.parse::<SocketAddr>() {
            return Ok(override_addr);
        }
    }

    log::info!("Looking for {role:?} node on the peer network...");
    let mut mesh = create_mesh_peer().await?;

    // There has to be a better way.
    sleep(Duration::from_secs(20)).await;

    let result = if let Some(role) = role {
        let candidates = mesh.peers_with_role(role).await;
        if let Some(target) = candidates.first() {
            // we know that peers_with_role filters out candidates without http addresses
            Ok(target.http_address().unwrap())
        } else {
            Err(anyhow!("Unable to locate a peer with the {role} role"))
        }
    } else {
        // any peer will do
        if let Some(target) = mesh.peers().await.first() {
            // we know that peers_with_role filters out candidates without http addresses
            Ok(target.http_address().unwrap())
        } else {
            Err(anyhow!("Unable to locate any peers on this mesh"))
        }
    };

    mesh.stop().await?;

    result
}
