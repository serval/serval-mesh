#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]
/// I am just a simple worker staying busy with one WebAssembly program at a time.
use clap::Parser;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::Path;
use std::{ffi::OsStr, fs};

use engine::ServalEngine;

/// Note: The CLI is just here for simple testing purpose.
/// The real worker will pick up executables and inputs from an API endpoint.
#[derive(Parser, Debug)]
struct CLIArgs {
    /// Path to the WASM executable to run
    // TODO: Check for the WASM binary magic bytes [1] or even evaluate file grammar [2].
    // [1]: Example: https://developer.mozilla.org/en-US/docs/WebAssembly/Understanding_the_text_format#the_simplest_module
    // [2]: Specification: https://webassembly.github.io/spec/core/index.html
    exec_path: String,
    /// Optional path to a file containing input for the executable
    // Naive initial approach: We don't check file content and assume the executable knows what to do with it.
    // TODO: How would we validate that an input file is "correct" without running the job and seeing if it fails? TBD.
    input_path: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = CLIArgs::parse();

    let exec_path = Path::new(&args.exec_path);
    let extension = exec_path.extension().and_then(OsStr::to_str);

    // TODO: check if it is *actually* valid WebAssembly (rather than just a valid extension).
    if extension != Some("wasm") {
        println!(
            "\t⚠️ {}: file extension should be `wasm` but is instead `{}`.",
            "Warning".red(),
            extension.unwrap_or_default().blue()
        );
    }
    let binary = fs::read(exec_path)?;

    let stdin = if let Some(input_file) = args.input_path {
        let input_path = Path::new(&input_file);
        fs::read(input_path)?
    } else {
        Vec::<u8>::new()
    };

    // Are we still running? Great, let's assume executable and input are usable.
    // The following section is highly inspired by "Approach 1" in [3]. Its "Approach 2" is potentially
    // a lot more powerful and may be the way to go, but I had too many question marks in my eyes when
    // initially reading it to pursue it for a first draft.
    // [3]: https://petermalmgren.com/serverside-wasm-data/

    eprintln!("\n{} {}", "executing:".blue().bold(), exec_path.display());
    let mut engine = ServalEngine::new(HashMap::new())?;
    let result = engine.execute(&binary, &stdin)?;
    eprintln!("{} {}", "exit status:".blue().bold(), result.code);
    eprintln!("\n{}:", "stdout".yellow().bold());
    println!("{}", String::from_utf8(result.stdout)?);
    eprintln!("\n{}:", "stderr".yellow().bold());
    println!("{}", String::from_utf8(result.stderr)?);

    Ok(())
}
