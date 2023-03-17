use std::{fs, path::PathBuf};

use wasmtime::{Engine, Module};

use crate::errors::ServalEngineError;

#[derive(Clone, Debug)]
pub struct ServalExtension {
    filename: PathBuf,
    name: String,
}

impl ServalExtension {
    pub fn new(filename: PathBuf) -> Self {
        let name = {
            let mut filename = filename.clone();
            filename.set_extension("");
            filename.file_name().unwrap().to_string_lossy().into()
        };

        ServalExtension { filename, name }
    }

    pub fn module_for_engine(&self, engine: &Engine) -> Result<Module, ServalEngineError> {
        let bytes = &fs::read(&self.filename)?[..];
        Module::from_binary(engine, bytes).map_err(ServalEngineError::ModuleLoadError)
    }
}

pub fn load_extensions(path: &PathBuf) -> Result<HashMap<String, ServalExtension>, ServalError> {
    // Read the contents of the directory at the given path and build a HashMap that maps
    // from the module's name (the filename minus the .wasm extension) to its path on disk.

    let extensions: HashMap<String, ServalExtension> = fs::read_dir(&path)?
        .filter_map(|entry| {
            entry.ok().and_then(|entry| {
                let filename = entry.file_name();
                let filename = filename.to_string_lossy();
                if !filename.to_lowercase().ends_with(".wasm") {
                    return None;
                }
                let module_name = &filename[0..filename.len() - ".wasm".len()];
                Some((module_name.to_string(), ServalExtension::new(entry.path())))
            })
        })
        .collect();

    Ok(extensions)
}
