#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use std::env;
use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use dotenvy::dotenv;
use utils::mdns::advertise_service;
use utils::networking::find_nearest_port;
use uuid::Uuid;

mod api;
mod queue;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    persist: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let did_find_dotenv = dotenv().ok().is_some();
    if cfg!(debug_assertions) && !did_find_dotenv {
        println!("Debug-only warning: no .env file found to configure logging; all logging will be disabled. Add RUST_LOG=info to .env to see logging.");
    }
    env_logger::init();

    // Figure out where we're storing data
    let args = Args::parse();
    let job_queue_persist_filename = args.persist.unwrap_or_else(|| {
        let default_filename = String::from("queuey-queue.json");
        log::warn!("No --persist filename specified; defaulting to $TMPDIR/{default_filename}");
        env::temp_dir().join(default_filename)
    });

    let http_port = find_nearest_port(1717)?;
    let instance_id = Uuid::new_v4();
    advertise_service("serval_queue", http_port, &instance_id, None)?;
    api::init_http("0.0.0.0", http_port, job_queue_persist_filename).await?;

    Err(anyhow!("Future resolved unexpectedly"))
}
