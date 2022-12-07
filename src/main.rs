use clap::Parser;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::Path;
use wasi_common::pipe::{ReadPipe, WritePipe};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

/// I am just a simple worker staying busy with one WebAssembly program at a time.

/// Note: The CLI is just here for simple testing purpose.
/// The real worker will pick up executables and inputs from an API endpoint.
#[derive(Parser, Debug)]
struct CLIArgs {
    /// Path to the executable file
    /// Note that we only accept files ending in .wasm as executables.
    /// TODO: Check for the WASM binary magic bytes [1] or even evaluate file grammar [2].
    /// [1]: Example: https://developer.mozilla.org/en-US/docs/WebAssembly/Understanding_the_text_format#the_simplest_module
    /// [2]: Specification: https://webassembly.github.io/spec/core/index.html
    exec_path: String,
    /// Path to the input file
    /// Naive initial approach: We don't check file content and assume the executable knows what to do with it.
    /// TODO: How would we validate that an input file is "correct" without running the job and seeing if it fails? TBD.
    input_path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Input {
    pub name: String,
    pub num: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Output {
    pub names: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = CLIArgs::parse();

    let exec_path = Path::new(&args.exec_path);
    let exec_exists = exec_path.exists();

    // Check if the executable file exists and looks as expected
    println!("\nExecutable file: {}", exec_path.display());
    if exec_exists {
        println!("\t✅ File exists!");
    } else {
        println!("\t🛑 File does not exist!\nExiting.");
        // Note: Got sidetracked with writing an enum for different exit codes etc here, but ran into some weird issues
        // often enough to realize I'm getting sidetracked with a low-prio aspect of this thing.
        // TODO: Proper exit handling.
        std::process::exit(1)
    }
    if exec_path.extension().and_then(OsStr::to_str) != Some("wasm") {
        println!(
            "\t🛑 File type not supported ({})!\nExiting.",
            exec_path.extension().and_then(OsStr::to_str).unwrap()
        );
        std::process::exit(2)
    }
    // TODO: check if it is *actually* valid WebAssembly (rather than just a valid extension).

    let input_path = Path::new(&args.input_path);
    let input_exists = input_path.exists();

    // Check if the input file exists and looks as expected
    println!("\nInput file: {}", input_path.display());
    if input_exists {
        println!("\t✅ File exists!");
    } else {
        println!("\t🛑 File does not exist!\nExiting.");
        // TODO: Proper exit handling.
        std::process::exit(3)
    }

    // Are we still running? Great, let's assume executable and input are usable.
    // The following section is highly inspired by "Approach 1" in [3]. Its "Approach 2" is potentially
    // a lot more powerful and may be the way to go, but I had too many question marks in my eyes when
    // initially reading it to pursue it for a first draft.
    // [3]: https://petermalmgren.com/serverside-wasm-data/

    let engine = Engine::default();
    let mut linker: Linker<WasiCtx> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |s| s)?;

    // Creating some dummy input structure
    let input = Input {
        name: args.input_path,
        num: 10,
    };
    // Serializing input structure to a string
    let serialized_input = serde_json::to_string(&input)?;

    // Creating stdin and stdout for the WASI context.
    // This allows us to pipe input to the module and retrieve output after execution.
    let stdin = ReadPipe::from(serialized_input);
    let stdout = WritePipe::new_in_memory();

    // Build a WASI context which uses the custom stdin and stdout
    let wasi = WasiCtxBuilder::new()
        .stdin(Box::new(stdin))
        .stdout(Box::new(stdout.clone()))
        .inherit_stderr()
        .build();

    // Create a `Store` for the WASI module to live in
    let mut store = Store::new(&engine, wasi);

    // Register the module with the linker
    let module = Module::from_file(&engine, exec_path)?;
    linker.module(&mut store, "", &module)?;

    // This is where the WASM module actually gets run
    println!("\nRunning {}...", exec_path.display());
    linker
        .get_default(&mut store, "")?
        .typed::<(), (), _>(&store)?
        .call(&mut store, ())?;

    // From [3]: "Calling drop(store) is important, otherwise converting the WritePipe into a Vec<u8> will fail"
    drop(store);

    let bytes: Vec<u8> = stdout
        .try_into_inner()
        .map_err(|_err| anyhow::Error::msg("sole remaining reference"))?
        .into_inner();
    let contents = std::str::from_utf8(&bytes)?;
    println!("raw output:\n{:#?}", contents);
    let output: String = serde_json::from_str(contents)?;
    println!("The answer is {:#?}.", output);

    Ok(())
}
