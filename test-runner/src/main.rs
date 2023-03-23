#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]
use clap::Parser;
use engine::extensions::load_extensions;
use owo_colors::OwoColorize;

use std::fs;
use std::fs::File;
use std::io::{stdin, Read};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::str::FromStr;
use utils::structs::{Manifest, Permission};

use engine::ServalEngine;

/// Note: The CLI is just here for simple testing purpose.
/// The real worker will pick up executables and inputs from an API endpoint.
#[derive(Parser, Debug)]
struct CLIArgs {
    /// Path to the Wasm executable to run
    // TODO: Check for the Wasm binary magic bytes [1] or even evaluate file grammar [2].
    // [1]: Example: https://developer.mozilla.org/en-US/docs/WebAssembly/Understanding_the_text_format#the_simplest_module
    // [2]: Specification: https://webassembly.github.io/spec/core/index.html
    exec_path: PathBuf,
    /// Optional path to a file containing input for the executable
    // Naive initial approach: We don't check file content and assume the executable knows what to do with it.
    // TODO: How would we validate that an input file is "correct" without running the job and seeing if it fails? TBD.
    #[clap(long)]
    input_path: Option<PathBuf>,
    /// Optional path to a directory full of Serval extensions
    #[clap(long)]
    extensions_path: Option<PathBuf>,
    #[clap(long)]
    permissions: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = CLIArgs::parse();

    let manifest = match args
        .exec_path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .as_ref()
    {
        "toml" => {
            let manifest = Manifest::from_file(&args.exec_path)?;

            eprintln!(
                "{} {}",
                "manifest:".blue().bold(),
                toml::to_string(&manifest)
                    .unwrap()
                    .replace('\n', "\n          ")
            );

            manifest
        }
        _ => Manifest::new(&args.exec_path),
    };

    let permissions_override = args.permissions.map(|raw_perms| {
        raw_perms
            .split(',')
            .map(|raw_perm| {
                Permission::from_str(raw_perm).unwrap_or_else(|_| {
                    eprintln!("Error: Invalid permission '{raw_perm}'");
                    exit(1);
                })
            })
            .collect::<Vec<_>>()
    });

    let Ok(binary) = fs::read(manifest.binary()) else {
        eprintln!("error: failed to read binary '{}'", manifest.binary().to_string_lossy());
        exit(1);
    };
    if !is_wasm_executable(manifest.binary()) {
        eprintln!(
            "error: binary '{}' is not a wasm executable",
            manifest.binary().to_string_lossy()
        );
        exit(1);
    }

    let stdin = if let Some(input_file) = args.input_path {
        let input_path = Path::new(&input_file);
        fs::read(input_path)?
    } else if !atty::is(atty::Stream::Stdin) {
        let mut buf = vec![];
        stdin().lock().read_to_end(&mut buf)?;
        buf
    } else {
        vec![]
    };

    let extensions = args
        .extensions_path
        .map(|extensions_path| {
            load_extensions(&extensions_path).expect("Failed to load extensions")
        })
        .unwrap_or_default();
    {
        let extensions_readable = if extensions.is_empty() {
            String::from("none")
        } else {
            extensions
                .keys()
                .map(|str| str.to_owned())
                .collect::<Vec<_>>()
                .join(",")
        };
        eprintln!("{} {}", "extensions:".blue().bold(), extensions_readable);
    }

    // Are we still running? Great, let's assume executable and input are usable.
    // The following section is highly inspired by "Approach 1" in [3]. Its "Approach 2" is potentially
    // a lot more powerful and may be the way to go, but I had too many question marks in my eyes when
    // initially reading it to pursue it for a first draft.
    // [3]: https://petermalmgren.com/serverside-wasm-data/

    let permissions =
        permissions_override.unwrap_or_else(|| manifest.required_permissions().to_owned());
    eprintln!("{} {:?}", "permissions:".blue().bold(), permissions);
    eprintln!(
        "{} {}",
        "executing:".blue().bold(),
        manifest.binary().display()
    );
    let mut engine = ServalEngine::new(extensions)?;
    let result = match engine.execute(&binary, &stdin, &permissions) {
        Ok(result) => result,
        Err(err) => match err {
            engine::errors::ServalEngineError::ExecutionError {
                stdout,
                stderr,
                error,
            } => {
                eprintln!(
                    "execution error {error}: stdout={} stderr={}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
                exit(1);
            }
            _ => {
                eprintln!("error: {err}");
                exit(1);
            }
        },
    };
    eprintln!("{} {}", "exit status:".blue().bold(), result.code);
    eprintln!("\n{}:", "stdout".yellow().bold());
    println!("{}", String::from_utf8(result.stdout)?);
    eprintln!("\n{}:", "stderr".yellow().bold());
    println!("{}", String::from_utf8(result.stderr)?);

    Ok(())
}

fn is_wasm_executable(path: &Path) -> bool {
    File::open(path)
        .and_then(|mut file| {
            let mut buf = [0u8; 4];
            file.read_exact(&mut buf)?;
            Ok(buf == *b"\0asm")
        })
        .unwrap_or(false)
}
