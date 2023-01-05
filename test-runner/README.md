# wasm test runner

This is a command-line executable that runs a single wasi payload and exits. It is useful as a testbed for the embedded wasm engine!

A wasm-automatic-iterative-travailleur. Simple-minded, entirely focused on crunching crunchy wasm files.

The current state of this code base is far from where it will eventually be. Right now, this

* Is a simple command-line application that runs a WASM program provided to it
* Will perform some simple checks to test if the relevant files exist or not, but is not very thorough at it
## Issues

* There are three sections marked up with `FIXME:` markers. They all relate to an issue with providing custom pipes for stdin and stdout

## Usage

```text
test-runner
Note: The CLI is just here for simple testing purpose. The real worker will pick up executables and
inputs from an API endpoint

USAGE:
    test-runner <EXEC_PATH> [INPUT_PATH]

ARGS:
    <EXEC_PATH>     Path to the WASM executable to run
    <INPUT_PATH>    Optional path to a file containing input for the executable

OPTIONS:
    -h, --help    Print help information
```

## Example

This assumes the existence of a `.wasm` file and an input file with arbitrary content (but usable by the WASM executable).

Compiling our [wasi-hello-world](https://github.com/servals/wasm-samples/tree/main/wasi-hello-world) example and creating a `testinput.txt` file with some text in it works for an initial quick & dirty test.

Then, this code can be run as follows:

```
cargo run -- path/to/serval-facts.wasm
```

Running the `serval-facts` example will produce output similar to the following:
```
executing: ../../wasm-samples/build/serval-facts.wasm
exit status: 0

stdout:
Content-Type: text/plain

Servals have cat fights. Threat displays between hostile servals can look scary,
with the cats flattening their ears, arching their backs, baring their teeth,
and nodding their heads vigorously. If the situation escalates, they lash out
with their long front legs and bark and growl.

stderr:
```
