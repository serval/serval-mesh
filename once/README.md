# wait-for-it one shot

This is a command-line executable that runs a single wasi payload and exits. It is useful as a testbed for the embedded wasm engine!

A wasm-automatic-iterative-travailleur. Simple-minded, entirely focused on crunching crunchy wasm files.

The current state of this code base is far from where it will eventually be. Right now, this

* Is a simple command-line application that runs a WASM program provided to it
* Will perform some simple checks to test if the relevant files exist or not, but is not very thorough at it
## Issues

* There are three sections marked up with `FIXME:` markers. They all relate to an issue with providing custom pipes for stdin and stdout
## Prerequisites

This assumes the existence of a `.wasm` file and an input file with arbitrary content (but usable by the WASM executable).

Compiling our [wasi-hello-world](https://github.com/servals/wasm-samples/tree/main/wasi-hello-world) example and creating a `testinput.txt` file with some text in it works for an initial quick & dirty test.

Then, this code can be run as follows:

```
cargo run -- path/to/wasi-hello-world.wasm testinput.txt
```

Running the `wasi-hello-world` example will produce output similar to the following:
```
Executable file: wasi-hello-world.wasm
        ✅ File exists!

Input file: testinput.txt
        ✅ File exists!

Running wasi-hello-world.wasm...
Content-Type: text/plain

Servals communicate with pee. Both males and females mark their territories by
spraying urine on trees and bushes, scraping fresh urine into the ground with
their claws, and rubbing their cheek glands on the ground or brush. Males tend
to mark their territory more frequently than females, spraying up to 46 times
per hour or 41 times per square kilometer. One male was recorded marking 566
times in a four hour period, when he was following a female.
```
