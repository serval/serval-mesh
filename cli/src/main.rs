#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]
/// Pounce is a CLI tool that interacts with a running serval agent daemon via
/// its HTTP API. It discovers running agents via mDNS advertisement.
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use humansize::{format_size, BINARY};
use owo_colors::OwoColorize;
use prettytable::{row, Table};
use tokio::runtime::Runtime;
use utils::registry::{download_module, gen_manifest, PackageRegistry, PackageSpec};
use utils::structs::Manifest;
use uuid::Uuid;

use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use utils::mdns::discover_service;

#[derive(Parser, Debug)]
#[clap(name = "pounce 🐈", version)]
/// A command-line tool for interacting with the Serval mesh.
struct Args {
    #[clap(
        short,
        parse(from_occurrences),
        help = "Pass -v or -vv to increase verbosity"
    )]
    verbose: u64,
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Store the given WASM task type in the mesh.
    #[clap(display_order = 1)]
    Store {
        /// Path to the task manifest file.
        manifest: PathBuf,
    },
    /// Run the specified WASM binary.
    #[clap(display_order = 2)]
    Run {
        /// The name of the previously-stored job to run.
        name: String,
        /// Path to a file to pass to the binary; omit to read from stdin (if present)
        input_file: Option<PathBuf>,
        /// Path to write the output of the job; omit to write to stdout
        output_file: Option<PathBuf>,
    },
    /// Get the status of a job in progress.
    #[clap(display_order = 3)]
    Status { id: Uuid },
    /// Get results for a job run, given its ID.
    #[clap(display_order = 4)]
    Results { id: Uuid },
    /// Get full job run history from the running process.
    #[clap(display_order = 5)]
    History,
    /// Liveness check: ping at least one node on the mesh.
    Ping,
    /// Highly experimental: Pull a package from WAPM.io and store it in Serval Mesh
    Pull {
        /// The name of the software package, formatted as
        /// [[protocol://]registry.tld/]author/packagename[@version][:module]
        identifer: String,
    },
}

static SERVAL_NODE_URL: Mutex<Option<String>> = Mutex::new(None);

/// Convenience function to build urls repeatably.
fn build_url(path: String, version: Option<&str>) -> String {
    let baseurl = SERVAL_NODE_URL.lock().unwrap();
    let baseurl = baseurl
        .as_ref()
        .expect("build_url called while SERVAL_NODE_URL is None");
    if let Some(v) = version {
        format!("{baseurl}/v{v}/{path}")
    } else {
        format!("{baseurl}/{path}")
    }
}

fn upload_manifest(manifest_path: PathBuf) -> Result<()> {
    println!("Reading manifest: {}", manifest_path.display());
    let manifest = Manifest::from_file(&manifest_path)?;

    let mut wasmpath = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    wasmpath.push(manifest.binary());

    println!("Reading WASM executable:{}", wasmpath.display());
    let executable = read_file(wasmpath)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    // Start building pretty output now that we're past the most likely errors.
    println!();
    let mut table = Table::new();
    table.set_format(*prettytable::format::consts::FORMAT_CLEAN);
    table.add_row(row!["WASM task name:", manifest.fq_name()]);
    table.add_row(row!["Version:", manifest.version()]);

    let url = build_url("storage/manifests".to_string(), Some("1"));
    let response = client.post(url).body(manifest.to_string()).send()?;

    if !response.status().is_success() {
        table.add_row(row!["Storing the WASM manifest failed!".bold()]);
        table.add_row(row![format!("{} {}", response.status(), response.text()?)]);
        println!("{table}");
        return Ok(());
    }

    let manifest_integrity = response.text()?;
    table.add_row(row!["Manifest integrity:", manifest_integrity]);

    let vstring = format!(
        "storage/manifests/{}/executable/{}",
        manifest.fq_name(),
        manifest.version()
    );
    let url = build_url(vstring, Some("1"));
    let response = client.put(url).body(executable).send()?;
    if response.status().is_success() {
        let wasm_integrity = response.text()?;
        table.add_row(row!["WASM integrity:", wasm_integrity]);
        table.add_row(row![
            "To run:",
            format!("cargo run -p serval -- run {}", manifest.fq_name())
                .bold()
                .blue()
        ]);
    } else {
        table.add_row(row!["Storing the WASM executable failed!"]);
        table.add_row(row![format!("{} {}", response.status(), response.text()?)]);
    }

    println!("{table}");
    Ok(())
}

