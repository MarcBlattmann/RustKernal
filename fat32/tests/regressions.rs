//! Regression tests for review findings: name comparison, rename rollback, timestamps and name
//! length errors.

mod common;

use common::*;
use fat32::{BlockDevice, DeviceError, Error, FileSystem, Timestamp};
use std::cell::Cell;
use std::rc::Rc;

const SMALL: usize = 34 * MIB;

#[test]
fn names_differing_only_by_full_case_mapping_are_distinct() {
    let mut fs = fresh(SMALL);
    fs.write_file("/straße.txt", b"original").unwrap();
    fs.write_file("/strasse.txt", b"other file").unwrap();
    assert_eq!(fs.read_file("/straße.txt").unwrap(), b"original");
    assert_eq!(fs.read_file("/strasse.txt").unwrap(), b"other file");
    // ASCII and one-to-one case mapping still match.
    assert_eq!(fs.read_file("/STRASSE.TXT").unwrap(), b"other file");
    assert_eq!(fs.read_file("/STRAßE.TXT").unwrap(), b"original");
    assert_eq!(fs.create_dir("/STRASSE.TXT"), Err(Error::AlreadyExists));

    fs.write_file("/\u{FB01}le", b"ligature").unwrap();
    assert!(!fs.exists("/FILE").unwrap());
    assert!(!fs.exists("/file").unwrap());
    fs.write_file("/file", b"plain").unwrap();
    assert_eq!(fs.read_file("/\u{FB01}le").unwrap(), b"ligature");
    assert_eq!(fs.read_file("/FILE").unwrap(), b"plain");

    let names: Vec<String> = fs.read_dir("/").unwrap().into_iter().map(|m| m.name).collect();
    for name in ["straße.txt", "strasse.txt", "\u{FB01}le", "file"] {
        assert!(names.iter().any(|n| n == name), "{} missing from {:?}", name, names);
    }
    // check() compares names the same way: no false "duplicate name".
    assert_clean(&mut fs);
}

/// A RAM disk whose writes fail from the `fail_at`-th write on (counted by `writes`): only that
/// one write, or every later one too when `persistent` is set.
struct FlakyDisk {
    disk: RamDisk,
    fail_at: Rc<Cell<Option<u64>>>,
    writes: Rc<Cell<u64>>,
    persistent: bool,
}

impl BlockDevice for FlakyDisk {
    fn sector_count(&self) -> u64 {
        self.disk.sector_count()
    }
    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        self.disk.read_sectors(lba, buffer)
    }
    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        let index = self.writes.get();
        self.writes.set(index + 1);
        if let Some(fail_at) = self.fail_at.get()
            && (index == fail_at || (self.persistent && index > fail_at))
        {
            return Err(DeviceError::Io);
        }
        self.disk.write_sectors(lba, buffer)
    }
    fn flush(&mut self) -> Result<(), DeviceError> {
        self.disk.flush()
    }
}

fn rename_base() -> RamDisk {
    let mut fs = fresh(SMALL);
    fs.create_dir("/a").unwrap();
    fs.write_file("/a/f.txt", &pattern(5000, 1)).unwrap();
    fs.create_dir("/b").unwrap();
    fs.write_file("/plain.txt", &pattern(3000, 2)).unwrap();
    fs.unmount().unwrap()
}

/// A mounted `FlakyDisk` with handles to its fault injection (`fail_at`, `writes`).
type Flaky = (FileSystem<FlakyDisk>, Rc<Cell<Option<u64>>>, Rc<Cell<u64>>);

/// Mounts `rename_base()` and returns the file system with handles to its fault injection.
fn flaky(persistent: bool) -> Flaky {
    flaky_on(rename_base(), persistent)
}

fn flaky_on(disk: RamDisk, persistent: bool) -> Flaky {
    let fail_at = Rc::new(Cell::new(None));
    let writes = Rc::new(Cell::new(0));
    let device = FlakyDisk { disk, fail_at: fail_at.clone(), writes: writes.clone(), persistent };
    (FileSystem::mount(device).unwrap(), fail_at, writes)
}

fn only_lost_clusters(problems: &[String]) -> bool {
    problems.iter().all(|problem| problem.contains("lost clusters"))
}

/// Space leaks only: nothing is cross-linked, duplicated or unreachable.
fn only_leaks(problems: &[String]) -> bool {
    problems.iter().all(|problem| problem.contains("lost clusters") || problem.contains("orphaned long-name entries"))
}

