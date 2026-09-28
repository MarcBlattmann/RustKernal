//! Power-loss simulation: every mutating operation is cut off after each possible number of
//! device writes, and the disk as it was at that moment must mount, pass `check()` apart from
//! harmless leftovers (lost clusters, over-long chains, orphaned long-name entries) and keep every
//! file the operation did not touch intact. A rename cut off while both names exist is repaired by
//! that mount, so no cross-linked or duplicated entry is ever accepted.

mod common;

use common::*;
use fat32::{BlockDevice, CachedDevice, DeviceError, FileSystem, FormatOptions};
use std::cell::Cell;
use std::rc::Rc;

/// A disk that silently stops persisting writes once its budget is used up (the power is gone)
/// and reports an I/O error for them so the operation aborts.
struct CrashDisk {
    disk: RamDisk,
    budget: Rc<Cell<Option<u64>>>,
    writes: Rc<Cell<u64>>,
}

impl BlockDevice for CrashDisk {
    fn sector_count(&self) -> u64 {
        self.disk.sector_count()
    }
    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        self.disk.read_sectors(lba, buffer)
    }
    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        if let Some(left) = self.budget.get() {
            if left == 0 {
                return Err(DeviceError::Io);
            }
            self.budget.set(Some(left - 1));
        }
        self.writes.set(self.writes.get() + 1);
        self.disk.write_sectors(lba, buffer)
    }
    fn flush(&mut self) -> Result<(), DeviceError> {
        self.disk.flush()
    }
}

type Op = fn(&mut FileSystem<CrashDisk>) -> fat32::Result<()>;

/// Problems a crash may legitimately leave behind.
fn acceptable(problem: &str) -> bool {
    // Lost clusters and long-name entries whose short entry never reached the disk take space
    // but reference nothing.
    if problem.contains("lost clusters") || problem.contains("orphaned long-name entries") {
        return true;
    }
    // "chain has N clusters but the size (...) needs M" is fine when N > M (space leak only).
    if let Some(rest) = problem.split("chain has ").nth(1) {
        let have: u64 = rest.split(' ').next().unwrap().parse().unwrap();
        let needs: u64 = problem.rsplit(' ').next().unwrap().parse().unwrap();
        return have > needs;
    }
    false
}

fn base_image() -> RamDisk {
    let mut disk = RamDisk::new(34 * MIB);
    fat32::format(&mut disk, &FormatOptions::default()).unwrap();
    let mut fs = FileSystem::mount(disk).unwrap();
    fs.create_dir_all("/docs/deep/deeper").unwrap();
    fs.create_dir("/other").unwrap();
    for i in 0..40 {
        fs.write_file(&format!("/docs/document number {}.txt", i), &pattern(700 * i, i as u64)).unwrap();
    }
    fs.write_file("/docs/deep/deeper/big.bin", &pattern(40_000, 99)).unwrap();
    fs.write_file("/keep.txt", b"must survive").unwrap();
    fs.write_file("/victim.bin", &pattern(9000, 5)).unwrap();
    fs.unmount().unwrap()
}

fn run_scenario(name: &str, op: Op, touched: &[&str]) {
    let base = base_image();
    let mut reference = FileSystem::mount(base.clone()).unwrap();
    let before = dump_ours(&mut reference);
    drop(reference);

    let mut budget = 0u64;
    loop {
        let limit = Rc::new(Cell::new(None));
        let writes = Rc::new(Cell::new(0));
        let device = CrashDisk { disk: base.clone(), budget: limit.clone(), writes: writes.clone() };
        let mut fs = FileSystem::mount(device).unwrap();
        limit.set(Some(budget));
        let result = op(&mut fs);
        let crashed = result.is_err();
        let image = fs.device().disk.clone();
        drop(fs);

        // "Reboot": mount whatever reached the disk.
        let mut after =
            FileSystem::mount(image).unwrap_or_else(|e| panic!("{}: mount after {} writes: {:?}", name, budget, e));
        let report = after.check().unwrap();
        for problem in &report.problems {
            assert!(
                acceptable(problem),
                "{}: after {} writes: {}\n{:#?}",
                name,
                budget,
                problem,
                report.problems
            );
        }
        let tree = dump_ours(&mut after);
        for (path, contents) in &before {
            if touched.iter().any(|t| path.starts_with(t)) {
                continue;
            }
            assert_eq!(tree.get(path), Some(contents), "{}: after {} writes: {} changed", name, budget, path);
        }
        if !crashed {
            assert!(report.problems.is_empty(), "{}: completed but {:?}", name, report.problems);
            break;
        }
        budget += 1;
        assert!(budget < 500, "{}: never completed", name);
    }
}