/// Convenience function to read an input wasm binary either from a pathbuf or from stdin.
fn read_file_or_stdin(maybepath: Option<PathBuf>) -> Result<Vec<u8>, anyhow::Error> {
    // TODO This implementation should become a streaming implementation.
    let mut buf: Vec<u8> = Vec::new();
    if let Some(fpath) = maybepath {
        return read_file(fpath);
    }

    if atty::is(atty::Stream::Stdin) {
        return Ok(buf);
    }

    let mut reader = BufReader::new(std::io::stdin());
    reader.read_to_end(&mut buf)?;

    Ok(buf)
}

fn read_file(path: PathBuf) -> Result<Vec<u8>, anyhow::Error> {
    // TODO This implementation should become a streaming implementation.
    let mut buf: Vec<u8> = Vec::new();
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    reader.read_to_end(&mut buf)?;

    Ok(buf)
}

/// Request that an available agent run a stored job, with optional input.
fn run(name: String, maybe_input: Option<PathBuf>, maybe_output: Option<PathBuf>) -> Result<()> {
    let input_bytes = read_file_or_stdin(maybe_input)?;

    println!(
        "Sending job {} with {} payload to serval agent...",
        name.blue().bold(),
        format_size(input_bytes.len(), BINARY),
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let url = build_url(format!("jobs/{name}/run"), Some("1"));
    let response = client.post(url).body(input_bytes).send()?;

    if !response.status().is_success() {
        println!("Running the WASM failed!");
        println!("{} {}", response.status(), response.text()?);
        return Ok(());
    }

    let response_body = response.bytes()?;
    log::info!("response body read; length={}", response_body.len());
    match maybe_output {
        Some(outputpath) => {
            eprintln!("Writing output to {outputpath:?}");
            let mut f = File::create(&outputpath)?;
            f.write_all(&response_body)?;
        }
        None => {
            if atty::is(atty::Stream::Stdin) && String::from_utf8(response_body.to_vec()).is_err() {
                eprintln!("Response is non-printable binary data; redirect output to a file or provide an output filename to retrieve it.");
            } else {
                eprintln!("----------");
                std::io::stdout().write_all(&response_body)?;
                eprintln!("----------");
            };
        }
    }

    Ok(())
}

/// Get a job's status from a serval agent node.
fn status(id: Uuid) -> Result<()> {
    let url = build_url(format!("jobs/{id}/status"), Some("1"));
    let response = reqwest::blocking::get(url)?;
    let body: serde_json::Map<String, serde_json::Value> = response.json()?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    Ok(())
}

/// Get a job's results from a serval agent node.
fn results(id: Uuid) -> Result<()> {
    let url = build_url(format!("jobs/{id}/results"), Some("1"));
    let response = reqwest::blocking::get(url)?;
    let body: serde_json::Map<String, serde_json::Value> = response.json()?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    Ok(())
}

/// Get in-memory history from an agent node.
fn history() -> Result<()> {
    let url = build_url("monitor/history".to_string(), Some("1"));
    let response = reqwest::blocking::get(url)?;
    let body: serde_json::Map<String, serde_json::Value> = response.json()?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    Ok(())
}

/// Ping whichever node we've discovered.
fn ping() -> Result<()> {
    let url = build_url("monitor/ping".to_string(), None);
    let response = reqwest::blocking::get(url)?;
    let body = response.text()?;
    println!("PING: {body}");

    Ok(())
}

fn blocking_maybe_discover_service_url(
    service_type: &str,
    env_var_override_name: &str,
) -> Result<String> {
    if let Ok(override_url) = std::env::var(env_var_override_name) {
        return Ok(override_url);
    }

    log::info!("Looking for {service_type} node on the local network...");

    let Ok(info) = Runtime::new().unwrap().block_on(discover_service(service_type)) else {
        return Err(anyhow!(format!(
            "Failed to discover {service_type} node on the local network"
        )));
    };

    let Some(addr) = info.get_addresses().iter().next() else {
        // this should not ever happen, but computers
        return Err(anyhow!(format!(
            "Discovered a node that has no addresses",
        )));
    };

    let port = info.get_port();
    Ok(format!("http://{addr}:{port}"))
}

/// Pull a Wasm package from a package manager, generate its manifest, and store it.
fn pull(identifer: String) -> Result<()> {
    let pkg_spec = PackageSpec::try_from(identifer).unwrap();
    log::debug!("{:#?}", pkg_spec);
    println!(
        "📦 Identified package {}",
        pkg_spec.profile_url().bold().blue()
    );
    println!("🏷  Using module {}", pkg_spec.module.bold().blue());
    if pkg_spec.is_binary_cached() {
        println!(
            "✅ Binary for {} ({}) available locally.",
            pkg_spec.fq_name().bold().green(),
            pkg_spec.fq_digest()
        );
        // Creating a temporary manifest file
        let manifest_path = gen_manifest(&pkg_spec).unwrap();
        // Handing over to existing storage logic
        upload_manifest(manifest_path)?;
    } else {
        println!(
            "⌛️ Binary for {} not available locally, downloading...",
            pkg_spec.fq_name().blue()
        );
        let mod_dl = download_module(&pkg_spec);
        match mod_dl {
            // This means the download function did not break. It does not mean that
            // the executable was downloaded successfully... check HTTP status code.
            Ok(status_code) => {
                if status_code.is_success() {
                    println!(
                        "✅ Downloaded {} ({}) successfully.",
                        pkg_spec.fq_name().bold().green(),
                        pkg_spec.fq_digest()
                    );
                    // Creating a temporary manifest file
                    let manifest_path = gen_manifest(&pkg_spec).unwrap();
                    // Handing over to existing storage logic
                    upload_manifest(manifest_path)?;
                } else if status_code.is_server_error() {
                    println!("🛑 Server error: {}", status_code);
                    println!("   There may be an issue with this package manager.");
                } else if status_code.is_client_error() {
                    println!("🛑 Client error: {}", status_code);
                    println!("{:#?}", status_code);
                    if status_code == 404 {
                        println!("   Failed to download from {:?}", pkg_spec.download_urls());
                    }
                    println!();
                    if pkg_spec.version == "latest" && pkg_spec.registry == PackageRegistry::Wapm {
                        println!(
                            "💡 Please note that wapm.io does not properly alias the `{}` version tag.",
                            "latest".bold().yellow()
                        );
                        println!("   You might want to look up the package and explicitly provide its most recent version:");
                        println!("   \t{}", pkg_spec.profile_url());
                        println!();
                    }
                    // Currently, a 404 is very likely if a package only contains modules that have names other than
                    // the package name (which the module name defaults to if not provided).
                    // TODO: retrieve available modules and interactively ask which module should be downloaded
                    // Quick fix is to point this out to the user:
                    if pkg_spec.name == pkg_spec.module {
                        println!(
                            "💡 Please verify that this package actually contains a `{}` module",
                            pkg_spec.module.bold().yellow()
                        );
                        println!("   by checking the MODULES section on its profile page:");
                        println!("   \t{}", pkg_spec.profile_url());
                        println!(
                            "   If the module name differs from the package name, you need to provide it with"
                        );
                        println!(
                            "   \tserval pull {}:{}",
                            pkg_spec.profile_url(),
                            "<module>".bold().yellow()
                        );
                    }
                } else {
                    println!("😵‍💫 Something else happened. Status: {:?}", status_code);
                }
            }
            // Something went horribly wrong.
            Err(err) => println!("{:#?}", err),
        }
    }
    Ok(())
}

/// Parse command-line arguments and act.
fn main() -> Result<()> {
    let args = Args::parse();

    loggerv::Logger::new()
        .verbosity(args.verbose) // if -v not passed, our default level is WARN
        .line_numbers(false)
        .module_path(true)
        .colors(true)
        .init()
        .unwrap();

    let baseurl = blocking_maybe_discover_service_url("_serval_daemon", "SERVAL_NODE_URL")?;
    SERVAL_NODE_URL.lock().unwrap().replace(baseurl);

    match args.cmd {
        Command::Store { manifest } => upload_manifest(manifest)?,
        Command::Run {
            name,
            input_file,
            output_file,
        } => {
            // If people provide - as the filename, interpret that as stdin/stdout
            let input_file = input_file.filter(|p| p != &PathBuf::from("-"));
            let output_file = output_file.filter(|p| p != &PathBuf::from("-"));
            run(name, input_file, output_file)?;
        }
        Command::Results { id } => results(id)?,
        Command::Status { id } => status(id)?,
        Command::History => history()?,
        Command::Ping => ping()?,
        Command::Pull { identifer } => pull(identifer)?,
    };

    Ok(())
}
