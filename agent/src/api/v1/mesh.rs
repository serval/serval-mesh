use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Json,
};
use utils::mesh::{KaboodleMesh, PeerMetadata, ServalRole};

use crate::structures::*;

/// Mount all mesh-related introspection endpoints.
pub fn mount(router: ServalRouter) -> ServalRouter {
    router
        .route("/v1/mesh/peers/:role", get(filter_peers)) // TODO
        .route("/v1/mesh/peers", get(list_peers)) // TODO
}

/// List all known peers.
async fn list_peers(_state: State<AppState>) -> Json<Vec<PeerMetadata>> {
    let mesh = MESH.get().expect("Peer network not initialized!"); // yes, we crash in this case
    let peers = mesh.peers().await;
    Json(peers)
}

/// Filter known peers to only those that advertise the specific role.
async fn filter_peers(Path(role): Path<ServalRole>, _state: State<AppState>) -> impl IntoResponse {
    let mesh = MESH.get().expect("Peer network not initialized!"); // yes, we crash in this case
    let peers = mesh.peers_with_role(&role).await;
    Json(peers)
}