/// Number of entries named like the last component of any of `paths` (ignoring case) in the
/// paths' parent directories: how many names the renamed data has.
fn names_of<D: BlockDevice>(fs: &mut FileSystem<D>, paths: &[&str]) -> usize {
    let mut parents: Vec<(&str, Vec<&str>)> = Vec::new();
    for path in paths {
        let (parent, name) = path.rsplit_once('/').unwrap();
        let parent = if parent.is_empty() { "/" } else { parent };
        match parents.iter_mut().find(|p| p.0 == parent) {
            Some(p) => p.1.push(name),
            None => parents.push((parent, vec![name])),
        }
    }
    let mut count = 0;
    for (parent, names) in parents {
        for entry in fs.read_dir(parent).unwrap() {
            if names.iter().any(|name| name.eq_ignore_ascii_case(&entry.name)) {
                count += 1;
            }
        }
    }
    count
}

const LONG_SOURCE: &str = "/d/a long name for the source file.txt";

/// `/d` holds '.', '..' and 13 upper-case 8.3 names (15 slots, no long-name entries), so the long-name entries of
/// `LONG_SOURCE` start in the directory's first sector and its short entry lands in the second:
/// deleting it takes two sector writes, the short entry's sector first.
fn long_name_base() -> RamDisk {
    let mut fs = fresh(SMALL);
    fs.create_dir("/d").unwrap();
    for i in 0..13 {
        fs.write_file(&format!("/d/F{}.TXT", i), b"x").unwrap();
    }
    fs.write_file(LONG_SOURCE, &pattern(6000, 3)).unwrap();
    fs.create_dir("/e").unwrap();
    fs.write_file("/plain.txt", &pattern(3000, 2)).unwrap();
    fs.unmount().unwrap()
}

#[test]
fn failed_rename_is_rolled_back() {
    for (from, to, inside, expected) in
        [("/a", "/b/a", "/f.txt", pattern(5000, 1)), ("/plain.txt", "/b/moved plain.txt", "", pattern(3000, 2))]
    {
        let mut completed = false;
        for offset in 0..60 {
            let (mut fs, fail_at, writes) = flaky(false);
            fail_at.set(Some(writes.get() + offset));
            let result = fs.rename(from, to);
            fail_at.set(None);
            let (old, new) = (fs.exists(from).unwrap(), fs.exists(to).unwrap());
            match &result {
                Ok(()) => assert!(!old && new, "{} -> {}: renamed but old={} new={}", from, to, old, new),
                Err(_) => assert!(old && !new, "{} -> {}, write {} failed: old={} new={}", from, to, offset, old, new),
            }
            let path = if new { to } else { from };
            assert_eq!(fs.read_file(&format!("{}{}", path, inside)).unwrap(), expected);
            let report = fs.check().unwrap();
            assert!(only_lost_clusters(&report.problems), "{} -> {}, write {} failed: {:?}", from, to, offset, report.problems);
            // The volume stays usable after a rolled-back rename.
            fs.write_file("/after.txt", b"ok").unwrap();
            let disk = fs.unmount().unwrap().disk;
            let mut fs = FileSystem::mount(disk).unwrap();
            assert_eq!(fs.exists(to).unwrap(), new);
            if result.is_ok() {
                completed = true;
                break;
            }
        }
        assert!(completed, "{} -> {} never completed", from, to);
    }
}

#[test]
fn rename_that_cannot_be_undone_blocks_further_changes() {
    // Writes keep failing once the new entry is written, so the rollback fails too. The volume
    // must then refuse changes: `rm -r` of either name would free clusters the other still uses.
    let mut saw_blocked = false;
    for offset in 0..60 {
        let (mut fs, fail_at, writes) = flaky(true);
        fail_at.set(Some(writes.get() + offset));
        let result = fs.rename("/a", "/b/a");
        fail_at.set(None);
        if result.is_ok() {
            break;
        }
        if fs.exists("/a").unwrap() && fs.exists("/b/a").unwrap() {
            saw_blocked = true;
            assert!(matches!(fs.remove_dir_all("/a"), Err(Error::Corrupt(_))));
            assert!(matches!(fs.write_file("/x", b"x"), Err(Error::Corrupt(_))));
            assert_eq!(fs.read_file("/b/a/f.txt").unwrap(), pattern(5000, 1));
        } else {
            // Rolled back, or the new entry's own write failed (it may or may not be on disk, so
            // the volume may be blocked conservatively). Anything allowed leaves it consistent.
            match fs.remove_dir_all("/a") {
                Ok(()) => {
                    let report = fs.check().unwrap();
                    assert!(only_lost_clusters(&report.problems), "write {} failed: {:?}", offset, report.problems);
                }
                Err(Error::Corrupt(_)) => {}
                Err(error) => panic!("write {} failed: rm -r: {:?}", offset, error),
            }
        }
    }
    assert!(saw_blocked, "no failure point left both names behind");
}

