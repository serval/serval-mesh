use async_trait::async_trait;
use bincode::{Decode, Encode};
use if_addrs::Interface;
use kaboodle::{errors::KaboodleError, Kaboodle};

use std::net::SocketAddr;

/// A little wrapper around kaboodle so we can hide the machinery of encoding and decoding.
/// the identity payload.
#[async_trait]
pub trait KaboodleMesh {
    type A;

    /// Create a new entry for a Kaboodle peer network and add ourselves to the mesh.
    async fn start(&mut self) -> Result<(), KaboodleError>;
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
    roles: Vec<String>,
}

impl PeerMetadata {
    /// Create a new metadata node from useful information.
    pub fn new(instance_id: String, roles: Vec<String>, address: Option<SocketAddr>) -> Self {
        let inner = MetadataInner { instance_id, roles };
        Self { address, inner }
    }

    /// Get the instance_id for this peer.
    pub fn instance_id(&self) -> &str {
        &self.inner.instance_id
    }

    /// Get the roles this peer has chosen to advertise.
    pub fn roles(&self) -> Vec<String> {
        self.inner.roles.clone()
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
        // TODO: this is actually fallible; when might it fail?
        let config = bincode::config::standard();
        let rest: Vec<u8> = bincode::encode_to_vec(self.inner.clone(), config).unwrap();
        let envelope = VersionEnvelope { version: 1, rest };
        let identity: Vec<u8> = bincode::encode_to_vec(envelope, config).unwrap();
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
    metadata: PeerMetadata,  // TODO: do I need this?
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
        Ok(Self { kaboodle, metadata })
    }

    /// Given a specific role, look for a peer that advertises the role.
    pub async fn find_role(&self, role: &String) -> Option<PeerMetadata> {
        let peers = self.peers().await;
        // A naive implementation, to understate the matter, but it gets us going.
        peers.into_iter().find(|xs| xs.roles().contains(role))
    }
}

#[async_trait]
impl KaboodleMesh for ServalMesh {
    type A = PeerMetadata;

    async fn start(&mut self) -> Result<(), KaboodleError> {
        self.kaboodle.start().await
    }

    async fn peers(&self) -> Vec<Self::A> {
        // No caching or smarts at all in the initial implementation.
        let peers = self.kaboodle.peers().await; // this isn't fallible? really? okay
        peers
            .into_iter()
            .map(|(addr, identity)| PeerMetadata::from_identity(addr, identity.to_vec()))
            .collect()
    }
}
