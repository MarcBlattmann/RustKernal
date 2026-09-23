//! Cross-checks against the independent `fatfs` crate.

mod common;

use common::*;
use fat32::{FileSystem, FormatOptions};
use std::io::Cursor;

/// A varied tree: nested directories, long/unicode/case-sensitive names, empty and multi-cluster
/// files, a directory with hundreds of entries and similar long names.
/// fatfs cannot create names outside the Basic Multilingual Plane, so `astral` is off for
/// trees fatfs writes.
fn sample_tree(big: usize, astral: bool) -> Tree {
    let mut tree = Tree::new();
    let dir = |tree: &mut Tree, path: &str| {
        tree.insert(path.to_string(), None);
    };
    let file = |tree: &mut Tree, path: &str, len: usize, seed: u64| {
        tree.insert(path.to_string(), Some(pattern(len, seed)));
    };
    dir(&mut tree, "/docs");
    file(&mut tree, "/docs/readme.txt", 120, 1);
    file(&mut tree, "/docs/Report 2026 Final Version.docx", 50_000, 2);
    file(&mut tree, "/docs/ünïcödé ✓.txt", 999, 3);
    let notes = if astral { "/docs/emoji 😀 notes.md" } else { "/docs/notes ✓ ☃.md" };
    file(&mut tree, notes, 4097, 4);
    dir(&mut tree, "/src");
    file(&mut tree, "/src/main.rs", 2048, 5);
    dir(&mut tree, "/src/lib");
    dir(&mut tree, "/src/lib/deep");
    dir(&mut tree, "/src/lib/deep/deeper");
    file(&mut tree, "/src/lib/deep/deeper/blob.bin", big, 6);
    dir(&mut tree, "/empty dir");
    file(&mut tree, "/EMPTY.TXT", 0, 7);
    file(&mut tree, "/UPPER.TXT", 10, 8);
    file(&mut tree, "/lower.txt", 11, 9);
    file(&mut tree, "/Mixed.Txt", 12, 10);
    file(&mut tree, "/.hidden", 13, 11);
    file(&mut tree, "/archive.tar.gz", 70_000, 12);
    file(&mut tree, "/a+b=c;d,e[f].txt", 14, 13);
    dir(&mut tree, "/many");
    for i in 0..250 {
        file(&mut tree, &format!("/many/file number {} with a long name.dat", i), (i * 37) % 2000, 100 + i as u64);
    }
    dir(&mut tree, "/sim");
    for i in 1..=20 {
        file(&mut tree, &format!("/sim/long file name {}.txt", i), i, 400 + i as u64);
    }
    tree
}

fn write_tree_ours<D: fat32::BlockDevice>(fs: &mut FileSystem<D>, tree: &Tree) {
    for (path, contents) in tree {
        match contents {
            None => fs.create_dir(path).unwrap(),
            Some(data) => fs.write_file(path, data).unwrap(),
        }
    }
}

