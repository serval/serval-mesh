use async_trait::async_trait;
use bincode::{Decode, Encode};
use if_addrs::Interface;
use kaboodle::{errors::KaboodleError, Kaboodle};

use std::net::SocketAddr;

/// This type encodes the responsibilities of the resources we are meshing together.
pub trait KaboodlePeer {
    fn new(address: SocketAddr, identity: Vec<u8>) -> Self;
    fn identity(&self) -> Vec<u8>;
    fn name(&self) -> &str;
    fn roles(&self) -> Vec<String>;
}

/// A little wrapper around kaboodle so we can hide the machinery of encoding and decoding.
/// the identity payload.
#[async_trait]
pub trait KaboodleMesh {
    type A;

    /// Create a new entry for a Kaboodle peer network and add ourselves to the mesh.
    async fn new(
        name: &str,
        roles: Vec<String>,
        port: u16,
        interface: Option<Interface>,
    ) -> Result<Box<Self>, KaboodleError>;
    /// Get a list of peers. It's the implementer's responsibility to decide if this is fresh or cached somehow.
    async fn peers(&self) -> Vec<Self::A>;
    /// Given a specific role, look for a peer that advertises the role.
    async fn find_role(&self, role: &str) -> Result<Self::A, KaboodleError>;
}

#[derive(Debug, Clone, Decode, Encode)]
struct VersionEnvelope {
    version: u8,
    rest: Vec<u8>
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct ServalPeer {
    name: String,
    roles: Vec<String>,
}

impl KaboodlePeer for ServalPeer {
    fn new(address: SocketAddr, encoded: Vec<u8>) -> Self {
        // TODO: this is actually fallible
        let config = bincode::config::standard();
        let (envelope, _len): (VersionEnvelope, usize) = bincode::decode_from_slice(&encoded[..], config).unwrap();
        // In the future, switch on version in the envelope and decode into variants.
        let (peer, _len): (Self, usize) = bincode::decode_from_slice(&envelope.rest[..], config).unwrap();
        peer
    }

    fn identity(&self) -> Vec<u8> {
        // TODO: this is actually fallible
        let config = bincode::config::standard();
        let rest: Vec<u8> = bincode::encode_to_vec(self, config).unwrap();
        let envelope = VersionEnvelope { version: 1, rest };
        let identity: Vec<u8> = bincode::encode_to_vec(envelope, config).unwrap();
        identity
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn roles(&self) -> Vec<String> {
        self.roles.clone()
    }
}

#[derive(Debug)]
pub struct AgentMesh {
    kaboodle: Kaboodle,
    agent: ServalPeer,
}

#[async_trait]
impl KaboodleMesh for AgentMesh {
    type A = ServalPeer;

    async fn new(
        name: &str,
        roles: Vec<String>,
        port: u16,
        interface: Option<Interface>,
    ) -> Result<Box<Self>, KaboodleError> {
        let agent = ServalPeer {
            name: name.to_string(),
            roles,
        };
        let identity = agent.identity();
        let mut kaboodle = Kaboodle::new(port, interface, identity).unwrap();
        kaboodle.start().await?;
        Ok(Box::new(Self { kaboodle, agent }))
    }

    async fn peers(&self) -> Vec<Self::A> {
        let peers = self.kaboodle.peers().await; // this isn't fallible? really? okay
        peers.into_iter().map(|(addr, identity)| {
            ServalPeer::new(addr, identity.to_vec())
        }).collect()
    }

    async fn find_role(&self, role: &str) -> Result<Self::A, KaboodleError> {
        let peers = self.peers().await;
        // find somebody who matches our role
        todo!();
    }
}