#[test]
fn crash_during_create() {
    run_scenario("create", |fs| fs.write_file("/new file with a long name.bin", &pattern(20_000, 1)), &["/new"]);
    run_scenario(
        "create in full dir",
        |fs| fs.write_file("/docs/another long document name.txt", b"x"),
        &["/docs/another"],
    );
}

#[test]
fn crash_during_replace_and_append() {
    run_scenario("replace grow", |fs| fs.write_file("/victim.bin", &pattern(30_000, 2)), &["/victim.bin"]);
    run_scenario("replace shrink", |fs| fs.write_file("/victim.bin", b"small"), &["/victim.bin"]);
    run_scenario("append", |fs| fs.append_file("/victim.bin", &pattern(10_000, 3)), &["/victim.bin"]);
    run_scenario("write_at sparse", |fs| fs.write_at("/victim.bin", 50_000, b"far"), &["/victim.bin"]);
}

#[test]
fn crash_during_truncate_and_remove() {
    run_scenario("truncate shrink", |fs| fs.truncate("/victim.bin", 100), &["/victim.bin"]);
    run_scenario("truncate grow", |fs| fs.truncate("/victim.bin", 30_000), &["/victim.bin"]);
    run_scenario("remove file", |fs| fs.remove_file("/victim.bin"), &["/victim.bin"]);
    run_scenario("remove tree", |fs| fs.remove_dir_all("/docs"), &["/docs"]);
}

#[test]
fn crash_during_directory_operations() {
    run_scenario("mkdir", |fs| fs.create_dir("/brand new directory"), &["/brand"]);
    run_scenario("mkdir -p", |fs| fs.create_dir_all("/a/b/c/d"), &["/a"]);
    run_scenario("copy", |fs| fs.copy_file("/docs/deep/deeper/big.bin", "/other/copy.bin"), &["/other"]);
    run_scenario(
        "rename file",
        |fs| fs.rename("/victim.bin", "/other/moved victim.bin"),
        &["/victim.bin", "/other"],
    );
    run_scenario("rename dir", |fs| fs.rename("/docs/deep", "/other/deep moved"), &["/docs/deep", "/other"]);
}

// ----- the kernel's stack: a write-back cache in front of the disk ----------------------------------

/// Sectors the kernel caches (`CACHE_SECTORS` in kernel/src/fs/mod.rs).
const KERNEL_CACHE_SECTORS: usize = 512;

type CachedOp = fn(&mut FileSystem<CachedDevice<CrashDisk>>) -> fat32::Result<()>;

