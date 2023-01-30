use std::mem::size_of;

use anyhow::anyhow;
use wasmtime::{Caller, Extern, Memory, Val};

/// Calls into the guest environment to allocate a chunk of memory of the given size. if the guest
/// does not expose a compatible alloc function, this will fail, and data exchange with the guest
/// will not be possible. Our SDK automatically provides guest apps with this function.
pub fn alloc<T>(
    caller: &mut Caller<'_, T>,
    num_bytes_required: usize,
) -> Result<usize, anyhow::Error> {
    let Ok(alloc) = get_func_from_caller(caller, "alloc") else {
        return Err(anyhow!("Failed to get alloc function from guest"));
    };

    let params: Vec<Val> = vec![Val::I64(num_bytes_required as i64)];

    // Note that results has to have one Val in it already for this call to work, but the Val itself
    // doesn't matter. I32(0), I64(0), heck even FuncRef(None) works just fine. I am noting that
    // both because I find it surprising and because I don't want future spelunkers to worry about
    // this Val::I64 here in the context of wasm32-wasi.
    let mut results: Vec<Val> = vec![Val::I64(0)];
    alloc.call(caller, &params, &mut results)?;

    // This bit of code is here as future-proofing; we're development with workloads built using
    // wasm32-wasi, so usize will always be an I32 from those. Whenever wasm64-wasi becomes a thing,
    // we can see what we need to do here.
    let ptr = match results[0] {
        wasmtime::Val::I32(ptr) => Ok(ptr as usize),
        _ => Err(anyhow!("Unsupported usize")),
    }?;

    Ok(ptr)
}

/// Returns a handle to the exported guest function with the given name, or an error if none exists.
pub fn get_func_from_caller<T>(
    caller: &mut Caller<'_, T>,
    export_name: &str,
) -> Result<wasmtime::Func, ()> {
    let Some(Extern::Func(f)) = caller.get_export(export_name) else {
        return Err(());
    };

    Ok(f)
}

/// Returns a handle to the guest environment's Memory object.
pub fn get_memory_from_caller<T>(caller: &mut Caller<'_, T>) -> Result<Memory, ()> {
    let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
        return Err(());
    };

    Ok(mem)
}

/// Reads `len` bytes of data from the guest's memory starting at `ptr`.
pub fn read_bytes<T>(
    caller: &Caller<'_, T>,
    memory: Memory,
    ptr: usize,
    len: usize,
) -> Result<Vec<u8>, anyhow::Error> {
    let mut buf: Vec<u8> = vec![0; len];
    if let Err(err) = memory.read(caller, ptr.to_owned(), &mut buf) {
        eprintln!("Memory access error: {err:?}");
        return Err(anyhow!(err));
    };
    Ok(buf)
}

/// Writes the given data into the guest's memory, prefixed with a i64 indicating how many bytes of
/// data were written. That is, if we want write the bytes [10, 20, 30, 40], this function will
/// actually allocate 8 bytes total: 4 bytes for a i64 indicating the length of the data, followed by
/// the 4 bytes of data itself. So, the value returned by this function would point to a chunk of
/// memory containing the byte sequence [4, 0, 0, 0, 10, 20, 30, 40].
/// A peer function to this one (to go from a pointer in shared memory to a Vec<u8> containing data)
/// exists in the SDK as `get_bytes_from_host`.
pub fn write_bytes<T>(
    caller: &mut Caller<'_, T>,
    memory: &Memory,
    bytes: Vec<u8>,
) -> Result<usize, anyhow::Error> {
    // Allocate enough memory to write a i64 + the contents of `bytes`. We'll write
    // the length of bytes as a i64 at the start of the memory range, followed by
    // the contents of `bytes`.
    let num_bytes_required = size_of::<i64>() + bytes.len();
    let ptr = alloc(caller, num_bytes_required)?;

    // Now, copy the data over
    let len_bytes = Vec::from((bytes.len() as i64).to_le_bytes());
    let out_buffer = [len_bytes, bytes].concat();
    memory.write(caller, ptr, &out_buffer)?;

    Ok(ptr)
}
