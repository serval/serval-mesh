use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::mesh::PeerMetadata;

/// A MeshMember is effectively a limited subset of information from a PeerMetadata instance. Unlike
/// PeerMetadata, MeshMember is publicly visible via the HTTP API. The intention is for it to only
/// contain enoug information to know how to talk to a node and who that node is.
#[derive(Deserialize, Serialize)]
pub struct MeshMember {
    pub http_address: Option<SocketAddr>,
    pub instance_id: String,
}

impl From<PeerMetadata> for MeshMember {
    fn from(peer_metadata: PeerMetadata) -> Self {
        MeshMember {
            http_address: peer_metadata.http_address(),
            instance_id: peer_metadata.instance_id().to_string(),
        }
    }
}
