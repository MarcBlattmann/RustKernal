//! Path arithmetic for the shell. Pure functions on strings (no file system access), so they can
//! be tested on the host.
//!
//! Canonical paths are absolute, `/`-separated, without `.`/`..` components, empty components or a
//! trailing slash; the root is `/`.

use alloc::string::String;
use alloc::vec::Vec;

/// Resolves `path` against the working directory `cwd` (itself canonical) and returns the
/// canonical absolute path. `~` means the root, `~/x` is `/x`, `..` at the root stays at the root
/// and repeated slashes collapse. Spaces are part of names (the command line parser already
/// removed the unquoted ones), so ` x` names a file whose name starts with a space.
///
/// An empty path names nothing: it stays empty, which is not an absolute path, so every file
/// system call rejects it (`invalid path`) instead of acting on the root or the working directory.
pub fn normalize(cwd: &str, path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let (base, rest) = if path == "~" {
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

/// Whether a path argument as typed must name a directory: it ends with `/` or `/.` (like
/// `archive/`), as on Unix.
pub fn names_directory(argument: &str) -> bool {
    argument.len() > 1 && (argument.ends_with('/') || argument.ends_with("/."))
}

/// The last component of a path argument as typed, ignoring trailing slashes (`a/b/` -> `b`).
pub fn last_component(argument: &str) -> &str {
    file_name(argument.trim_end_matches('/'))
}

/// Whether `path` is `ancestor` itself or lies below it. Both must be spelled exactly as stored
/// on disk (see the shell's `canonical_case`), so the comparison is exact.
pub fn is_within(path: &str, ancestor: &str) -> bool {
    if ancestor == "/" {
        return true;
    }
    path == ancestor || path.strip_prefix(ancestor).is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize("/", ""), "");
        assert_eq!(normalize("/docs", ""), "");
        assert_eq!(normalize("/docs", "~"), "/");
        assert_eq!(normalize("/docs", "~/a/b"), "/a/b");
        assert_eq!(normalize("/docs", "a"), "/docs/a");
        assert_eq!(normalize("/docs", "./a/./b/"), "/docs/a/b");
        assert_eq!(normalize("/docs", ".."), "/");
        assert_eq!(normalize("/docs", "../../.."), "/");
        assert_eq!(normalize("/a/b", "../c"), "/a/c");
        assert_eq!(normalize("/a", "//x///y//"), "/x/y");
        assert_eq!(normalize("/a", "/"), "/");
        assert_eq!(normalize("/a", " b"), "/a/ b");
        assert_eq!(normalize("/a", "b "), "/a/b ");
        assert_eq!(normalize("/a", " "), "/a/ ");
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
        assert!(is_within("/a/b", "/a"));
        assert!(is_within("/a", "/a"));
        assert!(!is_within("/ab", "/a"));
        assert!(is_within("/anything", "/"));
    }

    #[test]
    fn directory_arguments() {
        assert!(names_directory("archive/"));
        assert!(names_directory("a/b/"));
        assert!(names_directory("dir/."));
        assert!(!names_directory("/"));
        assert!(!names_directory("file.txt"));
        assert!(!names_directory("."));
        assert_eq!(last_component("a/b/"), "b");
        assert_eq!(last_component(".."), "..");
        assert_eq!(last_component("x/.."), "..");
        assert_eq!(last_component("./"), ".");
    }
}
