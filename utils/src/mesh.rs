use async_trait::async_trait;
use bincode::{Decode, Encode};
use if_addrs::Interface;
use kaboodle::{errors::KaboodleError, Kaboodle};
use strum::{Display, EnumString};

use std::net::SocketAddr;

/// A little wrapper around kaboodle so we can hide the machinery of encoding and decoding.
/// the identity payload.
#[async_trait]
pub trait KaboodleMesh {
    type A: KaboodlePeer;

    /// Create a new entry for a Kaboodle peer network and add ourselves to the mesh.
    async fn start(&mut self) -> Result<(), KaboodleError>;
    /// Remove this peer from the mesh.
    async fn stop(&mut self) -> Result<(), KaboodleError>;
    /// Get the fingerprint of the current state of the mesh.
    async fn fingerprint(&self) -> u32;
    /// Get a list of peers. It's the implementer's responsibility to decide if this is fresh or cached somehow.
    async fn peers(&self) -> Vec<Self::A>;
}

/// This type encodes the responsibilities of the resources we are meshing together.
pub trait KaboodlePeer {
    /// Create a new peer structure from the node identity payload plus an address.
    fn from_identity(address: SocketAddr, encoded: Vec<u8>) -> Self;
    /// Create an identity payload from whatever internal information matters to your implementation.
    fn identity(&self) -> Vec<u8>;
    /// Get the address of this node.
    fn address(&self) -> Option<SocketAddr>;
}

// End of tiny wrapper around Kaboodle.

/// These are the roles we allow peers to advertise on the mesh
#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Decode, Encode)]
#[strum(serialize_all = "lowercase")]
pub enum ServalRole {
    Runner,
    Storage,
    Client,
}

// An envelope that holds a version number. A little bit of future-proofing
// to allow agents with higher version numbers to decode payloads from older agents.
// Possibly over-thinking it.
#[derive(Debug, Clone, Decode, Encode)]
struct VersionEnvelope {
    version: u8,
    rest: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PeerMetadata {
    address: Option<SocketAddr>,
    inner: MetadataInner,
}

// The ddta we need to encode our identity as a serval peer. Done with an additional
// type to get the derive. There'll be another way to do this, I'm sure.
#[derive(Debug, Clone, Decode, Encode)]
struct MetadataInner {
    instance_id: String,
    http_address: Option<SocketAddr>, // this is an option because CLIs don't have one
    roles: Vec<ServalRole>,
}

impl PeerMetadata {
    /// Create a new metadata node from useful information.
    pub fn new(
        instance_id: String,
        http_address: Option<SocketAddr>,
        roles: Vec<ServalRole>,
        address: Option<SocketAddr>,
    ) -> Self {
        let inner = MetadataInner {
            instance_id,
            http_address,
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
        self.inner.http_address.map(|http_address| {
            if http_address.ip().is_unspecified() {
                let mut addr = self.address.unwrap().clone();
                addr.set_port(http_address.port());
                addr
            } else {
                http_address
            }
        })
    }
}

impl KaboodlePeer for PeerMetadata {
    fn from_identity(address: SocketAddr, encoded: Vec<u8>) -> Self {
        // TODO: this is actually fallible; when might it fail?
        let config = bincode::config::standard();
        let (envelope, _len): (VersionEnvelope, usize) =
            bincode::decode_from_slice(&encoded[..], config).unwrap();
        // In the future, switch on version in the envelope and decode into variants.
        let (inner, _len): (MetadataInner, usize) =
            bincode::decode_from_slice(&envelope.rest[..], config).unwrap();
        PeerMetadata {
            address: Some(address),
            inner,
        }
    }

    fn identity(&self) -> Vec<u8> {
        let config = bincode::config::standard();
        let rest: Vec<u8> = bincode::encode_to_vec(self.inner.clone(), config).unwrap_or_default();
        let envelope = VersionEnvelope { version: 1, rest };
        let identity: Vec<u8> = bincode::encode_to_vec(envelope, config).unwrap_or_default();
        identity
    }

    fn address(&self) -> Option<SocketAddr> {
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

    /// Given a specific role, look for all peers that advertise the role.
    pub async fn peers_with_role(&self, role: &ServalRole) -> Vec<PeerMetadata> {
        let peers = self.peers().await;
        // A naive implementation, to understate the matter, but it gets us going.
        peers
            .into_iter()
            .filter(|xs| xs.roles().contains(role) && xs.http_address().is_some())
            .collect()
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

    async fn fingerprint(&self) -> u32 {
        self.kaboodle.fingerprint().await
    }

    async fn peers(&self) -> Vec<Self::A> {
        let peers = self.kaboodle.peers().await;
        peers
            .into_iter()
            .map(|(addr, identity)| PeerMetadata::from_identity(addr, identity.to_vec()))
            .collect()
    }
}
