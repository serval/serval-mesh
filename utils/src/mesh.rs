use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use bincode::{Decode, Encode};
use if_addrs::Interface;
use kaboodle::errors::KaboodleError;
use kaboodle::Kaboodle;
use serde::{Deserialize, Serialize};

use crate::errors::ServalError;

/// A little wrapper around kaboodle so we can hide the machinery of encoding and decoding.
/// the identity payload.
#[async_trait]
pub trait KaboodleMesh {
    type A: KaboodlePeer;

    /// Create a new entry for a Kaboodle peer network and add ourselves to the mesh.
    async fn start(&mut self) -> Result<(), KaboodleError>;
    /// Remove this peer from the mesh.
    async fn stop(&mut self) -> Result<(), KaboodleError>;
    /// Get a list of peers. It's the implementer's responsibility to decide if this is fresh or cached somehow.
    async fn peers(&self) -> Vec<Self::A>;
}

/// This type encodes the responsibilities of the resources we are meshing together.
pub trait KaboodlePeer {
    /// Create a new peer structure from the node identity payload plus an address.
    fn from_identity(address: IpAddr, encoded: Vec<u8>) -> Self;
    /// Create an identity payload from whatever internal information matters to your implementation.
    fn identity(&self) -> Vec<u8>;
    /// Get the address of this node.
    fn address(&self) -> IpAddr;
}

// End of tiny wrapper around Kaboodle.

/// These are the roles we allow peers to advertise on the mesh
#[derive(Debug, Clone, PartialEq, Eq, Decode, Encode, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServalRole {
    Scheduler,
    Runner,
    Storage,
    Observer,
}

impl std::fmt::Display for ServalRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServalRole::Runner => write!(f, "runner"),
            ServalRole::Scheduler => write!(f, "scheduler"),
            ServalRole::Storage => write!(f, "storage"),
            ServalRole::Observer => write!(f, "observer"),
        }
    }
}

impl FromStr for ServalRole {
    type Err = ServalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "runner" => Ok(ServalRole::Runner),
            "scheduler" => Ok(ServalRole::Scheduler),
            "storage" => Ok(ServalRole::Storage),
            "observer" => Ok(ServalRole::Observer),
            _ => Err(ServalError::InvalidRole(s.to_string())),
        }
    }
}

// An envelope that holds a version number. A little bit of future-proofing
// to allow agents with higher version numbers to decode payloads from older agents.
// Possibly over-thinking it.
#[derive(Debug, Clone, Decode, Encode)]
struct VersionEnvelope {
    version: u8,
    rest: Vec<u8>,
}

/// Represents a peer within the mesh. Generally speaking, this contains the data needed to identify
/// and communicate with a particular peer.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct PeerMetadata {
    address: IpAddr,
    inner: MetadataInner,
}

// The data we need to encode our identity as a serval peer. Done with an additional
// type to get the derive. There'll be another way to do this, I'm sure.
#[derive(Debug, Clone, Decode, Encode, Hash, Eq, PartialEq, Deserialize, Serialize)]
struct MetadataInner {
    instance_id: String,
    http_port: Option<u16>, // Observer-only mesh members will not be listening over HTTP at all
    roles: Vec<ServalRole>,
}

impl PeerMetadata {
    /// Create a new metadata node from useful information.
    pub fn new(
        instance_id: String,
        http_port: Option<u16>,
        roles: Vec<ServalRole>,
        address: IpAddr,
    ) -> Self {
        let inner = MetadataInner {
            instance_id,
            http_port,
            roles,
        };
        Self { address, inner }
    }

    /// Get the instance_id for this peer.
    pub fn instance_id(&self) -> &str {
        &self.inner.instance_id
    }

    /// Get the roles this peer has chosen to advertise.
    pub fn roles(&self) -> Vec<ServalRole> {
        self.inner.roles.clone()
    }

    /// Get the advertised http address of this peer.
    pub fn http_address(&self) -> Option<SocketAddr> {
        self.inner.http_port.map(|port| match self.address() {
            IpAddr::V4(ip) => SocketAddr::V4(SocketAddrV4::new(ip, port)),
            IpAddr::V6(ip) => SocketAddr::V6(SocketAddrV6::new(ip, port, 0, 0)),
        })
    }
}

