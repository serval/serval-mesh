//! Registry Module
//!
//! Serval supports downloading WebAssembly executables from package registries.
//! Downloaded packages are automatically stored to the Serval Mesh and can be
//! run just like any manually stored WebAssembly executable.

use std::str::FromStr;

use regex::Regex;

use crate::errors::ServalError;

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

    fn download_url(&self, pkg: &PackageSpec) -> String {
        match self {
            PackageRegistry::Wapm => {
                format!("https://registry-cdn.wapm.io/contents/{}/{}/{}/target/wasm32-wasi/release/{}.wasm", pkg.author, pkg.name, pkg.version, pkg.name)
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
#[derive(Debug, PartialEq)]
pub struct PackageSpec {
    pub registry: PackageRegistry,
    pub author: String,
    pub name: String,
    pub version: String,
}

impl PackageSpec {
    // other useful functions here

    pub fn profile_url(&self) -> String {
        self.registry.profile_url(self)
    }

    pub fn download_url(&self) -> String {
        self.registry.download_url(self)
    }
}

impl TryFrom<std::string::String> for PackageSpec {
    type Error = ServalError;
    // put your parsing code here
    fn try_from(value: std::string::String) -> Result<Self, Self::Error> {
        // This regex matches a package specification. It currently supports any of the following variants:
        // - http(s)://registry.tld/author/package@version
        // - registry.tld/author/package@version
        // - registry.tld/author/package    => defaults to latest version
        // - author/package@version         => defaults to wapm.io
        // - author/package                 => defaults to wapm.io and latest version
        // TODO The wapm.io default should be made configurable
        let re = Regex::new(
            r"(?x)
            (?:[a-z]+/{2})?             # the protocol (optional, non-capturing)
            (([a-z0-9.]+)(?:/))?        # $1 (optional) package registry domain incl. trailing slash
                                        # $2 (optional) package registry domain w/o trailing slash
            ([a-zA-Z0-9-]+)             # $3 package author
            (?:/)                       # slash (non-capturing)
            ([a-zA-Z0-9-]+)             # $4 package name
            ((?:@)([a-zA-Z0-9.-]+))?    # $5 (optional) package version incl. @ prefix
                                        # $6 (optional) package version w/o @ prefix
            ",
        ).unwrap();
        let cap = re.captures(&value).unwrap();
        let (pkg_reg, pkg_auth, pkg_name, pkg_version) = (
            cap.get(2).map_or(PackageRegistry::Wapm, |m| {
                PackageRegistry::from_str(m.as_str()).unwrap()
            }),
            String::from(cap.get(3).map(|m| m.as_str()).unwrap()),
            String::from(cap.get(4).map(|m| m.as_str()).unwrap()),
            String::from(cap.get(6).map_or("latest", |m| m.as_str())),
        );
        Ok(PackageSpec {
            author: pkg_auth,
            name: pkg_name,
            version: pkg_version,
            registry: pkg_reg,
        })
    }
}
