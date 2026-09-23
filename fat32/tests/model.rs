//! Randomized model-based test: thousands of operations compared against an in-memory model.

#![allow(clippy::explicit_auto_deref)]

mod common;

use common::*;
use fat32::{BlockDevice, CachedDevice, Error, FileSystem, FormatOptions};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
enum Node {
    File(Vec<u8>),
    /// Children by upper-case name: (display name, node).
    Dir(BTreeMap<String, (String, Node)>),
}

fn key(name: &str) -> String {
    name.to_uppercase()
}

fn parts(path: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out
}

struct Model {
    root: Node,
}

type R<T> = Result<T, Error>;

impl Model {
    fn resolve(&self, comps: &[&str]) -> R<&Node> {
        let mut node = &self.root;
        for comp in comps {
            match node {
                Node::Dir(children) => node = &children.get(&key(comp)).ok_or(Error::NotFound)?.1,
                Node::File(_) => return Err(Error::NotADirectory),
            }
        }
        Ok(node)
    }

    fn resolve_mut(&mut self, comps: &[&str]) -> &mut Node {
        let mut node = &mut self.root;
        for comp in comps {
            match node {
                Node::Dir(children) => node = &mut children.get_mut(&key(comp)).unwrap().1,
                Node::File(_) => unreachable!(),
            }
        }
        node
    }

    /// Resolves the parent directory; returns its children map.
    fn parent(&self, comps: &[&str]) -> R<&BTreeMap<String, (String, Node)>> {
        match self.resolve(&comps[..comps.len() - 1])? {
            Node::Dir(children) => Ok(children),
            Node::File(_) => Err(Error::NotADirectory),
        }
    }

    fn parent_mut(&mut self, comps: &[&str]) -> &mut BTreeMap<String, (String, Node)> {
        match self.resolve_mut(&comps[..comps.len() - 1]) {
            Node::Dir(children) => children,
            Node::File(_) => unreachable!(),
        }
    }

    fn file_mut(&mut self, comps: &[&str]) -> &mut Vec<u8> {
        match self.resolve_mut(comps) {
            Node::File(data) => data,
            Node::Dir(_) => unreachable!(),
        }
    }

