# List available recipes
help:
    just -l

# Run a test, assuming the example repo is one level up...
test IN="README.md":
    cargo run -- ../wasm-samples/build/wasi-hello-world.wasm {{IN}}
