#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use uuid::Uuid;

use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Parser, Debug)]
#[clap(name = "pounce 🐈", version)]
/// Interacts with the running serval agent daemon via its http API.
struct Args {
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Run the specified WASM binary.
    #[clap(display_order = 1)]
    Run {
        /// A descriptive name for the job
        #[clap(long, short)]
        name: Option<String>,
        /// A description for the job
        #[clap(long, short)]
        description: Option<String>,
        /// The file containing the wasm binary to run. Omit to read from stdin.
        #[clap(value_name = "FILE")]
        file: Option<PathBuf>,
    },
    /// Get the status of a job in progress.
    #[clap(display_order = 2)]
    Status { id: Uuid },
    /// Get results for a job run, given its ID.
    #[clap(display_order = 3)]
    Results { id: Uuid },
    /// Get full job run history from the running process.
    #[clap(display_order = 4)]
    History,
}

static SERVAL_NODE_URL: Mutex<Option<String>> = Mutex::new(None);

/// Convenience function to build urls repeatably.
fn build_url(path: String) -> String {
    let baseurl = SERVAL_NODE_URL.lock().unwrap();
    let baseurl = baseurl
        .as_ref()
        .expect("build_url called while SERVAL_NODE_URL is None");
    format!("{baseurl}/{path}")
}

/// Convenience function to read an input wasm binary either from a pathbuf or from stdin.
fn read_binary(maybepath: Option<PathBuf>) -> Result<Vec<u8>, anyhow::Error> {
    // TODO This implementation should become a streaming implementation.
    let mut binary: Vec<u8> = Vec::new();
    let size = if let Some(ref fpath) = maybepath {
        let file = File::open(fpath)?;
        let mut reader = BufReader::new(file);
        reader.read_to_end(&mut binary)?
    } else {
        let mut reader = BufReader::new(std::io::stdin());
        reader.read_to_end(&mut binary)?
    };

    if size == 0 {
        Err(anyhow!("no executable data read!"))
    } else {
        Ok(binary)
    }
}

/// Post a wasm executable to a waiting agent to run.
fn run(
    name: Option<String>,
    description: Option<String>,
    maybepath: Option<PathBuf>,
) -> Result<()> {
    let binary = read_binary(maybepath)?;
    let binary_part = reqwest::blocking::multipart::Part::bytes(binary);

    let envelope = serde_json::json!({
        "id": &Uuid::new_v4().to_string(),
        "name": name.unwrap_or_else(|| "temp-name".to_string()),
        "description": description.unwrap_or_else(|| "posted via command-line".to_string())
    });
    let envelope_part = reqwest::blocking::multipart::Part::text(envelope.to_string());

    let client = reqwest::blocking::Client::new();
    let form = reqwest::blocking::multipart::Form::new()
        .part("envelope", envelope_part)
        .part("executable", binary_part);

    let url = build_url("jobs".to_string());
    let response = client.post(&url).multipart(form).send()?;

    let body = response.text()?;

    println!("{body}");

    Ok(())
}

/// Get a job's status from a serval agent node.
fn status(id: Uuid) -> Result<()> {
    let url = build_url(format!("jobs/{id}/status"));
    let response = reqwest::blocking::get(&url)?;
    let body: serde_json::Map<String, serde_json::Value> = response.json()?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    Ok(())
}

/// Get a job's results from a serval agent node.
fn results(id: Uuid) -> Result<()> {
    let url = build_url(format!("jobs/{id}/results"));
    let response = reqwest::blocking::get(&url)?;
    let body: serde_json::Map<String, serde_json::Value> = response.json()?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    Ok(())
}

/// Get in-memory history from an agent node.
fn history() -> Result<()> {
    let url = build_url("monitor/history".to_string());
    let response = reqwest::blocking::get(&url)?;
    let body: serde_json::Map<String, serde_json::Value> = response.json()?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    Ok(())
}

fn blocking_maybe_discover_service_url(
    service_type: &str,
    env_var_override_name: &str,
) -> Result<String> {
    if let Ok(override_url) = std::env::var(env_var_override_name) {
        return Ok(override_url);
    }

    println!("Looking for {service_type} node on the local network...");

    let mdns = ServiceDaemon::new()?;
    let service_type = format!("{service_type}._tcp.local.");
    let receiver = mdns.browse(&service_type)?;
    while let Ok(event) = receiver.recv() {
        let ServiceEvent::ServiceResolved(info) = event else {
            // We don't care about other events here
            continue;
        };
        if let Some(addr) = info.get_addresses().iter().next() {
            let port = info.get_port();
            return Ok(format!("http://{addr}:{port}"));
        }
    }

    Err(anyhow!(format!(
        "Failed to discover {service_type} node on the local network"
    )))
}

/// Parse command-line arguments and act.
fn main() -> Result<()> {
    let baseurl = blocking_maybe_discover_service_url("_serval_daemon", "SERVAL_NODE_URL")?;
    SERVAL_NODE_URL.lock().unwrap().replace(baseurl);

    let args = Args::parse();
    match args.cmd {
        Command::Run {
            name,
            description,
            file,
        } => {
            run(name, description, file)?;
        }
        Command::Results { id } => results(id)?,
        Command::Status { id } => status(id)?,
        Command::History => history()?,
    };

    Ok(())
}
