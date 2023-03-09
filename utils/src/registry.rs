use std::str::FromStr;

use crate::errors::ServalError;

/// Package registry information, used to download executables and construct the Manifest.
#[derive(Debug, PartialEq)]
pub struct Registry {
    pub namespace: String,
    pub baseurl_summary: &'static str,
    pub baseurl_download: &'static str,
}

impl FromStr for Registry {
    type Err = ();

    fn from_str(input: &str) -> Result<Registry, Self::Err> {
        if let "wapm" = input {
            Ok(Registry {
                namespace: String::from("io.wapm"),
                baseurl_summary: "https://wapm.io/{author}/{name}@{version}",
                baseurl_download: "https://registry-cdn.wapm.io/contents/{author}/{name}/{version}/target/wasm32-wasi/release/{name}.wasm",
        })
        } else {
            Err(())
        }
    }
}

/// Specification for a registry package
#[derive(Debug, PartialEq)]
pub struct RegistryPackageSpec {
    pub registry: Registry,
    pub author: String,
    pub package: String,
    pub version: String,
}

impl RegistryPackageSpec {

    /// Parses a registry package using a registry key and a package identifier
    pub fn parse(registry: &str, identifer: String) -> Result<RegistryPackageSpec, ServalError> {
        let registry_spec = Registry::from_str(registry).unwrap();
        // Split the string by "/". We expect the provided identifier to be one of the following:
        // - author/package/version
        // - author/package@version
        let pkg_author_spec: Vec<&str> = identifer.split('/').collect();
        match pkg_author_spec.len() {
            // we assume the provided format is author/package@version
            2 => {
                let author = pkg_author_spec[0].to_string();
                // The string after the / should contain package@version
                let pkg_version_spec: Vec<&str> = pkg_author_spec[1].split('@').collect();
                match pkg_version_spec.len() {
                    // we found a package and version
                    2 => {
                        let (name, version) = (
                            pkg_version_spec[0].to_string(),
                            pkg_version_spec[1].to_string(),
                        );
                        Ok(RegistryPackageSpec {
                            registry: registry_spec,
                            author,
                            package: name,
                            version,
                        })
                    }
                    _ => Err(ServalError::PackageRegistryManifestError(String::from(
                        "could not parse version.",
                    ))),
                }
            }
            // we assume the provided format is author/package/version
            3 => {
                let (author, name, version) = (
                    pkg_author_spec[0].to_string(),
                    pkg_author_spec[1].to_string(),
                    pkg_author_spec[2].to_string(),
                );
                Ok(RegistryPackageSpec {
                    registry: registry_spec,
                    author,
                    package: name,
                    version,
                })
            }
            // unclear format, we're erroring out
            _ => Err(ServalError::PackageRegistryManifestError(String::from(
                "could not parse package identifier.",
            ))),
        }
    }

    /// The fully-qualified task name of the package, composed of the registry
    /// namespace, author, package name and version.
    pub fn fqtn(self) -> String {
        format!(
            "{}.{}.{}@{}",
            self.registry.namespace, self.author, self.package, self.version
        )
    }
}