impl KaboodlePeer for PeerMetadata {
    fn from_identity(address: IpAddr, encoded: Vec<u8>) -> Self {
        // TODO: this is actually fallible; when might it fail?
        let config = bincode::config::standard();
        let (envelope, _len): (VersionEnvelope, usize) =
            bincode::decode_from_slice(&encoded[..], config).unwrap();
        // In the future, switch on version in the envelope and decode into variants.
        let (inner, _len): (MetadataInner, usize) =
            bincode::decode_from_slice(&envelope.rest[..], config).unwrap();
        PeerMetadata { address, inner }
    }

    fn identity(&self) -> Vec<u8> {
        let config = bincode::config::standard();
        let rest: Vec<u8> = bincode::encode_to_vec(self.inner.clone(), config).unwrap_or_default();
        let envelope = VersionEnvelope { version: 1, rest };
        let identity: Vec<u8> = bincode::encode_to_vec(envelope, config).unwrap_or_default();
        identity
    }

    fn address(&self) -> IpAddr {
        self.address
    }
}

// End of peer implementation. Now we dive into the mesh itself.

#[derive(Debug)]
pub struct ServalMesh {
    kaboodle: Kaboodle,
    _metadata: PeerMetadata, // TODO: do I need this?
}

impl ServalMesh {
    /// Create a new node, with a kaboodle instance ready to run but not yet joined.
    pub async fn new(
        metadata: PeerMetadata,
        port: u16,
        interface: Option<Interface>,
    ) -> Result<Self, KaboodleError> {
        let identity = metadata.identity();
        let kaboodle = Kaboodle::new(port, interface, identity)?;
        Ok(Self {
            kaboodle,
            _metadata: metadata,
        })
    }

    /// Returns a map of all peers with known latencies.
    pub async fn peer_latencies(&self) -> HashMap<PeerMetadata, Duration> {
        self.kaboodle
            .peer_states()
            .await
            .into_iter()
            .filter_map(|(addr, peer_info)| {
                peer_info.latency.map(|latency| {
                    (
                        PeerMetadata::from_identity(addr.ip(), peer_info.identity.to_vec()),
                        latency,
                    )
                })
            })
            .collect::<HashMap<_, _>>()
    }

    /// Given a specific role, look for all peers that advertise the role.
    pub async fn peers_with_role(&self, role: &ServalRole) -> Vec<PeerMetadata> {
        let peers = self.peers().await;
        // A naive implementation, to understate the matter, but it gets us going.
        peers
            .into_iter()
            .filter(|xs| xs.roles().contains(role) && xs.http_address().is_some())
            .collect()
    }

    // Delegation would be nice.
    pub fn discover_peers(
        &mut self,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<(SocketAddr, axum::body::Bytes)>, KaboodleError>
    {
        self.kaboodle.discover_peers()
    }

    pub fn discover_departures(
        &mut self,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<SocketAddr>, KaboodleError> {
        self.kaboodle.discover_departures()
    }
}

#[async_trait]
impl KaboodleMesh for ServalMesh {
    type A = PeerMetadata;

    async fn start(&mut self) -> Result<(), KaboodleError> {
        self.kaboodle.start().await
    }

    async fn stop(&mut self) -> Result<(), KaboodleError> {
        self.kaboodle.stop().await
    }

    async fn peers(&self) -> Vec<Self::A> {
        let peers = self.kaboodle.peers().await;
        peers
            .into_iter()
            .map(|(addr, identity)| PeerMetadata::from_identity(addr.ip(), identity.to_vec()))
            .collect()
    }
}

/// Discover a single nearby node in the mesh, without the overhead of joining it.
pub async fn discover() -> Result<PeerMetadata, KaboodleError> {
    let (iface, port) = mesh_interface_and_port();
    let (address, identity) = Kaboodle::discover_mesh_member(port, Some(iface)).await?;
    Ok(PeerMetadata::from_identity(address.ip(), identity.to_vec()))
}

pub fn mesh_interface_and_port() -> (if_addrs::Interface, u16) {
    let mesh_port: u16 = std::env::var("MESH_PORT")
        .ok()
        .map(|port_str| port_str.parse().expect("Invalid value given for MESH_PORT"))
        .unwrap_or(8181);
    let mesh_interface = match std::env::var("MESH_INTERFACE") {
        Ok(v) => crate::networking::get_interface(&v)
            .expect("Failed to find interface matching MESH_INTERFACE value"),
        Err(_) => crate::networking::best_available_interface().expect("No available interfaces"),
    };
    log::info!(
        "connecting to the mesh on port {mesh_port} over {} ({})",
        mesh_interface.name,
        mesh_interface.ip()
    );
    (mesh_interface, mesh_port)
}
