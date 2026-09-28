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

/// Mounts `rename_base()` and returns the file system with handles to its fault injection.
fn flaky(persistent: bool) -> (FileSystem<FlakyDisk>, Rc<Cell<Option<u64>>>, Rc<Cell<u64>>) {
    let fail_at = Rc::new(Cell::new(None));
    let writes = Rc::new(Cell::new(0));
    let device = FlakyDisk { disk: rename_base(), fail_at: fail_at.clone(), writes: writes.clone(), persistent };
    (FileSystem::mount(device).unwrap(), fail_at, writes)
}

fn only_lost_clusters(problems: &[String]) -> bool {
    problems.iter().all(|problem| problem.contains("lost clusters"))
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