/// Like `run_scenario`, but on the kernel's stack: the file system sits on a `CachedDevice`
/// (which writes sectors back in its own order between flushes), is mounted and synced first,
/// and the operation is followed by the sync the shell runs after every command. `verify` checks
/// the files the operation touched on every intermediate disk state.
fn run_cached_scenario(
    name: &str,
    op: CachedOp,
    touched: &[&str],
    verify: fn(&mut FileSystem<RamDisk>, &str, u64),
) {
    let base = base_image();
    let mut reference = FileSystem::mount(base.clone()).unwrap();
    let before = dump_ours(&mut reference);
    drop(reference);

    let mut budget = 0u64;
    loop {
        let limit = Rc::new(Cell::new(None));
        let writes = Rc::new(Cell::new(0));
        let device = CrashDisk { disk: base.clone(), budget: limit.clone(), writes: writes.clone() };
        let mut fs = FileSystem::mount(CachedDevice::new(device, KERNEL_CACHE_SECTORS)).unwrap();
        fs.sync().unwrap();
        limit.set(Some(budget));
        let result = op(&mut fs).and_then(|()| fs.sync());
        let crashed = result.is_err();
        let image = fs.device().inner().disk.clone();
        drop(fs);

        let mut after =
            FileSystem::mount(image).unwrap_or_else(|e| panic!("{}: mount after {} writes: {:?}", name, budget, e));
        let report = after.check().unwrap();
        for problem in &report.problems {
            assert!(
                acceptable(problem),
                "{} (cached): after {} writes: {}\n{:#?}",
                name,
                budget,
                problem,
                report.problems
            );
        }
        let tree = dump_ours(&mut after);
        for (path, contents) in &before {
            if touched.iter().any(|t| path.starts_with(t)) {
                continue;
            }
            assert_eq!(tree.get(path), Some(contents), "{} (cached): after {} writes: {} changed", name, budget, path);
        }
        verify(&mut after, name, budget);
        if !crashed {
            assert!(report.problems.is_empty(), "{} (cached): completed but {:?}", name, report.problems);
            assert!(after.was_cleanly_unmounted(), "{} (cached): completed but not clean", name);
            break;
        }
        budget += 1;
        assert!(budget < 5000, "{} (cached): never completed", name);
    }
}

fn no_verify(_: &mut FileSystem<RamDisk>, _: &str, _: u64) {}

/// A file that exists must have its whole contents, never zeros or stale bytes.
fn verify_many(fs: &mut FileSystem<RamDisk>, name: &str, budget: u64) {
    if !fs.exists("/many").unwrap() {
        return;
    }
    for entry in fs.read_dir("/many").unwrap() {
        let index: u64 = entry.name.trim_start_matches('f').trim_end_matches(".txt").parse().unwrap();
        let data = fs.read_file(&format!("/many/{}", entry.name)).unwrap();
        assert_eq!(data, pattern(3000, index), "{}: after {} writes: /many/{} is damaged", name, budget, entry.name);
    }
}

/// A newly created file that exists has its whole contents.
fn verify_new(fs: &mut FileSystem<RamDisk>, name: &str, budget: u64) {
    if let Ok(data) = fs.read_file("/new file with a long name.bin") {
        assert_eq!(data, pattern(20_000, 1), "{}: after {} writes: the new file is damaged", name, budget);
    }
}

#[test]
fn crash_with_write_back_cache() {
    run_cached_scenario("create", |fs| fs.write_file("/new file with a long name.bin", &pattern(20_000, 1)), &["/new"], verify_new);
    // Replacing reuses the file's clusters, so after a crash its contents may be mixed; the
    // structure (entry size against its chain) must still be consistent.
    run_cached_scenario("replace shrink", |fs| fs.write_file("/victim.bin", b"small"), &["/victim.bin"], no_verify);
    run_cached_scenario("truncate shrink", |fs| fs.truncate("/victim.bin", 100), &["/victim.bin"], no_verify);
    run_cached_scenario("remove file", |fs| fs.remove_file("/victim.bin"), &["/victim.bin"], no_verify);
    run_cached_scenario("remove tree", |fs| fs.remove_dir_all("/docs"), &["/docs"], no_verify);
    run_cached_scenario("mkdir -p", |fs| fs.create_dir_all("/a/b/c/d"), &["/a"], no_verify);
    run_cached_scenario("copy", |fs| fs.copy_file("/docs/deep/deeper/big.bin", "/other/copy.bin"), &["/other"], no_verify);
    run_cached_scenario(
        "rename dir",
        |fs| fs.rename("/docs/deep", "/other/deep moved"),
        &["/docs/deep", "/other"],
        no_verify,
    );
}

#[test]
fn crash_while_creating_many_files_with_write_back_cache() {
    run_cached_scenario(
        "many files",
        |fs| {
            fs.create_dir("/many")?;
            for i in 0..40u64 {
                fs.write_file(&format!("/many/f{}.txt", i), &pattern(3000, i))?;
            }
            Ok(())
        },
        &["/many"],
        verify_many,
    );
}
