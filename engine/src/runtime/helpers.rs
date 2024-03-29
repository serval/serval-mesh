use std::mem::size_of;

use wasmtime::{Caller, Extern, Memory, Val};

use crate::errors::ServalEngineError;

/// Calls into the guest environment to allocate a chunk of memory of the given size. if the guest
/// does not expose a compatible alloc function, this will fail, and data exchange with the guest
/// will not be possible. Our SDK automatically provides guest apps with this function.
pub fn alloc<T>(
    caller: &mut Caller<'_, T>,
    num_bytes_required: usize,
) -> Result<usize, ServalEngineError> {
    let Ok(alloc) = get_func_from_caller(caller, "alloc") else {
        return Err(ServalEngineError::InteropAllocUnavailable);
    };

    // Note: We're casting an unsigned usize into an i32, which means we're losing
    // half of our value range in wasm32. This shouldn't be a problem in practice,
    // but I am calling it out here since it's not ideal. We can switch to using an
    // I64 if it ever turns out to be a problem.
    let params: Vec<Val> = vec![Val::I32(num_bytes_required as i32)];
    let mut results: Vec<Val> = vec![Val::I32(0)];

    if alloc.call(caller, &params, &mut results).is_err() {
        return Err(ServalEngineError::InteropAllocFailed);
    };
    let wasmtime::Val::I32(ptr) = results[0] else {
        return Err(ServalEngineError::InteropAllocFailed);
    };

    Ok(ptr as usize)
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
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, ServalEngineError> {
    let mut buf: Vec<u8> = vec![0; len as usize];
    if let Err(err) = memory.read(caller, ptr.to_owned() as usize, &mut buf) {
        return Err(ServalEngineError::InteropMemoryAccessError(err));
    };
    Ok(buf)
}

/// Writes the given data into the guest's memory, prefixed with a u32 indicating how many bytes of
/// data were written. That is, if we want write the bytes [10, 20, 30, 40], this function will
/// actually allocate 8 bytes total: 4 bytes for a u32 indicating the length of the data, followed by
/// the 4 bytes of data itself. So, the value returned by this function would point to a chunk of
/// memory containing the byte sequence [4, 0, 0, 0, 10, 20, 30, 40].
/// A peer function to this one (to go from a pointer in shared memory to a Vec<u8> containing data)
/// exists in the SDK as `get_bytes_from_host`.
pub fn write_bytes<T>(
    caller: &mut Caller<'_, T>,
    memory: &Memory,
    bytes: Vec<u8>,
) -> Result<usize, ServalEngineError> {
    assert!(bytes.len() < u32::MAX as usize);

    // Allocate enough memory to write a u32 + the contents of `bytes`. We'll write
    // the length of bytes as a u32 at the start of the memory range, followed by
    // the contents of `bytes`.
    let num_bytes_required = size_of::<u32>() + bytes.len();
    let ptr = alloc(caller, num_bytes_required)?;

    // Now, copy the data over
    let len_bytes = Vec::from((bytes.len() as u32).to_le_bytes());
    let out_buffer = [len_bytes, bytes].concat();
    if let Err(err) = memory.write(caller, ptr, &out_buffer) {
        return Err(ServalEngineError::InteropMemoryAccessError(err));
    };

    Ok(ptr)
}
