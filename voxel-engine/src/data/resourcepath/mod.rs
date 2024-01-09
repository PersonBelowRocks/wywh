pub mod error;
pub mod impls;
pub mod serde;

pub type ResourcePathPart = String;

pub fn rpath(s: &str) -> ResourcePath {
    ResourcePath::try_from(s).unwrap()
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ResourcePath {
    parts: Vec<ResourcePathPart>,
}

impl ResourcePath {
    pub(self) fn from_parts(parts: Vec<ResourcePathPart>) -> Self {
        Self { parts }
    }

    pub fn len(&self) -> usize {
        let mut len = 0usize;
        self.parts.iter().for_each(|part| len += part.len());
        len
    }

    pub fn num_parts(&self) -> usize {
        self.parts.len()
    }

    pub fn parts(&self) -> impl Iterator<Item = &ResourcePathPart> {
        self.parts.iter()
    }

    pub fn get_part(&self, idx: usize) -> Option<&str> {
        self.parts.get(idx).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        let rpath = ResourcePath::try_from("should\\format\\correctly").unwrap();

        assert_eq!("[should/format/correctly]", format!("{rpath}"));
    }
}