#[test]
fn rename_failing_after_the_old_short_entry_is_gone_keeps_the_new_name() {
    // A write failing between the two sector writes that delete the old entry used to roll the
    // rename back although the old short entry was already gone: neither name survived.
    for persistent in [false, true] {
        // Whether a failure point hit the gap: the rename then took effect despite the error.
        let mut saw_split = false;
        for offset in 0..60 {
            let (mut fs, fail_at, writes) = flaky_on(long_name_base(), persistent);
            let start = writes.get();
            fail_at.set(Some(start + offset));
            let result = fs.rename(LONG_SOURCE, "/e/moved.txt");
            let failed = writes.get() > start + offset;
            fail_at.set(None);
            let (old, new) = (fs.exists(LONG_SOURCE).unwrap(), fs.exists("/e/moved.txt").unwrap());
            let what =
                format!("write {} failed (persistent: {}): {:?} old={} new={}", offset, persistent, result, old, new);
            assert!(old || new, "{}: the file lost every name", what);
            if result.is_ok() {
                assert!(!old && new, "{}", what);
            }
            // A single failed write in the gap is recovered from (the rename completes); with
            // writes failing for good, only the new name is left and the error is reported.
            saw_split |= if persistent { result.is_err() && !old } else { result.is_ok() && failed };
            let path = if new { "/e/moved.txt" } else { LONG_SOURCE };
            assert_eq!(fs.read_file(path).unwrap(), pattern(6000, 3), "{}", what);
            let disk = fs.unmount().unwrap().disk;
            // After a remount exactly one name is left and nothing is cross-linked.
            let mut fs = FileSystem::mount(disk).unwrap();
            assert_eq!(names_of(&mut fs, &[LONG_SOURCE, "/e/moved.txt"]), 1, "{}: after remount", what);
            let new = fs.exists("/e/moved.txt").unwrap();
            let report = fs.check().unwrap();
            assert!(only_leaks(&report.problems), "{}: {:?}", what, report.problems);
            fs.remove_file(if new { "/e/moved.txt" } else { LONG_SOURCE }).unwrap();
            assert_eq!(fs.read_file("/plain.txt").unwrap(), pattern(3000, 2), "{}", what);
            let report = fs.check().unwrap();
            assert!(only_leaks(&report.problems), "{}: after rm: {:?}", what, report.problems);
            if result.is_ok() {
                break;
            }
        }
        assert!(saw_split, "persistent: {}: no failure point hit the gap between the deletion writes", persistent);
    }
}

#[test]
fn mount_after_a_cut_off_rename_removes_the_duplicate() {
    // The power fails at every point of a rename (nothing after the failing write reaches the
    // disk). The next mount must leave exactly one name for the data, so removing it never frees
    // clusters another entry still uses.
    for (from, to, inside, expected) in [
        ("/a", "/b/a", "/f.txt", pattern(5000, 1)),
        ("/a", "/A", "/f.txt", pattern(5000, 1)),
        ("/plain.txt", "/b/moved plain.txt", "", pattern(3000, 2)),
    ] {
        let mut saw_both = false;
        for offset in 0..60 {
            let (mut fs, fail_at, writes) = flaky(true);
            fail_at.set(Some(writes.get() + offset));
            let result = fs.rename(from, to);
            let crashed = fs.device().disk.clone();
            drop(fs);
            let what = format!("{} -> {}, power lost at write {}", from, to, offset);
            // Before the repair: a read-only look at the image.
            let mut image = crashed.clone();
            image.read_only = true;
            let mut view = FileSystem::mount(image).unwrap();
            saw_both |= names_of(&mut view, &[from, to]) > 1;
            drop(view);

            let mut fs = FileSystem::mount(crashed).unwrap();
            assert_eq!(names_of(&mut fs, &[from, to]), 1, "{}: {:?}", what, fs.read_dir("/").unwrap());
            let path = if result.is_ok() || !fs.exists(from).unwrap() { to } else { from };
            assert_eq!(fs.read_file(&format!("{}{}", path, inside)).unwrap(), expected, "{}", what);
            let report = fs.check().unwrap();
            assert!(only_leaks(&report.problems), "{}: {:?}", what, report.problems);
            if inside.is_empty() { fs.remove_file(path).unwrap() } else { fs.remove_dir_all(path).unwrap() }
            fs.write_file("/new.bin", &pattern(20_000, 7)).unwrap();
            let report = fs.check().unwrap();
            assert!(only_leaks(&report.problems), "{}: after rm: {:?}", what, report.problems);
            assert_eq!(fs.read_file("/new.bin").unwrap(), pattern(20_000, 7), "{}", what);
            if result.is_ok() {
                break;
            }
        }
        assert!(saw_both, "{} -> {}: no crash point left both names on disk", from, to);
    }
}

