use std::{ffi::OsStr, fs::File, io::Read, path::Path};

use super::{
    error::VariantFileLoaderError, resourcepath::ResourcePath, voxel::descriptor::VariantDescriptor,
};

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
    raw_descriptors: hb::HashMap<ResourcePath, Vec<u8>>,
}

impl VariantFileLoader {
    pub fn new() -> Self {
        Self {
            raw_descriptors: hb::HashMap::new(),
        }
    }

    pub fn labels(&self) -> impl Iterator<Item = &ResourcePath> {
        self.raw_descriptors.keys()
    }

    // TODO: this should recurse through the folder loading all variants
    pub fn load_folder(&mut self, path: impl AsRef<Path>) -> Result<(), VariantFileLoaderError> {
        let path = path.as_ref();

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;

            if entry.file_type()?.is_file()
                && entry.path().extension().is_some_and(|ext| ext == "variant")
            {
                let label = ResourcePath::try_from(
                    entry
                        .path()
                        .strip_prefix(path)
                        .map_err(|_| VariantFileLoaderError::InvalidFileName(entry.path()))?,
                )?;

                self.load_file(entry.path(), Some(label))?;
            }
        }

        Ok(())
    }

    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
        label: Option<ResourcePath>,
    ) -> Result<(), VariantFileLoaderError> {
        let path = path.as_ref();

        let label = match label {
            Some(label) => label,
            None => ResourcePath::try_from(path)?,
        };

        let mut file = File::open(&path)?;

        let mut buffer = Vec::<u8>::with_capacity(file.metadata()?.len() as _);
        file.read_to_end(&mut buffer)?;

        self.add_raw_buffer(label.into(), buffer);
        Ok(())
    }

    pub fn add_raw_buffer(&mut self, label: ResourcePath, buffer: Vec<u8>) {
        self.raw_descriptors.insert(label, buffer);
    }

    pub fn parse(&self, label: &ResourcePath) -> Result<VariantDescriptor, VariantFileLoaderError> {
        let buffer = self
            .raw_descriptors
            .get(label)
            .ok_or(VariantFileLoaderError::VariantNotFound)?;

        let descriptor = deser_hjson::from_slice::<VariantDescriptor>(buffer)?;
        Ok(descriptor)
    }
}
