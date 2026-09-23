//! Path parsing. Paths are absolute and `/`-separated; empty components and `.` are ignored and
//! `..` removes the previous component (never going above the root).

use crate::{Error, Result};
use alloc::vec::Vec;

/// Splits an absolute path into its normalized components.
pub(crate) fn components(path: &str) -> Result<Vec<&str>> {
    if !path.starts_with('/') || path.contains('\0') {
        return Err(Error::InvalidPath);
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(components("/").unwrap(), Vec::<&str>::new());
        assert_eq!(components("//a///b/").unwrap(), ["a", "b"]);
        assert_eq!(components("/a/./b/../c").unwrap(), ["a", "c"]);
        assert_eq!(components("/../..").unwrap(), Vec::<&str>::new());
        assert_eq!(components("/a/b/../../..").unwrap(), Vec::<&str>::new());
        assert_eq!(components("/a b/ c ").unwrap(), ["a b", " c "]);
        assert_eq!(components("/...").unwrap(), ["..."]);
    }

    #[test]
    fn relative_paths_rejected() {
        assert_eq!(components(""), Err(Error::InvalidPath));
        assert_eq!(components("a/b"), Err(Error::InvalidPath));
        assert_eq!(components("./a"), Err(Error::InvalidPath));
        assert_eq!(components("/a\0b"), Err(Error::InvalidPath));
    }
}
