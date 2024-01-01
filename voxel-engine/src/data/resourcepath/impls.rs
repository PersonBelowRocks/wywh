use std::{fmt::Write, path::Path};

use super::{
    error::{FromPathError, FromStrError},
    ResourcePath, ResourcePathPart,
};

impl<'a> TryFrom<&'a Path> for ResourcePath {
    type Error = FromPathError;

    fn try_from(value: &'a Path) -> Result<Self, Self::Error> {
        let mut buffer = value.to_str().ok_or(FromPathError::InvalidUtf8)?;

        if let Some(ext) = value.extension() {
            let ext = ext.to_str().ok_or(FromPathError::InvalidUtf8)?;
            buffer = buffer.strip_suffix(ext).unwrap();

            if let Some(dotless) = buffer.strip_suffix('.') {
                buffer = dotless;
            }
        }

        Ok(Self::try_from(buffer)?)
    }
}

impl<'a> TryFrom<&'a str> for ResourcePath {
    type Error = FromStrError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let mut parts = Vec::<ResourcePathPart>::new();

        for (i, part) in value
            .split(|ch| ch == std::path::MAIN_SEPARATOR || ch == '/' || ch == '\\')
            .enumerate()
        {
            if part.is_empty() || part.contains('.') {
                return Err(FromStrError::InvalidElement(i));
            }

            parts.push(part.to_string());
        }

        // optimize our memory footprint a little
        parts.shrink_to_fit();

        Ok(Self::from_parts(parts))
    }
}

impl std::fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char('[')?;

        let mut parts = self.parts().peekable();
        while let Some(part) = parts.next() {
            f.write_str(part)?;

            if parts.peek().is_some() {
                f.write_char('/')?;
            }
        }

        f.write_char(']')
    }
}

impl From<ResourcePath> for String {
    fn from(value: ResourcePath) -> Self {
        let mut string = String::with_capacity(value.len());
        let mut parts = value.parts().peekable();
        while let Some(part) = parts.next() {
            string.push_str(part);

            if parts.peek().is_some() {
                string.write_char('/');
            }
        }

        string.shrink_to_fit();
        string
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use super::*;

    #[test]
    fn successful_conversions() {
        let path = PathBuf::from_str("does/this\\work.questionmark").unwrap();

        let rpath = ResourcePath::try_from(path.as_path()).unwrap();

        assert_eq!(Some("does"), rpath.get_part(0));
        assert_eq!(Some("this"), rpath.get_part(1));
        assert_eq!(Some("work"), rpath.get_part(2));

        let path = PathBuf::from_str("does/this\\work.").unwrap();

        let rpath = ResourcePath::try_from(path.as_path()).unwrap();

        assert_eq!(Some("does"), rpath.get_part(0));
        assert_eq!(Some("this"), rpath.get_part(1));
        assert_eq!(Some("work"), rpath.get_part(2));

        let path = PathBuf::from_str("work.questionmark").unwrap();

        let rpath = ResourcePath::try_from(path.as_path()).unwrap();

        assert_eq!(Some("work"), rpath.get_part(0));

        let path = PathBuf::from_str("does/this\\work").unwrap();

        let rpath = ResourcePath::try_from(path.as_path()).unwrap();

        assert_eq!(Some("does"), rpath.get_part(0));
        assert_eq!(Some("this"), rpath.get_part(1));
        assert_eq!(Some("work"), rpath.get_part(2));
    }

    #[test]
    fn failed_conversions() {
        let path = PathBuf::from_str("does/this//work.questionmark").unwrap();

        assert_eq!(
            Err(FromPathError::FromStrError(FromStrError::InvalidElement(2))),
            ResourcePath::try_from(path.as_path())
        );
    }
}
