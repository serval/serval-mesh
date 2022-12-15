use wasi_common::pipe::{ReadPipe, WritePipe};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

use std::io::Cursor;

#[derive(Clone)]
pub struct ServalEngine {
    engine: Engine,
    linker: Linker<WasiCtx>,
}

impl ServalEngine {
    pub fn new() -> anyhow::Result<Self> {
        let engine = Engine::default();
        let mut linker: Linker<WasiCtx> = Linker::new(&engine);
        wasmtime_wasi::add_to_linker(&mut linker, |s| s)?;

        Ok(Self { engine, linker })
    }

    pub fn execute(
        &mut self,
        binary: &[u8],
        input: Option<ReadPipe<Cursor<String>>>,
    ) -> anyhow::Result<Vec<u8>> {
        let stdout = WritePipe::new_in_memory();

        let stdin = match input {
            Some(v) => v,
            None => ReadPipe::from("".to_string()),
        };

        let wasi = WasiCtxBuilder::new()
            .stdin(Box::new(stdin))
            .stdout(Box::new(stdout.clone()))
            .inherit_stderr()
            .build();

        let mut store = Store::new(&self.engine, wasi);
        let module = Module::from_binary(&self.engine, binary)?;
        self.linker.module(&mut store, "", &module)?;

        self.linker
            .get_default(&mut store, "")?
            .typed::<(), (), _>(&store)?
            .call(&mut store, ())?;

        // From [3]: "Calling drop(store) is important, otherwise converting the WritePipe into a Vec<u8> will fail"
        drop(store);

        let bytes: Vec<u8> = stdout
            .try_into_inner()
            .map_err(|_err| anyhow::Error::msg("sole remaining reference"))?
            .into_inner();
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn write_tests_please() {}
}
