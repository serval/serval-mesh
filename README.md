# serval-mesh

This monorepo contains the source for the various components of the Serval mesh, intended to run on any host where you want to run WASM payloads.

- `engine`: the [wasmtime](https://lib.rs/crates/wasmtime) glue
- `once`: a CLI to execute a WASM payload once, useful for developing the engine
- `daemon`: a daemon that listens on a port for incoming HTTP requests with payloads to run
- `queuey-queue`: temporarily a separate service during MVP/demo period, this is manages the job queue for the Serval mesh. This functionality will eventually be integrated into the Serval agent.

To build everything: `just build`. To test everything: `just ci`. `just help` to see other justfile recipes.

## LICENSE

[BSD-2-Clause-Patent](./LICENSE)
