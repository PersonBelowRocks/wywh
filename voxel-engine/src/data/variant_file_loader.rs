use std::{ffi::OsStr, fs::File, io::Read, path::Path};

use super::{error::VariantFileLoaderError, voxel::descriptor::VariantDescriptor};

pub static VARIANT_FILE_EXTENSION: &'static str = "variant";

fn path_to_label(path: &Path) -> Option<&str> {
    path.extension()
        .and_then(OsStr::to_str)
        .filter(|&e| e == VARIANT_FILE_EXTENSION)?;

    path.file_stem()
        .and_then(OsStr::to_str)
        .filter(|&s| !s.contains(':'))
}

#[derive(Clone)]
pub struct VariantFileLoader {
    raw_descriptors: hb::HashMap<String, Vec<u8>>,
}

impl VariantFileLoader {
    pub fn new() -> Self {
        Self {
            raw_descriptors: hb::HashMap::new(),
        }
    }

    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.raw_descriptors.keys().map(AsRef::as_ref)
    }

    pub fn load_folder(&mut self, path: impl AsRef<Path>) -> Result<(), VariantFileLoaderError> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;

            if entry.file_type()?.is_file() {
                self.load_file(entry.path())?;
            }
        }

        Ok(())
    }

    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<(), VariantFileLoaderError> {
        let path = path.as_ref();

        let Some(label) = path_to_label(&path) else {
            return Err(VariantFileLoaderError::InvalidFileName(path.to_path_buf()));
        };

        let mut file = File::open(&path)?;

        let mut buffer = Vec::<u8>::with_capacity(file.metadata()?.len() as _);
        file.read_to_end(&mut buffer)?;

        self.add_raw_buffer(label.into(), buffer);
        Ok(())
    }

    pub fn add_raw_buffer(&mut self, label: String, buffer: Vec<u8>) {
        self.raw_descriptors.insert(label, buffer);
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
