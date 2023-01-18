#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use std::{env, fs, path::PathBuf};

use anyhow::anyhow;
use clap::Parser;
use dotenvy::dotenv;
use utils::{mdns::advertise_service, networking::find_nearest_port};
use uuid::Uuid;

mod api;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    // Where to store data; defaults to a one-off temporary directory if not specified
    path: Option<PathBuf>,
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
    let storage_path = args.path.unwrap_or_else(|| {
        log::warn!("No --path specified; defaulting to $TMPDIR/castaway");
        env::temp_dir().join("castaway")
    });
    if !storage_path.exists() {
        fs::create_dir(&storage_path)?;
    }

    log::info!("Storage path: {}", &storage_path.display());

    // Actually do the thing
    let http_port = find_nearest_port(7475)?;
    let instance_id = Uuid::new_v4();
    advertise_service("serval_storage", http_port, &instance_id, None)?;
    api::init_http("0.0.0.0", http_port, storage_path).await?;

    Err(anyhow!("Future resolved unexpectedly"))
}