#[test]
fn ours_write_fatfs_reads() {
    for (size, big) in [(64 * MIB, 1 << 20), (300 * MIB, 3 << 20)] {
        let mut disk = RamDisk::new(size);
        fat32::format(&mut disk, &FormatOptions::default()).unwrap();
        let mut fs = FileSystem::mount(disk).unwrap();
        let mut tree = sample_tree(big, true);
        write_tree_ours(&mut fs, &tree);
        // Some churn: deletions, renames, appends and truncation.
        for i in (0..250).step_by(3) {
            let path = format!("/many/file number {} with a long name.dat", i);
            fs.remove_file(&path).unwrap();
            tree.remove(&path);
        }
        fs.rename("/src/lib", "/docs/lib moved").unwrap();
        let moved: Vec<(String, Option<Vec<u8>>)> =
            tree.iter().filter(|(p, _)| p.starts_with("/src/lib")).map(|(p, c)| (p.clone(), c.clone())).collect();
        for (path, contents) in moved {
            tree.remove(&path);
            tree.insert(path.replacen("/src/lib", "/docs/lib moved", 1), contents);
        }
        fs.append_file("/lower.txt", b" appended").unwrap();
        tree.get_mut("/lower.txt").unwrap().as_mut().unwrap().extend_from_slice(b" appended");
        fs.truncate("/archive.tar.gz", 5000).unwrap();
        tree.get_mut("/archive.tar.gz").unwrap().as_mut().unwrap().truncate(5000);
        fs.rename("/Mixed.Txt", "/MIXED.TXT").unwrap();
        let contents = tree.remove("/Mixed.Txt").unwrap();
        tree.insert("/MIXED.TXT".into(), contents);
        assert_clean(&mut fs);
        assert_eq!(dump_ours(&mut fs), tree);
        let free = fs.stats().unwrap().free_clusters;

        let disk = fs.unmount().unwrap();
        let mut image = disk.data;
        assert_eq!(dump_fatfs(&mut image), tree, "fatfs sees a different tree ({} MiB)", size / MIB);
        let mut cursor = Cursor::new(&mut image);
        let other = fatfs::FileSystem::new(&mut cursor, fatfs::FsOptions::new()).unwrap();
        assert_eq!(other.stats().unwrap().free_clusters(), free);
        assert_eq!(other.volume_label(), "RUSTKERNAL");
        assert_eq!(other.read_volume_label_from_root_dir().unwrap().as_deref(), Some("RUSTKERNAL"));
        assert!(!other.read_status_flags().unwrap().dirty(), "clean after unmount");
        assert_eq!(other.volume_id(), 0x5255_5354);
        {
            let root = other.root_dir();
            let short: Vec<String> = root
                .open_dir("sim")
                .unwrap()
                .iter()
                .map(|e| e.unwrap().short_file_name())
                .filter(|n| n != "." && n != "..")
                .collect();
            assert!(short.contains(&"LONGFI~1.TXT".to_string()), "{:?}", short);
            let mut sorted = short.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), 20);
        }
        other.unmount().unwrap();
    }
}

#[test]
fn fatfs_writes_ours_reads() {
    for (size, cluster) in [(128 * MIB, None), (300 * MIB, Some(4096u32))] {
        let mut image = vec![0u8; size];
        {
            let mut cursor = Cursor::new(&mut image);
            let mut options =
                fatfs::FormatVolumeOptions::new().fat_type(fatfs::FatType::Fat32).volume_label(*b"FATFS VOL  ");
            if let Some(bytes) = cluster {
                options = options.bytes_per_cluster(bytes);
            }
            fatfs::format_volume(&mut cursor, options).unwrap();
        }
        let tree = sample_tree(2 << 20, false);
        write_tree_fatfs(&mut image, &tree);
        let mut fs = FileSystem::mount(RamDisk::from_vec(image)).unwrap();
        assert_eq!(dump_ours(&mut fs), tree);
        assert_clean(&mut fs);
        let stats = fs.stats().unwrap();
        assert_eq!(stats.label, "FATFS VOL");
        if let Some(bytes) = cluster {
            assert_eq!(stats.cluster_size, bytes);
        }
        // Lookups by short name and case-insensitively work on fatfs-made entries.
        assert!(fs.exists("/SIM/LONGFI~1.TXT").unwrap());
        assert_eq!(fs.read_file("/DOCS/README.TXT").unwrap(), tree["/docs/readme.txt"].clone().unwrap());
    }
}

