//! Registry Module
//!
//! Serval supports downloading WebAssembly executables from package registries.
//! Downloaded packages are automatically stored to the Serval Mesh and can be
//! run just like any manually stored WebAssembly executable.

use std::{fs::File, io::Write, path::PathBuf, str::FromStr, time::Duration};

use regex::Regex;
use reqwest::{blocking::Client, StatusCode};
use sha256::digest;

use crate::{errors::ServalError, structs::Manifest};

/// Package registry information, used to download executables and construct the Manifest.
#[derive(Debug, PartialEq, Clone)]
pub enum PackageRegistry {
    Wapm,
    Warg,
}

impl FromStr for PackageRegistry {
    type Err = ServalError;

    fn from_str(input: &str) -> Result<PackageRegistry, ServalError> {
        match input {
            "wapm.io" => Ok(PackageRegistry::Wapm),
            "warg" => Ok(PackageRegistry::Warg),
            _ => Err(ServalError::PackageRegistryUnknownError(input.to_string())),
        }
    }
}

impl PackageRegistry {
    pub fn namespace(&self) -> &str {
        match self {
            PackageRegistry::Wapm => "io.wapm",
            PackageRegistry::Warg => "io.warg",
        }
    }

    pub fn domain(&self) -> &str {
        match self {
            PackageRegistry::Wapm => "wapm.io",
            PackageRegistry::Warg => "warg.io",
        }
    }

    fn profile_url(&self, pkg: &PackageSpec) -> String {
        match self {
            PackageRegistry::Wapm => {
                format!(
                    "https://wapm.io/{}/{}@{}",
                    pkg.author, pkg.name, pkg.version
                )
            }
            PackageRegistry::Warg => todo!(),
        }
    }

    fn fq_name(&self, pkg: &PackageSpec) -> String {
        match self {
            PackageRegistry::Wapm => {
                format!(
                    "{}.{}.{}.{}@{}",
                    self.namespace(),
                    pkg.author,
                    pkg.name,
                    pkg.module,
                    pkg.version,
                )
            }
            PackageRegistry::Warg => todo!(),
        }
    }

    fn download_urls(&self, pkg: &PackageSpec) -> Vec<String> {
        match self {
            PackageRegistry::Wapm => {
                vec![
                    // For some very stupid reason, wasm binaries can sit in multiple locations. Hopefully this is the full list:
                    format!("https://registry-cdn.wapm.io/contents/{}/{}/{}/{}.wasm", pkg.author, pkg.name, pkg.version, pkg.module),
                    format!("https://registry-cdn.wapm.io/contents/{}/{}/{}/target/wasm32-wasi/release/{}.wasm", pkg.author, pkg.name, pkg.version, pkg.module)
                ]
            }
            PackageRegistry::Warg => todo!(),
        }
    }
    // even cooler....
    //fn download(&self, pkg: &PackageSpec) -> Result<Bytes, ServalError> {
    //    // do the work of downloading from this kind of registry
    //}
}

/// Specification for a registry package
#[derive(Debug, Clone, PartialEq)]
pub struct PackageSpec {
    pub registry: PackageRegistry,
    pub author: String,
    pub name: String,
    pub version: String,
    pub module: String,
}

impl PackageSpec {
    pub fn profile_url(&self) -> String {
        self.registry.profile_url(self)
    }

    pub fn download_urls(&self) -> Vec<String> {
        self.registry.download_urls(self)
    }

    pub fn fq_name(&self) -> String {
        self.registry.fq_name(self)
    }

    pub fn fq_digest(&self) -> String {
        digest(self.fq_name())
    }

    pub fn namespace(&self) -> String {
        format!("{}.{}", self.registry.namespace(), self.author)
    }

    pub fn binary_path(&self) -> PathBuf {
        PathBuf::from(format!("/tmp/{}.wasm", self.fq_digest()))
    }

    pub fn manifest_path(&self) -> PathBuf {
        PathBuf::from(format!("/tmp/{}.toml", self.fq_digest()))
    }

    pub fn is_binary_cached(&self) -> bool {
        self.binary_path().exists()
    }
}

