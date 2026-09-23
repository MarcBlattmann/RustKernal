//! Power-loss simulation: every mutating operation is cut off after each possible number of
//! device writes, and the disk as it was at that moment must mount, pass `check()` apart from
//! harmless leftovers (lost clusters, over-long chains, a briefly duplicated entry during a
//! rename) and keep every file the operation did not touch intact.

mod common;

use common::*;
use fat32::{BlockDevice, DeviceError, FileSystem, FormatOptions};
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
fn acceptable(problem: &str, allow_duplicates: bool) -> bool {
    if problem.contains("lost clusters") {
        return true;
    }
    // "chain has N clusters but the size (...) needs M" is fine when N > M (space leak only).
    if let Some(rest) = problem.split("chain has ").nth(1) {
        let have: u64 = rest.split(' ').next().unwrap().parse().unwrap();
        let needs: u64 = problem.rsplit(' ').next().unwrap().parse().unwrap();
        return have > needs;
    }
    // A rename that was cut short can leave both names (and a '..' still pointing at the old
    // parent) behind; nothing is lost.
    allow_duplicates
        && (problem.contains("cross-linked") || problem.contains("duplicate name") || problem.contains("'..'"))
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

fn run_scenario(name: &str, op: Op, touched: &[&str], allow_duplicates: bool) {
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
                acceptable(problem, allow_duplicates),
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
    run_scenario("create", |fs| fs.write_file("/new file with a long name.bin", &pattern(20_000, 1)), &["/new"], false);
    run_scenario(
        "create in full dir",
        |fs| fs.write_file("/docs/another long document name.txt", b"x"),
        &["/docs/another"],
        false,
    );
}

#[test]
fn crash_during_replace_and_append() {
    run_scenario("replace grow", |fs| fs.write_file("/victim.bin", &pattern(30_000, 2)), &["/victim.bin"], false);
    run_scenario("replace shrink", |fs| fs.write_file("/victim.bin", b"small"), &["/victim.bin"], false);
    run_scenario("append", |fs| fs.append_file("/victim.bin", &pattern(10_000, 3)), &["/victim.bin"], false);
    run_scenario("write_at sparse", |fs| fs.write_at("/victim.bin", 50_000, b"far"), &["/victim.bin"], false);
}

#[test]
fn crash_during_truncate_and_remove() {
    run_scenario("truncate shrink", |fs| fs.truncate("/victim.bin", 100), &["/victim.bin"], false);
    run_scenario("truncate grow", |fs| fs.truncate("/victim.bin", 30_000), &["/victim.bin"], false);
    run_scenario("remove file", |fs| fs.remove_file("/victim.bin"), &["/victim.bin"], false);
    run_scenario("remove tree", |fs| fs.remove_dir_all("/docs"), &["/docs"], false);
}

#[test]
fn crash_during_directory_operations() {
    run_scenario("mkdir", |fs| fs.create_dir("/brand new directory"), &["/brand"], false);
    run_scenario("mkdir -p", |fs| fs.create_dir_all("/a/b/c/d"), &["/a"], false);
    run_scenario("copy", |fs| fs.copy_file("/docs/deep/deeper/big.bin", "/other/copy.bin"), &["/other"], false);
    run_scenario(
        "rename file",
        |fs| fs.rename("/victim.bin", "/other/moved victim.bin"),
        &["/victim.bin", "/other"],
        true,
    );
    run_scenario("rename dir", |fs| fs.rename("/docs/deep", "/other/deep moved"), &["/docs/deep", "/other"], true);
}
