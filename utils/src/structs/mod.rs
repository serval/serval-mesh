use std::{fmt::Display, fs, path::PathBuf, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::errors::ServalError;

/// The results of running a Wasm executable.
#[derive(Debug)]
pub struct WasmResult {
    /// The status code returned by the execution; 0 for normal termination.
    pub code: i32,
    /// Whatever the Wasm executable wrote to stdout.
    pub stdout: Vec<u8>,
    /// Whatever the Wasm executable wrote to stderr.
    pub stderr: Vec<u8>,
}

/// Wasm executable metadata, for human reasons.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// Short name of this Wasm manifest. Lower-cased alphanumerics plus underscore.
    name: String,
    /// The namespace this Wasm manifest belongs to.
    namespace: String,
    /// A semver-compatible version string. Semver not yet enforced.
    version: String,
    /// Path to a compiled Wasm exectuable.
    binary: PathBuf,
    /// Human-readable description.
    description: String,
    /// Required extensions.
    #[serde(default)]
    required_extensions: Vec<String>,
    // TODO: this is a placeholder and requires more thought; the WASM binary itself contains the
    // info we need to enumerate the required extensions it is looking for. However, for job
    // routing, it would be great for this information to be available without having the binary
    // on-hand locally. The right answer here is probably to make this field be optional in manifest
    // files, and to derive the value automatically at binary/manifest storage time.
    /// Required permissions; it is up to the agent to ensure that the submitter of this job is
    /// actually authorized to run a job with said permissions.
    #[serde(default)]
    required_permissions: Vec<Permission>,
}

impl Manifest {
    pub fn new(path: &PathBuf) -> Manifest {
        Manifest {
            name: path.file_stem().unwrap().to_string_lossy().to_string(),
            namespace: String::from(""),
            binary: path.to_owned(),
            version: String::from("0.0.0"),
            description: String::from(""),
            required_extensions: vec![],
            required_permissions: vec![],
        }
    }

    pub fn from_string(input: &str) -> Result<Self, ServalError> {
        let manifest: Manifest = toml::from_str(input)?;
        if manifest.binary.is_relative() {
            return Err(ServalError::RelativeBinaryPathInManifestError);
        }
        Ok(manifest)
    }

    pub fn from_file(path: &PathBuf) -> Result<Self, ServalError> {
        let buf = std::fs::read_to_string(path)?;
        let mut manifest = toml::from_str::<Manifest>(&buf)?;
        if manifest.binary.is_relative() {
            // If the binary file actually exists, replace its relative path with absolute path. (If
            // it doesn't exist, well, that's a problem for another piece of code somewhere.)
            let path = path.parent().unwrap().join(&manifest.binary);
            if path.exists() {
                manifest.binary = fs::canonicalize(path)?;
            }
        }
        Ok(manifest)
    }

    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    /// Get the list of permissions that this manifest is requesting. Note that this list needs to
    /// be validated elsewhere to ensure that the running user is authorized to assign said
    /// permissions.
    pub fn required_permissions(&self) -> &Vec<Permission> {
        &self.required_permissions
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Permission {
    ProcRead,
    AllExtensions,
    Extension(String),
    AllHttpHosts,
    HttpHost(String),
}

impl Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Permission::ProcRead => String::from("proc:read:*"),
            Permission::AllExtensions => String::from("extension:*"),
            Permission::Extension(name) => format!("extension:{name}"),
            Permission::AllHttpHosts => String::from("http:*"),
            Permission::HttpHost(host) => format!("http:{host}"),
        };
        let _ = write!(f, "{}", str);
        Ok(())
    }
}

impl FromStr for Permission {
    type Err = ();

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        match str {
            "extension:*" => Ok(Permission::AllExtensions),
            "http:*" => Ok(Permission::AllHttpHosts),
            "proc:read:*" => Ok(Permission::ProcRead),
            str => {
                if str.starts_with("extension:") {
                    if let Some((_, ext_name)) = str.split_once(':') {
                        return Ok(Permission::Extension(ext_name.to_string()));
                    }
                } else if str.starts_with("http:") {
                    if let Some((_, host)) = str.split_once(':') {
                        return Ok(Permission::HttpHost(host.to_string()));
                    }
                }

                Err(())
            }
        }
    }
}

impl<'de> Deserialize<'de> for Permission {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw_perm = String::deserialize(deserializer)?;
        Permission::from_str(raw_perm.as_str())
            .map_err(|_| serde::de::Error::custom(format!("Invalid permission: {raw_perm}")))
    }
}

impl Serialize for Permission {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