#[test]
fn mixed_fatfs_ours_fatfs() {
    let mut image = fatfs_format(128 * MIB, b"MIXED      ");
    let mut tree = sample_tree(1 << 20, false);
    write_tree_fatfs(&mut image, &tree);

    // Ours modifies the fatfs-made volume.
    let mut fs = FileSystem::mount(RamDisk::from_vec(image)).unwrap();
    fs.remove_dir_all("/many").unwrap();
    tree.retain(|path, _| !path.starts_with("/many"));
    fs.remove_file("/docs/readme.txt").unwrap();
    tree.remove("/docs/readme.txt");
    fs.rename("/sim", "/src/sim renamed").unwrap();
    let moved: Vec<(String, Option<Vec<u8>>)> =
        tree.iter().filter(|(p, _)| p.starts_with("/sim")).map(|(p, c)| (p.clone(), c.clone())).collect();
    for (path, contents) in moved {
        tree.remove(&path);
        tree.insert(path.replacen("/sim", "/src/sim renamed", 1), contents);
    }
    fs.append_file("/src/main.rs", &pattern(10_000, 77)).unwrap();
    tree.get_mut("/src/main.rs").unwrap().as_mut().unwrap().extend_from_slice(&pattern(10_000, 77));
    fs.write_at("/UPPER.TXT", 3000, b"far away").unwrap();
    {
        let data = tree.get_mut("/UPPER.TXT").unwrap().as_mut().unwrap();
        data.resize(3000, 0);
        data.extend_from_slice(b"far away");
    }
    fs.create_dir_all("/new/nested dir/level three").unwrap();
    for name in ["/new", "/new/nested dir", "/new/nested dir/level three"] {
        tree.insert(name.into(), None);
    }
    for i in 0..60 {
        let path = format!("/new/nested dir/level three/created by ours {}.txt", i);
        let data = pattern(i * 111, i as u64);
        fs.write_file(&path, &data).unwrap();
        tree.insert(path, Some(data));
    }
    fs.copy_file("/archive.tar.gz", "/new/archive copy.tar.gz").unwrap();
    tree.insert("/new/archive copy.tar.gz".into(), tree["/archive.tar.gz"].clone());
    fs.truncate("/src/lib/deep/deeper/blob.bin", 12345).unwrap();
    tree.get_mut("/src/lib/deep/deeper/blob.bin").unwrap().as_mut().unwrap().truncate(12345);
    assert_clean(&mut fs);
    assert_eq!(dump_ours(&mut fs), tree);
    let disk = fs.unmount().unwrap();

    // fatfs reads it back.
    let mut image = disk.data;
    assert_eq!(dump_fatfs(&mut image), tree);

    // fatfs modifies again; ours reads and checks.
    {
        let mut cursor = Cursor::new(&mut image);
        let other = fatfs::FileSystem::new(&mut cursor, fatfs::FsOptions::new()).unwrap();
        {
            let root = other.root_dir();
            root.remove("new/nested dir/level three/created by ours 5.txt").unwrap();
            let mut file = root.create_file("new/from fatfs again.txt").unwrap();
            std::io::Write::write_all(&mut file, b"second round").unwrap();
        }
        other.unmount().unwrap();
    }
    tree.remove("/new/nested dir/level three/created by ours 5.txt");
    tree.insert("/new/from fatfs again.txt".into(), Some(b"second round".to_vec()));
    let mut fs = FileSystem::mount(RamDisk::from_vec(image)).unwrap();
    assert_eq!(dump_ours(&mut fs), tree);
    assert_clean(&mut fs);
}

#[test]
fn fatfs_sees_our_timestamps_and_attributes() {
    let mut fs = FileSystem::mount({
        let mut disk = RamDisk::new(64 * MIB);
        fat32::format(&mut disk, &FormatOptions::default()).unwrap();
        disk
    })
    .unwrap();
    fs.set_time_source(|| fat32::Timestamp { year: 2029, month: 11, day: 30, hour: 22, minute: 58, second: 47 });
    fs.write_file("/stamped.txt", b"t").unwrap();
    fs.create_dir("/stamped dir").unwrap();
    let mut image = fs.unmount().unwrap().data;
    let mut cursor = Cursor::new(&mut image);
    let other = fatfs::FileSystem::new(&mut cursor, fatfs::FsOptions::new()).unwrap();
    {
        let root = other.root_dir();
        for entry in root.iter() {
            let entry = entry.unwrap();
            let created = entry.created();
            let modified = entry.modified();
            assert_eq!((created.date.year, created.date.month, created.date.day), (2029, 11, 30));
            assert_eq!((created.time.hour, created.time.min, created.time.sec), (22, 58, 47));
            assert_eq!((modified.date.year, modified.date.month, modified.date.day), (2029, 11, 30));
            assert_eq!((modified.time.hour, modified.time.min, modified.time.sec), (22, 58, 46));
            if entry.file_name() == "stamped dir" {
                assert!(entry.is_dir());
                let names: Vec<String> = entry.to_dir().iter().map(|e| e.unwrap().file_name()).collect();
                assert_eq!(names, [".", ".."]);
            }
        }
    }
    other.unmount().unwrap();
}
