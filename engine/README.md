# Engine glue

This is the library that implements our convenience layer around the embedded WASM engine.

## design notes

The current api is bare-bones. Suggested improvements:

- Provide an `execute()` that reads both the wasm executable bytes and input bytes from streams.
- Or abstract this somehow usefully. We might want to read from a file for instance.
- Figure out how to get the exit status from wasmtime for real. The example from their docs isn't working.
- Write tests once we have something to test that isn't just "the embedded wasmtime thing is working".

