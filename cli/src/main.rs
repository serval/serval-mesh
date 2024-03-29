#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Pounce is a CLI tool that interacts with a running serval agent daemon via
/// its HTTP API. It discovers running agents via mDNS advertisement.
use anyhow::Result;
use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use humansize::{format_size, BINARY};
use owo_colors::OwoColorize;
use prettytable::{row, Table};
use utils::mesh::ServalRole;

mod mesh;
mod peers;

use peers::api_client;
use utils::structs::Manifest;

#[derive(Parser, Debug)]
#[clap(name = "pounce 🐈", version)]
/// A command-line tool for interacting with the Serval mesh.
struct Args {
    #[clap(
        short,
        action = clap::ArgAction::Count,
        help = "Pass -v or -vv to increase verbosity"
    )]
    verbose: u8,
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Store the given Wasm task type in the mesh.
    #[clap(display_order = 1)]
    Store {
        /// Path to the task manifest file.
        manifest: PathBuf,
    },
    /// Run the specified Wasm binary.
    #[clap(display_order = 2)]
    Run {
        /// The name of the previously-stored job to run.
        name: String,
        /// Path to a file to pass to the binary; omit to read from stdin (if present)
        input_file: Option<PathBuf>,
        /// Path to write the output of the job; omit to write to stdout
        output_file: Option<PathBuf>,
    },
    /// Get the manifest for a stored job type.
    #[clap(display_order = 3)]
    Manifest {
        /// The name of the stored job.
        name: String,
    },
    /// List all known peers of this node.
    #[clap(display_order = 4)]
    Peers,
    /// List all known peers with the named role.
    #[clap(display_order = 5)]
    PeersWithRole {
        /// The role
        role: ServalRole,
    },
    NodeStatus,
    /// Liveness check: ping at least one node on the mesh.
    Ping,
    /// Monitor a mesh: print out new peers and departing peers as we learn about them.
    Monitor,
}

async fn upload_manifest(manifest_path: PathBuf) -> Result<()> {
    println!("Reading manifest: {}", manifest_path.display());
    let manifest = Manifest::from_file(&manifest_path)?;

    let mut wasmpath = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    wasmpath.push(manifest.binary());

    println!("Reading Wasm executable:{}", wasmpath.display());
    let executable = read_file(wasmpath)?;

    let serval = api_client().await;

    // Start building pretty output now that we're past the most likely errors.
    println!();
    let mut table = Table::new();
    table.set_format(*prettytable::format::consts::FORMAT_CLEAN);
    table.add_row(row!["Wasm task name:", manifest.fq_name()]);
    table.add_row(row!["Version:", manifest.version()]);

    let manifest_resp = serval.store_manifest(&manifest).await;
    let Ok(manifest_integrity) = manifest_resp else {
        table.add_row(row!["Storing the Wasm manifest failed!".bold()]);
        table.add_row(row![format!(
            "{:?}", manifest_resp
        )]);
        println!("{table}");
        return Ok(());
    };

    table.add_row(row!["Manifest integrity:", manifest_integrity]);

    let exec_resp = serval
        .store_executable(&manifest.fq_name(), manifest.version(), executable)
        .await;
    if let Ok(wasm_integrity) = exec_resp {
        table.add_row(row!["Wasm integrity:", wasm_integrity]);
        table.add_row(row![
            "To run:",
            format!("cargo run -p serval -- run {}", manifest.fq_name())
                .bold()
                .blue()
        ]);
    } else {
        table.add_row(row!["Storing the Wasm executable failed!"]);
        table.add_row(row![format!("{:?}", exec_resp)]);
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
async fn run(
    name: String,
    maybe_input: Option<PathBuf>,
    maybe_output: Option<PathBuf>,
) -> Result<()> {
    let input_bytes = read_file_or_stdin(maybe_input)?;

    println!(
        "Sending job {} with {} payload to serval agent...",
        name.blue().bold(),
        format_size(input_bytes.len(), BINARY),
    );

    let serval = api_client().await;
    let response = serval.run_job(&name, input_bytes).await?;

    if !response.status().is_success() {
        println!("Running the Wasm failed!");
        println!("{} {}", response.status(), response.text().await?);
        return Ok(());
    }

    let response_body = response.bytes().await?;
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

async fn get_manifest(name: String) -> Result<()> {
    let manifest = api_client().await.get_manifest(&name).await?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

async fn list_peers() -> Result<()> {
    let body = api_client().await.all_peers().await?;
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}

async fn peers_with_role(role: ServalRole) -> Result<()> {
    let body = api_client().await.peers_with_role(role).await?;
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}

/// Get the runtime status from a serval agent node.
async fn monitor_status() -> Result<()> {
    let body = api_client().await.monitor_status().await?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    Ok(())
}

/// Ping whichever node we've discovered.
async fn ping() -> Result<()> {
    let body = api_client().await.ping().await?;
    println!("PING: {body}");

    Ok(())
}

/// Parse command-line arguments and act.
#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let args = Args::parse();

    loggerv::Logger::new()
        .verbosity(args.verbose as u64) // if -v not passed, our default level is WARN
        .line_numbers(false)
        .module_path(true)
        .colors(true)
        .init()
        .unwrap();

    match args.cmd {
        Command::Store { manifest } => upload_manifest(manifest).await?,
        Command::Run {
            name,
            input_file,
            output_file,
        } => {
            // If people provide - as the filename, interpret that as stdin/stdout
            let input_file = input_file.filter(|p| p != &PathBuf::from("-"));
            let output_file = output_file.filter(|p| p != &PathBuf::from("-"));
            run(name, input_file, output_file).await?;
        }
        Command::NodeStatus => monitor_status().await?,
        Command::Ping => ping().await?,
        Command::Monitor => mesh::monitor_mesh().await?,
        Command::Manifest { name } => get_manifest(name).await?,
        Command::Peers => list_peers().await?,
        Command::PeersWithRole { role } => peers_with_role(role).await?,
    };

    Ok(())
}
