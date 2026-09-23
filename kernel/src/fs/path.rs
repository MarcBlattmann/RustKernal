//! Path arithmetic for the shell. Pure functions on strings (no file system access), so they can
//! be tested on the host.
//!
//! Canonical paths are absolute, `/`-separated, without `.`/`..` components, empty components or a
//! trailing slash; the root is `/`.

use alloc::string::String;
use alloc::vec::Vec;

/// Resolves `path` against the working directory `cwd` (itself canonical) and returns the
/// canonical absolute path. An empty path or `~` means the root, `~/x` is `/x`, `..` at the root
/// stays at the root and repeated slashes collapse.
pub fn normalize(cwd: &str, path: &str) -> String {
    let path = path.trim();
    let (base, rest) = if path.is_empty() || path == "~" {
        ("/", "")
    } else if let Some(rest) = path.strip_prefix("~/") {
        ("/", rest)
    } else if path.starts_with('/') {
        ("/", path)
    } else {
        (cwd, path)
    };

    let mut parts: Vec<&str> = Vec::new();
    for part in base.split('/').chain(rest.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    join_parts(&parts)
}

fn join_parts(parts: &[&str]) -> String {
    if parts.is_empty() {
        return String::from("/");
    }
    let mut path = String::new();
    for part in parts {
        path.push('/');
        path.push_str(part);
    }
    path
}

/// `dir` + `/` + `name` for a canonical `dir`.
pub fn join(dir: &str, name: &str) -> String {
    let mut path = String::from(dir.trim_end_matches('/'));
    path.push('/');
    path.push_str(name);
    path
}

/// The last component of a canonical path (empty for the root).
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or("")
}

/// The directory containing a canonical path (the root is its own parent).
pub fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(index) => &path[..index],
    }
}

/// Case-insensitive equality, the way FAT compares names.
pub fn same_path(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Whether canonical `path` is `ancestor` itself or lies below it (case-insensitive).
pub fn is_within(path: &str, ancestor: &str) -> bool {
    if ancestor == "/" {
        return true;
    }
    let (path, ancestor) = (path.to_lowercase(), ancestor.to_lowercase());
    path == ancestor || path.strip_prefix(&ancestor).is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize("/", ""), "/");
        assert_eq!(normalize("/docs", "~"), "/");
        assert_eq!(normalize("/docs", "~/a/b"), "/a/b");
        assert_eq!(normalize("/docs", "a"), "/docs/a");
        assert_eq!(normalize("/docs", "./a/./b/"), "/docs/a/b");
        assert_eq!(normalize("/docs", ".."), "/");
        assert_eq!(normalize("/docs", "../../.."), "/");
        assert_eq!(normalize("/a/b", "../c"), "/a/c");
        assert_eq!(normalize("/a", "//x///y//"), "/x/y");
        assert_eq!(normalize("/a", "/"), "/");
        assert_eq!(normalize("/a", "  b  "), "/a/b");
        assert_eq!(normalize("/a", "my file.txt"), "/a/my file.txt");
        assert_eq!(normalize("/", "a/../../b"), "/b");
        assert_eq!(normalize("/", "~x"), "/~x");
    }

    #[test]
    fn splits_and_joins() {
        assert_eq!(join("/", "a"), "/a");
        assert_eq!(join("/a", "b"), "/a/b");
        assert_eq!(file_name("/a/b.txt"), "b.txt");
        assert_eq!(file_name("/"), "");
        assert_eq!(parent("/a/b"), "/a");
        assert_eq!(parent("/a"), "/");
        assert_eq!(parent("/"), "/");
    }

    #[test]
    fn compares() {
        assert!(same_path("/Docs/A.TXT", "/docs/a.txt"));
        assert!(!same_path("/docs/a", "/docs/b"));
        assert!(is_within("/a/b", "/a"));
        assert!(is_within("/A", "/a"));
        assert!(!is_within("/ab", "/a"));
        assert!(is_within("/anything", "/"));
    }
}
