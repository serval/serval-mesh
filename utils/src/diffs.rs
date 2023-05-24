use std::io::Cursor;

use qbsdiff::{Bsdiff, Bspatch};

use crate::errors::ServalResult;

pub fn make_patch(source: &[u8], target: &[u8]) -> ServalResult<Vec<u8>> {
    let mut patch = Vec::new();
    Bsdiff::new(source, target).compare(Cursor::new(&mut patch))?;
    Ok(patch)
}

pub fn apply_patch(source: &[u8], patch: &[u8]) -> ServalResult<Vec<u8>> {
    let patcher = Bspatch::new(patch)?;
    let mut target = Vec::with_capacity(patcher.hint_target_size() as usize);
    patcher.apply(source, Cursor::new(&mut target))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{BufReader, Read};

    use ssri::Integrity;

    use super::{apply_patch, make_patch};

    #[test]
    fn patch_integrity() {
        // verify that our patcher does what it promises.
        let mut version1: Vec<u8> = Vec::new();
        let file = File::open("./tests/fixtures/serval-facts-1.wasm").expect("fixture 1 missing!");
        let mut reader = BufReader::new(file);
        reader
            .read_to_end(&mut version1)
            .expect("fixture should be readable");

        let mut version2: Vec<u8> = Vec::new();
        let file = File::open("./tests/fixtures/serval-facts-2.wasm").expect("fixture 2 missing!");
        let mut reader = BufReader::new(file);
        reader
            .read_to_end(&mut version2)
            .expect("fixture should be readable");

        let patch = make_patch(&version1, &version2).expect("creating the patch failed!");
        let patched = apply_patch(&version1, &patch).expect("applying the patch failed!");

        let source_sri = Integrity::from(version2);
        let patched_sri = Integrity::from(patched);
        assert_eq!(source_sri, patched_sri);
    }
}
