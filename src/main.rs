use dotenv::dotenv;

fn main() {
    let did_find_dotenv = dotenv().ok().is_some();
    if cfg!(debug_assertions) && !did_find_dotenv {
        println!("Debug-only warning: no .env file found to configure logging; all logging will be disabled. Add RUST_LOG=info to .env to see logging.");
    }
    env_logger::init();

    log::info!("Hello, world!");
}