    fn resolve_file(&self, comps: &[&str]) -> R<&Vec<u8>> {
        match self.resolve(comps)? {
            Node::File(data) => Ok(data),
            Node::Dir(_) => Err(Error::IsADirectory),
        }
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> R<()> {
        let comps = parts(path);
        if comps.is_empty() {
            return Err(Error::IsADirectory);
        }
        let existing = self.parent(&comps)?.get(&key(comps[comps.len() - 1])).map(|e| matches!(e.1, Node::Dir(_)));
        match existing {
            Some(true) => Err(Error::IsADirectory),
            Some(false) => {
                *self.file_mut(&comps) = data.to_vec();
                Ok(())
            }
            None => {
                let name = comps[comps.len() - 1];
                self.parent_mut(&comps).insert(key(name), (name.to_string(), Node::File(data.to_vec())));
                Ok(())
            }
        }
    }

    fn append_file(&mut self, path: &str, data: &[u8]) -> R<()> {
        let comps = parts(path);
        if comps.is_empty() {
            return Err(Error::IsADirectory);
        }
        let existing = self.parent(&comps)?.get(&key(comps[comps.len() - 1])).map(|e| matches!(e.1, Node::Dir(_)));
        match existing {
            Some(true) => Err(Error::IsADirectory),
            Some(false) => {
                self.file_mut(&comps).extend_from_slice(data);
                Ok(())
            }
            None => self.write_file(path, data),
        }
    }

    fn write_at(&mut self, path: &str, offset: u64, data: &[u8]) -> R<()> {
        let comps = parts(path);
        self.resolve_file(&comps)?;
        if data.is_empty() {
            return Ok(());
        }
        let file = self.file_mut(&comps);
        let end = offset as usize + data.len();
        if file.len() < end {
            file.resize(end, 0);
        }
        file[offset as usize..end].copy_from_slice(data);
        Ok(())
    }

    fn truncate(&mut self, path: &str, length: u64) -> R<()> {
        let comps = parts(path);
        self.resolve_file(&comps)?;
        self.file_mut(&comps).resize(length as usize, 0);
        Ok(())
    }

    fn touch(&mut self, path: &str) -> R<()> {
        let comps = parts(path);
        if comps.is_empty() {
            return Ok(());
        }
        if self.parent(&comps)?.contains_key(&key(comps[comps.len() - 1])) {
            return Ok(());
        }
        self.write_file(path, b"")
    }

    fn create_dir(&mut self, path: &str) -> R<()> {
        let comps = parts(path);
        if comps.is_empty() {
            return Err(Error::AlreadyExists);
        }
        if self.parent(&comps)?.contains_key(&key(comps[comps.len() - 1])) {
            return Err(Error::AlreadyExists);
        }
        let name = comps[comps.len() - 1];
        self.parent_mut(&comps).insert(key(name), (name.to_string(), Node::Dir(BTreeMap::new())));
        Ok(())
    }

    fn create_dir_all(&mut self, path: &str) -> R<()> {
        let comps = parts(path);
        for i in 0..comps.len() {
            match self.resolve(&comps[..=i]) {
                Ok(Node::Dir(_)) => {}
                Ok(Node::File(_)) if i + 1 == comps.len() => return Err(Error::AlreadyExists),
                Ok(Node::File(_)) => return Err(Error::NotADirectory),
                Err(_) => self.create_dir(&format!("/{}", comps[..=i].join("/")))?,
            }
        }
        Ok(())
    }

    fn remove_file(&mut self, path: &str) -> R<()> {
        let comps = parts(path);
        self.resolve_file(&comps)?;
        self.parent_mut(&comps).remove(&key(comps[comps.len() - 1]));
        Ok(())
    }

    fn remove_dir(&mut self, path: &str, all: bool) -> R<()> {
        let comps = parts(path);
        if comps.is_empty() {
            return Err(Error::RootDirectory);
        }
        match self.resolve(&comps)? {
            Node::File(_) => return Err(Error::NotADirectory),
            Node::Dir(children) if !all && !children.is_empty() => return Err(Error::DirectoryNotEmpty),
            Node::Dir(_) => {}
        }
        self.parent_mut(&comps).remove(&key(comps[comps.len() - 1]));
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> R<()> {
        let from_comps = parts(from);
        if from_comps.is_empty() {
            return Err(Error::RootDirectory);
        }
        let source_is_dir = matches!(self.resolve(&from_comps)?, Node::Dir(_));
        let to_comps = parts(to);
        if to_comps.is_empty() {
            return Err(Error::RootDirectory);
        }
        let parent = &to_comps[..to_comps.len() - 1];
        // Walk the destination parent like the implementation does.
        for i in 0..parent.len() {
            match self.resolve(&parent[..=i])? {
                Node::Dir(_) => {}
                Node::File(_) => return Err(Error::NotADirectory),
            }
        }
        let from_keys: Vec<String> = from_comps.iter().map(|c| key(c)).collect();
        let parent_keys: Vec<String> = parent.iter().map(|c| key(c)).collect();
        if source_is_dir && parent_keys.starts_with(&from_keys) {
            return Err(Error::InvalidMove);
        }
        let name = to_comps[to_comps.len() - 1];
        let to_keys: Vec<String> = to_comps.iter().map(|c| key(c)).collect();
        let exists = self.parent(&to_comps)?.get(&key(name)).map(|e| e.0.clone());
        if let Some(display) = exists {
            if to_keys != from_keys {
                return Err(Error::AlreadyExists);
            }
            if display == name {
                return Ok(());
            }
        }
        let (_, node) = self.parent_mut(&from_comps).remove(&key(from_comps[from_comps.len() - 1])).unwrap();
        self.parent_mut(&to_comps).insert(key(name), (name.to_string(), node));
        Ok(())
    }

    fn copy_file(&mut self, from: &str, to: &str) -> R<()> {
        let data = self.resolve_file(&parts(from))?.clone();
        let comps = parts(to);
        if comps.is_empty() {
            return Err(Error::AlreadyExists);
        }
        if self.parent(&comps)?.contains_key(&key(comps[comps.len() - 1])) {
            return Err(Error::AlreadyExists);
        }
        self.write_file(to, &data)
    }

    fn tree(&self) -> Tree {
        fn walk(prefix: &str, node: &Node, out: &mut Tree) {
            if let Node::Dir(children) = node {
                for (display, child) in children.values() {
                    let path = if prefix == "/" { format!("/{}", display) } else { format!("{}/{}", prefix, display) };
                    match child {
                        Node::File(data) => {
                            out.insert(path, Some(data.clone()));
                        }
                        Node::Dir(_) => {
                            out.insert(path.clone(), None);
                            walk(&path, child, out);
                        }
                    }
                }
            }
        }
        let mut out = Tree::new();
        walk("/", &self.root, &mut out);
        out
    }

    /// Every path in the model (display names), plus the root.
    fn paths(&self, dirs_only: bool) -> Vec<String> {
        let mut out = vec![String::from("/")];
        for (path, contents) in self.tree() {
            if !dirs_only || contents.is_none() {
                out.push(path);
            }
        }
        out
    }
}

const NAMES: &[&str] = &[
    "a",
    "B",
    "file.txt",
    "File.TXT",
    "Long File Name.dat",
    "UPPER.TXT",
    "mixedCase",
    "x y z",
    "ünï.txt",
    "very long name that needs several long-name entries to be stored.bin",
    "dir",
    "Data",
    "notes.md",
    "é",
    "a.b.c",
    "semi;colon+plus",
];

fn vary_case(rng: &mut Rng, path: &str) -> String {
    match rng.below(4) {
        0 => path.to_uppercase(),
        1 => path.to_lowercase(),
        _ => path.to_string(),
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir == "/" { format!("/{}", name) } else { format!("{}/{}", dir, name) }
}

fn random_path(rng: &mut Rng, model: &Model) -> String {
    match rng.below(10) {
        // An existing path.
        0..=4 => {
            let paths = model.paths(false);
            let path = rng.pick(&paths).clone();
            vary_case(rng, &path)
        }
        // A new name in an existing directory (depth limited).
        5..=8 => {
            let dirs: Vec<String> = model.paths(true).into_iter().filter(|d| d.matches('/').count() < 4).collect();
            let dir = rng.pick(&dirs).clone();
            join(&dir, *rng.pick(NAMES))
        }
        // Something below a file or below a missing directory.
        _ => {
            let paths = model.paths(false);
            let base = rng.pick(&paths).clone();
            join(&join(&base, *rng.pick(NAMES)), *rng.pick(NAMES))
        }
    }
}

fn random_data(rng: &mut Rng, max: u64) -> Vec<u8> {
    let len = match rng.below(10) {
        0 => 0,
        1..=6 => rng.below(max / 8 + 1),
        _ => rng.below(max + 1),
    } as usize;
    pattern(len, rng.next())
}

fn verify<D: BlockDevice>(fs: &mut FileSystem<D>, model: &Model, step: usize) {
    let ours = dump_ours(fs);
    let expected = model.tree();
    if ours != expected {
        let a: Vec<&String> = ours.keys().collect();
        let b: Vec<&String> = expected.keys().collect();
        panic!("step {}: tree mismatch\nours:     {:?}\nexpected: {:?}", step, a, b);
    }
}

fn run<D: BlockDevice>(
    mut fs: FileSystem<D>,
    seed: u64,
    steps: usize,
    max_file: u64,
    remount: impl Fn(FileSystem<D>) -> FileSystem<D>,
) {
    let mut rng = Rng(seed);
    let mut model = Model { root: Node::Dir(BTreeMap::new()) };
    let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
    let mut max_entries = 0;
    for step in 0..steps {
        let op = rng.below(100);
        let (description, ours, expected): (String, R<()>, R<()>) = match op {
            0..=17 => {
                let path = random_path(&mut rng, &model);
                let data = random_data(&mut rng, max_file);
                (
                    format!("write_file {} ({} bytes)", path, data.len()),
                    fs.write_file(&path, &data),
                    model.write_file(&path, &data),
                )
            }
            18..=27 => {
                let path = random_path(&mut rng, &model);
                let data = random_data(&mut rng, max_file / 4);
                (
                    format!("append_file {} ({} bytes)", path, data.len()),
                    fs.append_file(&path, &data),
                    model.append_file(&path, &data),
                )
            }
            28..=37 => {
                let path = random_path(&mut rng, &model);
                let offset = rng.below(max_file);
                let data = random_data(&mut rng, max_file / 4);
                (
                    format!("write_at {} @{} ({} bytes)", path, offset, data.len()),
                    fs.write_at(&path, offset, &data),
                    model.write_at(&path, offset, &data),
                )
            }
            38..=44 => {
                let path = random_path(&mut rng, &model);
                let length = rng.below(max_file);
                (format!("truncate {} {}", path, length), fs.truncate(&path, length), model.truncate(&path, length))
            }
            45..=52 => {
                let path = random_path(&mut rng, &model);
                (format!("create_dir {}", path), fs.create_dir(&path), model.create_dir(&path))
            }
            53..=55 => {
                let path = random_path(&mut rng, &model);
                let deeper = join(&path, *rng.pick(NAMES));
                (format!("create_dir_all {}", deeper), fs.create_dir_all(&deeper), model.create_dir_all(&deeper))
            }
            56..=61 => {
                let path = random_path(&mut rng, &model);
                (format!("remove_file {}", path), fs.remove_file(&path), model.remove_file(&path))
            }
            62..=67 => {
                let path = random_path(&mut rng, &model);
                (format!("remove_dir {}", path), fs.remove_dir(&path), model.remove_dir(&path, false))
            }
            68..=69 => {
                let path = random_path(&mut rng, &model);
                (format!("remove_dir_all {}", path), fs.remove_dir_all(&path), model.remove_dir(&path, true))
            }
            70..=84 => {
                let from = random_path(&mut rng, &model);
                let to = if rng.chance(20) { vary_case(&mut rng, &from) } else { random_path(&mut rng, &model) };
                (format!("rename {} -> {}", from, to), fs.rename(&from, &to), model.rename(&from, &to))
            }
            85..=89 => {
                let from = random_path(&mut rng, &model);
                let to = random_path(&mut rng, &model);
                (format!("copy_file {} -> {}", from, to), fs.copy_file(&from, &to), model.copy_file(&from, &to))
            }
            90..=93 => {
                let path = random_path(&mut rng, &model);
                (format!("touch {}", path), fs.touch(&path), model.touch(&path))
            }
            _ => {
                // Reads through read_at in odd-sized chunks.
                let path = random_path(&mut rng, &model);
                let expected = model.resolve_file(&parts(&path)).cloned();
                let mut buffer = vec![0u8; 1 + rng.below(700) as usize];
                let offset = rng.below(max_file + 10);
                let ours = fs.read_at(&path, offset, &mut buffer);
                match (&ours, &expected) {
                    (Ok(n), Ok(data)) => {
                        let start = (offset as usize).min(data.len());
                        let end = (start + buffer.len()).min(data.len());
                        assert_eq!(*n, end - start, "step {}: read_at {} @{}", step, path, offset);
                        assert_eq!(&buffer[..*n], &data[start..end], "step {}: read_at {} @{}", step, path, offset);
                    }
                    (Err(a), Err(b)) => assert_eq!(a, b, "step {}: read_at {}", step, path),
                    _ => panic!(
                        "step {}: read_at {}: ours {:?}, model {:?}",
                        step,
                        path,
                        ours,
                        expected.map(|d| d.len())
                    ),
                }
                continue;
            }
        };
        assert_eq!(ours, expected, "step {}: {}", step, description);
        *outcomes.entry(format!("{:?}", ours)).or_default() += 1;
        verify(&mut fs, &model, step);
        max_entries = max_entries.max(model.tree().len());
        if step % 250 == 249 {
            assert_clean(&mut fs);
        }
        if step % 700 == 699 {
            fs = remount(fs);
            verify(&mut fs, &model, step);
        }
    }
    assert_clean(&mut fs);
    let mut fs = remount(fs);
    verify(&mut fs, &model, steps);
    assert_clean(&mut fs);
    eprintln!(
        "seed {:#x}: outcomes {:?}, largest tree {} entries, final tree {} entries",
        seed,
        outcomes,
        max_entries,
        model.tree().len()
    );
    // The run must have been meaningful: mostly successes, plus a variety of errors.
    assert!(outcomes.get("Ok(())").copied().unwrap_or(0) > steps / 3);
    assert!(outcomes.len() >= 6, "too few distinct outcomes: {:?}", outcomes);
    assert!(max_entries >= 20);
}

fn formatted(bytes: usize, options: &FormatOptions) -> RamDisk {
    let mut disk = RamDisk::new(bytes);
    fat32::format(&mut disk, options).unwrap();
    disk
}

#[test]
fn random_operations_small_clusters() {
    let fs = FileSystem::mount(formatted(34 * MIB, &FormatOptions::default())).unwrap();
    run(fs, 0x0123_4567_89AB_CDEF, 4000, 6000, |fs| FileSystem::mount(fs.unmount().unwrap()).unwrap());
}

#[test]
fn random_operations_4k_clusters() {
    let fs = FileSystem::mount(formatted(300 * MIB, &FormatOptions::default())).unwrap();
    run(fs, 0xDEAD_BEEF_F00D_0001, 3000, 20_000, |fs| FileSystem::mount(fs.unmount().unwrap()).unwrap());
}

#[test]
fn random_operations_through_a_small_write_back_cache() {
    // A tiny cache forces constant eviction; remounting goes through into_inner (flush) and a
    // brand new cache, so everything must have reached the disk.
    let disk = formatted(34 * MIB, &FormatOptions::default());
    let fs = FileSystem::mount(CachedDevice::new(disk, 24)).unwrap();
    run(fs, 42, 2500, 5000, |fs| {
        let cache = fs.unmount().unwrap();
        let disk = cache.into_inner().unwrap();
        FileSystem::mount(CachedDevice::new(disk, 24)).unwrap()
    });
}

#[test]
fn random_result_is_readable_by_fatfs() {
    let fs = FileSystem::mount(formatted(34 * MIB, &FormatOptions::default())).unwrap();
    let mut rng = Rng(7);
    let mut model = Model { root: Node::Dir(BTreeMap::new()) };
    let mut fs = fs;
    for _ in 0..1500 {
        let path = random_path(&mut rng, &model);
        match rng.below(6) {
            0 | 1 => {
                let data = random_data(&mut rng, 4000);
                assert_eq!(fs.write_file(&path, &data), model.write_file(&path, &data));
            }
            2 => assert_eq!(fs.create_dir(&path), model.create_dir(&path)),
            3 => {
                let to = random_path(&mut rng, &model);
                assert_eq!(fs.rename(&path, &to), model.rename(&path, &to));
            }
            4 => assert_eq!(fs.remove_file(&path), model.remove_file(&path)),
            _ => {
                let data = random_data(&mut rng, 1000);
                assert_eq!(fs.append_file(&path, &data), model.append_file(&path, &data));
            }
        }
    }
    let mut image = fs.unmount().unwrap().data;
    assert_eq!(dump_fatfs(&mut image), model.tree());
}
