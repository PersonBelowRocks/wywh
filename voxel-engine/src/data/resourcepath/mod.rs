use std::{fmt::Write, path::Path, str::FromStr};

use ascii::{AsciiChar, AsciiStr, AsciiString};
use itertools::Itertools;

use self::error::FromStrError;

pub mod error;
pub mod impls;
pub mod serde;

pub type ResourcePathPart = String;

pub fn rpath(s: &str) -> ResourcePath {
    ResourcePath::parse(s).unwrap()
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ResourcePath {
    parts: Vec<ResourcePathPart>,
}

impl ResourcePath {
    pub(self) fn from_parts(parts: Vec<ResourcePathPart>) -> Self {
        Self { parts }
    }

    pub fn parse(s: &str) -> Result<Self, FromStrError> {
        let mut parts = Vec::new();

        for (i, part) in s.split('.').enumerate() {
            if part.is_empty() {
                return Err(FromStrError::InvalidElement(i));
            }
            parts.push(part.to_string());
        }

        Ok(Self::from_parts(parts))
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

    pub fn string(&self) -> String {
        let mut string = String::with_capacity(self.len());
        let mut parts = self.parts().peekable();
        while let Some(part) = parts.next() {
            string.push_str(part);

            if parts.peek().is_some() {
                string.push('.');
            }
        }
        string
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        let rpath = ResourcePath::parse("should.format.correctly").unwrap();

        assert_eq!("[should.format.correctly]", format!("{rpath}"));
    }
}
