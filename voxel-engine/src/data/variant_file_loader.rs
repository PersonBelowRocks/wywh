use std::{
    ffi::OsStr,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use super::{error::VariantFileLoaderError, voxel::descriptor::VariantDescriptor};

fn path_to_label(path: &Path) -> Option<&str> {
    path.file_stem()
        .and_then(OsStr::to_str)
        .filter(|&s| s.contains(':'))
}

pub struct VariantFileLoader {
    raw_descriptors: hb::HashMap<String, Vec<u8>>,
}

impl VariantFileLoader {
    pub fn new() -> Self {
        Self {
            raw_descriptors: hb::HashMap::new(),
        }
    }

    pub fn load_folder(&mut self, path: impl AsRef<Path>) -> Result<(), VariantFileLoaderError> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;

            if entry.file_type()?.is_file() {
                let path = entry.path();
                let mut file = File::open(&path)?;

                let mut buffer = Vec::<u8>::with_capacity(file.metadata()?.len() as _);
                file.read_to_end(&mut buffer)?;

                let Some(label) = path_to_label(&path) else {
                    return Err(VariantFileLoaderError::InvalidFileName(path));
                };

                self.raw_descriptors.insert(label.into(), buffer);
            }
        }

        Ok(())
    }

    pub fn parse(&self, label: &str) -> Result<VariantDescriptor, VariantFileLoaderError> {
        let buffer = self
            .raw_descriptors
            .get(label)
            .ok_or(VariantFileLoaderError::VariantNotFound(label.into()))?;

        let descriptor = deser_hjson::from_slice::<VariantDescriptor>(buffer)?;
        Ok(descriptor)
    }
}
