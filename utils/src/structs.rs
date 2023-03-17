use std::{fmt::Display, path::PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{errors::ServalError, registry::PackageSpec};

/// The results of running a WASM executable.
#[derive(Debug)]
pub struct WasmResult {
    /// The status code returned by the execution; 0 for normal termination.
    pub code: i32,
    /// Whatever the WASM executable wrote to stdout.
    pub stdout: Vec<u8>,
    /// Whatever the WASM executable wrote to stderr.
    pub stderr: Vec<u8>,
}

/// WASM executable metadata, for human reasons.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// Short name of this WASM manifest. Lower-cased alphanumerics plus underscore.
    name: String,
    /// The namespace this WASM manifest belongs to.
    namespace: String,
    /// A semver-compatible version string. Semver not yet enforced.
    version: String,
    /// Path to a compiled WASM exectuable.
    binary: PathBuf,
    /// Human-readable description.
    description: String,
    /// Required extensions.
    required_extensions: Vec<String>, // TODO: this is a placeholder
}

impl Manifest {
    pub fn from_string(input: &str) -> Result<Self, ServalError> {
        let manifest: Manifest = toml::from_str(input)?;
        Ok(manifest)
    }

    pub fn from_file(path: &PathBuf) -> Result<Self, ServalError> {
        let buf = std::fs::read_to_string(path)?;
        let manifest: Manifest = toml::from_str(&buf)?;
        Ok(manifest)
    }

    pub fn from_packagespec(pkg_spec: &PackageSpec) -> Result<Self, ServalError> {
        let mut name = String::from(pkg_spec.name.clone());
        // If the module name differs from the package name, surface the module name
        // in the manifest to support installing multiple modules from the same package.
        if pkg_spec.name != pkg_spec.module {
            name = format!("{}.{}", name, pkg_spec.module);
        }
        let manifest = Manifest {
            name: name,
            namespace: pkg_spec.namespace(),
            version: pkg_spec.version.clone(),
            binary: pkg_spec.binary_path(),
            description: pkg_spec.profile_url(),
            required_extensions: Vec::new(),
        };
        Ok(manifest)
    }

    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get the fully-qualified-by-namespace name for this job type manifest.
    pub fn fq_name(&self) -> String {
        let name = self.name.to_ascii_lowercase().replace('-', "_");
        format!("{}.{name}", self.namespace)
    }

    /// Given a name but no manifest, build a key.
    pub fn make_manifest_key(name: &str) -> String {
        format!("{name}.manifest.toml")
    }

    /// Get the storage key for this manifest.
    pub fn manifest_key(&self) -> String {
        Manifest::make_manifest_key(&self.fq_name())
    }

    /// Given a name and a version but no manifest, build an executable key.
    pub fn make_executable_key(name: &str, version: &str) -> String {
        format!("{name}.{version}.wasm")
    }

    /// Get the key for the executable pointed to by this manifest.
    pub fn executable_key(&self) -> String {
        Manifest::make_executable_key(&self.fq_name(), &self.version)
    }
}

impl Display for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match toml::to_string(self) {
            Ok(v) => write!(f, "{v}"),
            Err(e) => write!(f, "{e:?}"),
        }
    }
}

/// Metadata about a specific job instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    /// The ID of this job.
    id: Uuid,
    /// Fully-qualified job manifest specification. E.g., "sh.serval.birdfeeder"
    manifest: Manifest,
    /// bytes for the wasm executable
    executable: Vec<u8>,
    /// Input data
    input: Vec<u8>,
    // TODO: might have version chosen to run here, plus run options; might also store the input
}

impl Job {
    pub fn new(manifest: Manifest, executable: Vec<u8>, input: Vec<u8>) -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            manifest,
            executable,
            input,
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn executable(&self) -> &Vec<u8> {
        &self.executable
    }

    pub fn input(&self) -> &Vec<u8> {
        &self.input
    }
}