/// Converts an identifier string to a `PackageSpec`
impl TryFrom<std::string::String> for PackageSpec {
    type Error = ServalError;
    /**
    This function matches a package specification string.
    It supports a number of variants:

    Full URL to package in a supported registry:
    ```
    # use utils::registry::PackageSpec;
    let pkg_spec = PackageSpec::try_from(String::from("https://wapm.io/author/serval@version")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "serval".to_string(),
    #     version: "version".to_string(),
    #     module: "serval".to_string(),
    # });
    ```

    Full URL to package in a supported registry, defaulting to latest version:
    ```
    # use utils::registry::PackageSpec;
    let pkg_spec = PackageSpec::try_from(String::from("https://wapm.io/author/tiger")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "tiger".to_string(),
    #     version: "latest".to_string(),
    #     module: "tiger".to_string(),
    # });
    ```

    When providing a URL, the protocol is optional. This is also valid:
    ```
    # use utils::registry::PackageSpec;
    let pkg_spec = PackageSpec::try_from(String::from("wapm.io/author/lion@version")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "lion".to_string(),
    #     version: "version".to_string(),
    #     module: "lion".to_string(),
    # });
    # let pkg_spec = PackageSpec::try_from(String::from("wapm.io/author/cheetah")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "cheetah".to_string(),
    #     version: "latest".to_string(),
    #     module: "cheetah".to_string(),
    # });
    ```

    When providing a simple author/package-style identifier, the default package
    manager (currently [wapm.io](https://wapm.io) -- this will be made configurable) is used.
    ```
    # use utils::registry::PackageSpec;
    // provide specific version:
    let pkg_spec = PackageSpec::try_from(String::from("author/panther@version")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "panther".to_string(),
    #     version: "version".to_string(),
    #     module: "panther".to_string(),
    # });
    // default to latest version:
    let pkg_spec = PackageSpec::try_from(String::from("author/leopard")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "leopard".to_string(),
    #     version: "latest".to_string(),
    #     module: "leopard".to_string(),
    # });
    ```

    In some cases, the actual Wasm module contained in a package has a different name than the
    package, or a package may contain more than one module.
    The package identifier defaults to a module name identical to the package name -- if a
    different module should be used, it can be provided by appending it with a dot:
    ```
    # use utils::registry::PackageSpec;
    // provide specific version and module name:
    let pkg_spec = PackageSpec::try_from(String::from("author/felis.catus@version")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "felis".to_string(),
    #     version: "version".to_string(),
    #     module: "catus".to_string(),
    # });
    // again, a missing version defaults to the latest version:
    let pkg_spec = PackageSpec::try_from(String::from("author/felis.lybica")).unwrap();
    # assert_eq!(pkg_spec, utils::registry::PackageSpec {
    #     registry: utils::registry::PackageRegistry::Wapm,
    #     author: "author".to_string(),
    #     name: "felis".to_string(),
    #     version: "latest".to_string(),
    #     module: "lybica".to_string(),
    # });
    ```
    */
    // TODO: The wapm.io package manager is currently the default package manager; this should be made configurable.
    fn try_from(value: std::string::String) -> Result<Self, Self::Error> {
        let re = Regex::new(
            r"(?x)
            (?:[a-z]+/{2})?             # the protocol (optional, non-capturing)
            (([a-z0-9.]+)(?:/))?        # $1 package registry domain incl. trailing slash (optional, not used) 
                                        # $2 package registry domain w/o trailing slash (optional)
            ([a-zA-Z0-9-]+)             # $3 package author
            (?:/)                       # slash (non-capturing)
            ([a-zA-Z0-9-]+)             # $4 package name
            (?:(?:\.)([a-zA-Z0-9]+))?   # $5 module name (optional)
            (?:(?:@)([a-zA-Z0-9.-]+))?  # $6 package version (optional)
            ",
        )
        .unwrap();
        let cap = re.captures(&value).unwrap();
        // We attempt to extract the following capture groups:
        // - the package registry domain without trailing slash ($2)
        // - the package author ($3)
        // - the package name ($4)
        // - the package version ($6)
        let (pkg_reg, pkg_auth, pkg_name, pkg_version) = (
            cap.get(2).map_or(PackageRegistry::Wapm, |m| {
                PackageRegistry::from_str(m.as_str()).unwrap()
            }),
            String::from(cap.get(3).map(|m| m.as_str()).unwrap()),
            String::from(cap.get(4).map(|m| m.as_str()).unwrap()),
            String::from(cap.get(6).map_or("latest", |m| m.as_str())),
        );
        // Collecting the module name ($5) separately as it needs to default to the package name
        // if not provided.
        let mod_name = cap
            .get(5)
            .map_or(pkg_name.clone(), |m| m.as_str().to_owned());
        Ok(PackageSpec {
            author: pkg_auth,
            name: pkg_name,
            version: pkg_version,
            registry: pkg_reg,
            module: mod_name,
        })
    }
}

pub fn download_module(pkg_spec: &PackageSpec) -> Result<StatusCode, ServalError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(360))
        .build()
        .unwrap();
    let mut last_status: StatusCode = StatusCode::IM_A_TEAPOT;
    for url in pkg_spec.download_urls() {
        let response = client.get(url).send();
        match response {
            Ok(r) => {
                // println!("Ok: {:#?}", r);
                let status = r.status();
                if r.status().is_success() {
                    let mut f = File::create(pkg_spec.binary_path())?;
                    f.write_all(&r.bytes().unwrap())?;
                    return Ok(status);
                } else {
                    last_status = status;
                }
            }
            _ => {
                return Err(ServalError::PackageRegistryDownloadError(
                    "something went horribly wrong".to_string(),
                ))
            }
        };
    }
    Ok(last_status)
}

pub fn gen_manifest(pkg_spec: &PackageSpec) -> Result<PathBuf, ServalError> {
    let manifest = Manifest::from_packagespec(pkg_spec)?;
    let mut f = File::create(pkg_spec.manifest_path())?;
    f.write_all(toml::to_string(&manifest).unwrap().as_bytes())?;
    Ok(pkg_spec.manifest_path())
}
