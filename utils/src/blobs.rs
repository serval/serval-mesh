use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::{fs::File, io::AsyncWriteExt};
use tokio_util::io::ReaderStream;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;

use std::path::PathBuf;

use crate::errors::ServalError;

fn is_valid_address(addr: &str) -> bool {
    // this does not seem worth adding a regexp crate for
    let valid_chars = String::from("0123456789abcdef");
    addr.len() == 64
        && addr
            .to_lowercase()
            .chars()
            .all(|ch| valid_chars.contains(ch))
}

#[derive(Clone, Debug, Serialize)]
pub struct BlobStore {
    location: PathBuf,
}

impl BlobStore {
    /// Create a new blob store, passing in a path to a writeable directory
    pub fn new(location: PathBuf) -> Self {
        Self { location }
    }

    /// Given a content address, return a read stream for the object stored there.
    /// Responds with an error if no object is found or if the address is invalid.
    pub async fn get_stream(&self, address: &str) -> Result<ReaderStream<File>, ServalError> {
        if !is_valid_address(address) {
            return Err(ServalError::BlobAddressInvalid(address.to_string()));
        }

        let filename = self.location.join(address);
        if !filename.exists() {
            return Err(ServalError::BlobAddressNotFound(address.to_string()));
        }

        let file = tokio::fs::File::open(filename).await?;
        let stream = ReaderStream::new(file);
        Ok(stream)
    }

    pub async fn get_bytes(&self, address: &str) -> Result<Vec<u8>, ServalError> {
        let stream = self.get_stream(address).await?;
        let mut reader = StreamReader::new(stream);
        let mut binary = Vec::new();
        let _count = reader.read_to_end(&mut binary).await;
        Ok(binary)
    }

    /// Store an object in our blob store.
    pub async fn store(&self, body: &[u8]) -> Result<(bool, String), ServalError> {
        let mut hasher = Sha256::new();
        hasher.update(body);
        let blob_addr = hex::encode(hasher.finalize());
        let filename = self.location.join(&blob_addr);
        if filename.exists() {
            return Ok((false, blob_addr));
        }

        let mut file = tokio::fs::File::create(filename).await?;
        file.write_all(body).await?;
        Ok((true, blob_addr))
    }
}

#[cfg(test)]
mod tests {
    use crate::blobs::is_valid_address;

    #[test]
    fn valid_and_invalid_addresses() {
        assert!(is_valid_address(
            "25449ceed05926fc81700a3e8b66f66291ba9ed67dea9af88f83647ddb40e2f3"
        ));
        assert!(!is_valid_address("deadbeef"));
        assert!(!is_valid_address("invalid characters"));
    }
}