#[test]
fn poisoned_volume_is_repaired_by_the_next_mount() {
    let mut saw_blocked = false;
    for offset in 0..60 {
        let (mut fs, fail_at, writes) = flaky(true);
        fail_at.set(Some(writes.get() + offset));
        let result = fs.rename("/a", "/b/a");
        fail_at.set(None);
        if result.is_ok() {
            break;
        }
        if !(fs.exists("/a").unwrap() && fs.exists("/b/a").unwrap()) {
            continue;
        }
        saw_blocked = true;
        // The device works again, but the volume stays blocked and is not marked clean.
        let disk = fs.unmount().unwrap().disk;
        let mut fs = FileSystem::mount(disk).unwrap();
        assert!(!fs.was_cleanly_unmounted(), "write {}: a poisoned volume was marked clean", offset);
        assert_eq!(names_of(&mut fs, &["/a", "/b/a"]), 1, "write {}", offset);
        let path = if fs.exists("/a").unwrap() { "/a" } else { "/b/a" };
        assert_eq!(fs.read_file(&format!("{}/f.txt", path)).unwrap(), pattern(5000, 1));
        fs.remove_dir_all(path).unwrap();
        let report = fs.check().unwrap();
        assert!(only_leaks(&report.problems), "write {}: {:?}", offset, report.problems);
        assert_eq!(fs.read_file("/plain.txt").unwrap(), pattern(3000, 2));
    }
    assert!(saw_blocked, "no failure point left both names behind");
}

#[test]
fn new_file_is_never_modified_before_it_was_created() {
    fn at_odd_second() -> Timestamp {
        Timestamp { year: 2026, month: 9, day: 23, hour: 19, minute: 29, second: 3 }
    }
    let mut fs = fresh(SMALL);
    fs.set_time_source(at_odd_second);
    fs.write_file("/new.txt", b"x").unwrap();
    let metadata = fs.metadata("/new.txt").unwrap();
    assert!(metadata.created <= metadata.modified, "{:?} > {:?}", metadata.created, metadata.modified);
    assert_eq!(metadata.modified.second, 2);
}

#[test]
fn too_long_names_are_reported_as_such() {
    let mut fs = fresh(SMALL);
    let name = format!("/{}", "n".repeat(256));
    assert_eq!(fs.write_file(&name, b""), Err(Error::NameTooLong));
    assert_eq!(fs.create_dir(&name), Err(Error::NameTooLong));
    assert_eq!(fs.touch(&name), Err(Error::NameTooLong));
    assert_eq!(fat32::validate_name(&"n".repeat(255)), Ok(()));
    assert_eq!(fat32::validate_name(&"n".repeat(256)), Err(Error::NameTooLong));
    assert_eq!(fat32::validate_name("trailing."), Err(Error::InvalidName));
    assert_eq!(fat32::validate_name("a:b"), Err(Error::InvalidName));
    assert_eq!(fat32::validate_name(" leading space"), Ok(()));
}

#[test]
fn names_are_never_silently_changed() {
    // A leading space is part of the name and kept as typed; a trailing space or dot is rejected
    // (Windows would strip it), never dropped.
    let mut fs = fresh(SMALL);
    fs.touch("/ lead").unwrap();
    assert!(fs.exists("/ lead").unwrap());
    assert!(!fs.exists("/lead").unwrap());
    let names: Vec<String> = fs.read_dir("/").unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(names, vec![String::from(" lead")]);
    assert_eq!(fs.touch("/trailsp "), Err(Error::InvalidName));
    assert_eq!(fs.touch("/trail."), Err(Error::InvalidName));
    assert_eq!(fs.create_dir("/dir."), Err(Error::InvalidName));
    assert_eq!(fs.write_file("/file ", b"x"), Err(Error::InvalidName));
    assert_eq!(fs.read_dir("/").unwrap().len(), 1);
    assert_clean(&mut fs);
}
