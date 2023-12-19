use std::{fmt::Write, path::Path, str::FromStr};

use ascii::{AsciiChar, AsciiStr, AsciiString};
use itertools::Itertools;

#[derive(Clone, Debug, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResourcePath(AsciiString);

impl ResourcePath {
    pub fn new(path: &Path) -> Option<Self> {
        let string = AsciiString::from_str(path.to_str()?).ok()?;
        Some(Self(string))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn path(&self) -> &Path {
        Path::new(self.0.as_str())
    }

    pub fn extension(&self) -> Option<&AsciiStr> {
        let idx = self
            .0
            .chars()
            .rev()
            .find_position(|ch| matches!(ch, AsciiChar::Dot))?
            .0;
        Some(&self.0[(self.0.len() - idx)..self.0.len()])
    }
}

impl std::fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char('[')?;
        f.write_str(self.0.as_str())?;
        f.write_char(']')
    }
}
