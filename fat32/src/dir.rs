//! Directory entries: 32-byte short (8.3) entries, VFAT long-name entries, directory iteration,
//! entry creation (with directory growth) and deletion.

use crate::boot_sector::{le16, le32, put16, put32};
use crate::name::{self, eq_ignore_case, lfn_checksum, short_name_to_string};
use crate::time::{self, FatTime};
use crate::{BlockDevice, Error, FileSystem, Metadata, Result, SECTOR_SIZE};
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

pub(crate) const ENTRY_SIZE: usize = 32;

pub(crate) const ATTR_READ_ONLY: u8 = 0x01;
pub(crate) const ATTR_HIDDEN: u8 = 0x02;
pub(crate) const ATTR_SYSTEM: u8 = 0x04;
pub(crate) const ATTR_VOLUME_ID: u8 = 0x08;
pub(crate) const ATTR_DIRECTORY: u8 = 0x10;
pub(crate) const ATTR_ARCHIVE: u8 = 0x20;
pub(crate) const ATTR_LONG_NAME: u8 = 0x0F;
const ATTR_LONG_NAME_MASK: u8 = 0x3F;

/// First name byte of a deleted entry.
pub(crate) const DELETED: u8 = 0xE5;
/// Ordinal flag of the last (first stored) long-name entry.
pub(crate) const LAST_LONG_ENTRY: u8 = 0x40;
/// A directory may not hold more than 65536 32-byte entries.
pub(crate) const MAX_DIR_ENTRIES: u32 = 65536;
/// Long names use at most 20 entries (255 UTF-16 units, 13 per entry).
const MAX_LONG_ENTRIES: u8 = 20;
const UNITS_PER_ENTRY: usize = 13;
/// Byte offsets of the 13 UTF-16 units in a long-name entry.
const UNIT_OFFSETS: [usize; UNITS_PER_ENTRY] = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];

pub(crate) type RawEntry = [u8; ENTRY_SIZE];

pub(crate) const DOT_NAME: [u8; 11] = *b".          ";
pub(crate) const DOTDOT_NAME: [u8; 11] = *b"..         ";

/// Position of a 32-byte slot: a cluster of the directory and the byte offset inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotLoc {
    pub cluster: u32,
    pub offset: u32,
}

pub(crate) fn is_long_entry(raw: &RawEntry) -> bool {
    raw[11] & ATTR_LONG_NAME_MASK == ATTR_LONG_NAME
}

/// A short entry together with its long name and where it is stored.
#[derive(Debug, Clone)]
pub(crate) struct DirEntry {
    /// The short (8.3) entry.
    pub raw: RawEntry,
    pub long_name: Option<String>,
    /// First cluster of the directory containing this entry.
    pub dir_cluster: u32,
    /// Slots of the long-name entries (in on-disk order) followed by the short entry's slot.
    pub slots: Vec<SlotLoc>,
}

impl DirEntry {
    pub fn short_name(&self) -> [u8; 11] {
        let mut short = [0u8; 11];
        short.copy_from_slice(&self.raw[..11]);
        short
    }

    pub fn attr(&self) -> u8 {
        self.raw[11]
    }

    pub fn is_dir(&self) -> bool {
        self.attr() & ATTR_DIRECTORY != 0
    }

    pub fn is_volume_label(&self) -> bool {
        self.attr() & ATTR_VOLUME_ID != 0 && !self.is_dir()
    }

    pub fn is_dot(&self) -> bool {
        self.raw[..11] == DOT_NAME || self.raw[..11] == DOTDOT_NAME
    }

    /// Visible in listings: not `.`/`..` and not a volume label.
    pub fn is_visible(&self) -> bool {
        !self.is_dot() && !self.is_volume_label()
    }

    pub fn first_cluster(&self) -> u32 {
        ((le16(&self.raw, 20) as u32) << 16) | le16(&self.raw, 26) as u32
    }

    pub fn set_first_cluster(&mut self, cluster: u32) {
        put16(&mut self.raw, 20, (cluster >> 16) as u16);
        put16(&mut self.raw, 26, cluster as u16);
    }

    pub fn size(&self) -> u32 {
        le32(&self.raw, 28)
    }

    pub fn set_size(&mut self, size: u32) {
        put32(&mut self.raw, 28, size);
    }

    pub fn set_modified(&mut self, now: FatTime) {
        put16(&mut self.raw, 22, now.time);
        put16(&mut self.raw, 24, now.date);
        put16(&mut self.raw, 18, now.date);
    }

    pub fn sfn_slot(&self) -> SlotLoc {
        *self.slots.last().expect("entry without slots")
    }

    /// Long name if present, otherwise the 8.3 name.
    pub fn name(&self) -> String {
        match &self.long_name {
            Some(long) => long.clone(),
            None => short_name_to_string(&self.short_name(), self.raw[12]),
        }
    }

    /// Case-insensitive match against the long name and the short name.
    pub fn matches(&self, name: &str) -> bool {
        if let Some(long) = &self.long_name
            && eq_ignore_case(long, name)
        {
            return true;
        }
        eq_ignore_case(&short_name_to_string(&self.short_name(), 0), name)
    }

    pub fn metadata(&self) -> Metadata {
        let attr = self.attr();
        Metadata {
            name: self.name(),
            is_dir: self.is_dir(),
            size: if self.is_dir() { 0 } else { self.size() as u64 },
            created: time::decode(le16(&self.raw, 16), le16(&self.raw, 14), self.raw[13]),
            modified: time::decode(le16(&self.raw, 24), le16(&self.raw, 22), 0),
            read_only: attr & ATTR_READ_ONLY != 0,
            hidden: attr & ATTR_HIDDEN != 0,
            system: attr & ATTR_SYSTEM != 0,
        }
    }
}

/// Builds a short entry.
pub(crate) fn short_entry(
    short: [u8; 11],
    attr: u8,
    first_cluster: u32,
    size: u32,
    created: FatTime,
    modified: FatTime,
) -> RawEntry {
    let mut raw = [0u8; ENTRY_SIZE];
    raw[..11].copy_from_slice(&short);
    raw[11] = attr;
    raw[13] = created.tenths;
    put16(&mut raw, 14, created.time);
    put16(&mut raw, 16, created.date);
    put16(&mut raw, 18, modified.date);
    put16(&mut raw, 20, (first_cluster >> 16) as u16);
    put16(&mut raw, 22, modified.time);
    put16(&mut raw, 24, modified.date);
    put16(&mut raw, 26, first_cluster as u16);
    put32(&mut raw, 28, size);
    raw
}

/// Long-name entries for `name` in on-disk order (highest ordinal first).
pub(crate) fn long_entries(name: &str, checksum: u8) -> Vec<RawEntry> {
    let mut units: Vec<u16> = name.encode_utf16().collect();
    let count = units.len().div_ceil(UNITS_PER_ENTRY);
    if !units.len().is_multiple_of(UNITS_PER_ENTRY) {
        units.push(0x0000);
    }
    units.resize(count * UNITS_PER_ENTRY, 0xFFFF);
    let mut entries = Vec::with_capacity(count);
    for ordinal in (1..=count).rev() {
        let mut raw = [0u8; ENTRY_SIZE];
        raw[0] = ordinal as u8 | if ordinal == count { LAST_LONG_ENTRY } else { 0 };
        raw[11] = ATTR_LONG_NAME;
        raw[12] = 0;
        raw[13] = checksum;
        let chunk = &units[(ordinal - 1) * UNITS_PER_ENTRY..ordinal * UNITS_PER_ENTRY];
        for (unit, &offset) in chunk.iter().zip(UNIT_OFFSETS.iter()) {
            put16(&mut raw, offset, *unit);
        }
        entries.push(raw);
    }
    entries
}

/// Assembles a long name from its entries while scanning a directory.
#[derive(Default)]
pub(crate) struct LongNameState {
    units: Vec<u16>,
    /// Ordinal expected next (counts down to 0 when complete).
    next: u8,
    checksum: u8,
    slots: Vec<SlotLoc>,
    active: bool,
}

impl LongNameState {
    /// Abandons the current sequence; returns whether one was in progress (an orphan).
    pub fn abandon(&mut self) -> bool {
        let was_active = self.active;
        self.active = false;
        self.slots.clear();
        was_active
    }

    /// Feeds one long-name entry. Returns the number of orphaned sequences detected.
    pub fn push(&mut self, loc: SlotLoc, raw: &RawEntry) -> u32 {
        let ordinal = raw[0] & 0x1F;
        let mut orphans = 0;
        if raw[0] & LAST_LONG_ENTRY != 0 {
            if self.abandon() {
                orphans += 1;
            }
            if ordinal == 0 || ordinal > MAX_LONG_ENTRIES {
                return orphans + 1;
            }
            self.units.clear();
            self.units.resize(ordinal as usize * UNITS_PER_ENTRY, 0xFFFF);
            self.checksum = raw[13];
            self.next = ordinal;
            self.active = true;
        } else if !self.active || ordinal != self.next || ordinal == 0 || raw[13] != self.checksum {
            let abandoned = self.abandon();
            return orphans + if abandoned { 1 } else { 0 } + 1;
        }
        let base = (ordinal as usize - 1) * UNITS_PER_ENTRY;
        for (i, &offset) in UNIT_OFFSETS.iter().enumerate() {
            self.units[base + i] = le16(raw, offset);
        }
        self.slots.push(loc);
        self.next -= 1;
        orphans
    }

    /// Completes the sequence with its short entry. Returns the long name (if the sequence is
    /// complete and its checksum matches), its slots, and whether an orphan was discarded.
    pub fn finish(&mut self, short: &[u8; 11]) -> (Option<String>, Vec<SlotLoc>, bool) {
        if !self.active {
            return (None, Vec::new(), false);
        }
        self.active = false;
        let slots = core::mem::take(&mut self.slots);
        if self.next != 0 || lfn_checksum(short) != self.checksum {
            return (None, Vec::new(), true);
        }
        let end = self.units.iter().position(|&u| u == 0).unwrap_or(self.units.len());
        let units = &self.units[..end];
        if units.is_empty() {
            return (None, Vec::new(), true);
        }
        (Some(String::from_utf16_lossy(units)), slots, false)
    }
}

/// Reads the raw 32-byte slots of a directory, a cluster at a time.
pub(crate) struct DirReader {
    cluster: u32,
    buffer: Vec<u8>,
    position: usize,
    slots_read: u32,
    started: bool,
    finished: bool,
}

impl DirReader {
    pub fn new(first_cluster: u32) -> Self {
        Self { cluster: first_cluster, buffer: Vec::new(), position: 0, slots_read: 0, started: false, finished: false }
    }

    /// The cluster the last returned slot came from.
    pub fn cluster(&self) -> u32 {
        self.cluster
    }

    pub fn slots_read(&self) -> u32 {
        self.slots_read
    }

    pub fn next_slot<D: BlockDevice>(&mut self, fs: &mut FileSystem<D>) -> Result<Option<(SlotLoc, RawEntry)>> {
        if self.finished {
            return Ok(None);
        }
        if self.slots_read >= MAX_DIR_ENTRIES {
            self.finished = true;
            return Ok(None);
        }
        if !self.started {
            if !fs.geo.is_valid_cluster(self.cluster) {
                return Err(Error::Corrupt("directory starts at an invalid cluster"));
            }
            self.buffer = vec![0u8; fs.geo.cluster_size as usize];
            fs.read_cluster(self.cluster, &mut self.buffer)?;
            self.started = true;
        } else if self.position >= self.buffer.len() {
            match fs.next_cluster(self.cluster)? {
                Some(next) => {
                    self.cluster = next;
                    fs.read_cluster(next, &mut self.buffer)?;
                    self.position = 0;
                }
                None => {
                    self.finished = true;
                    return Ok(None);
                }
            }
        }
        let mut raw = [0u8; ENTRY_SIZE];
        raw.copy_from_slice(&self.buffer[self.position..self.position + ENTRY_SIZE]);
        let loc = SlotLoc { cluster: self.cluster, offset: self.position as u32 };
        self.position += ENTRY_SIZE;
        self.slots_read += 1;
        Ok(Some((loc, raw)))
    }
}

/// Iterates the short entries of a directory (including `.`/`..` and volume labels) with their
/// long names, stopping at the end-of-directory marker.
pub(crate) struct EntryIter {
    reader: DirReader,
    long: LongNameState,
    dir: u32,
    /// Orphaned long-name sequences seen so far.
    pub orphans: u32,
}

impl EntryIter {
    pub fn new(dir_cluster: u32) -> Self {
        Self { reader: DirReader::new(dir_cluster), long: LongNameState::default(), dir: dir_cluster, orphans: 0 }
    }

    pub fn next<D: BlockDevice>(&mut self, fs: &mut FileSystem<D>) -> Result<Option<DirEntry>> {
        loop {
            let Some((loc, raw)) = self.reader.next_slot(fs)? else {
                if self.long.abandon() {
                    self.orphans += 1;
                }
                return Ok(None);
            };
            match raw[0] {
                0x00 => {
                    if self.long.abandon() {
                        self.orphans += 1;
                    }
                    self.reader.finished = true;
                    return Ok(None);
                }
                DELETED => {
                    if self.long.abandon() {
                        self.orphans += 1;
                    }
                    continue;
                }
                _ => {}
            }
            if is_long_entry(&raw) {
                self.orphans += self.long.push(loc, &raw);
                continue;
            }
            let mut short = [0u8; 11];
            short.copy_from_slice(&raw[..11]);
            let (long_name, mut slots, orphan) = self.long.finish(&short);
            if orphan {
                self.orphans += 1;
            }
            slots.push(loc);
            return Ok(Some(DirEntry { raw, long_name, dir_cluster: self.dir, slots }));
        }
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Finds a visible entry by name (case-insensitive, long or short name).
    pub(crate) fn lookup(&mut self, dir: u32, name: &str) -> Result<Option<DirEntry>> {
        let mut iter = EntryIter::new(dir);
        while let Some(entry) = iter.next(self)? {
            if entry.is_visible() && entry.matches(name) {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    /// All visible entries of a directory in on-disk order.
    pub(crate) fn list_dir(&mut self, dir: u32) -> Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        let mut iter = EntryIter::new(dir);
        while let Some(entry) = iter.next(self)? {
            if entry.is_visible() {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    pub(crate) fn dir_is_empty(&mut self, dir: u32) -> Result<bool> {
        let mut iter = EntryIter::new(dir);
        while let Some(entry) = iter.next(self)? {
            if entry.is_visible() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The volume label entry of the root directory, if any.
    pub(crate) fn root_label(&mut self) -> Result<Option<String>> {
        let mut iter = EntryIter::new(self.geo.root_cluster);
        while let Some(entry) = iter.next(self)? {
            if entry.is_volume_label() {
                return Ok(Some(crate::boot_sector::label_to_string(&entry.raw[..11])));
            }
        }
        Ok(None)
    }

    /// Every short name in use in a directory (except `exclude`).
    fn short_names_in(&mut self, dir: u32, exclude: Option<[u8; 11]>) -> Result<BTreeSet<[u8; 11]>> {
        let mut names = BTreeSet::new();
        let mut iter = EntryIter::new(dir);
        while let Some(entry) = iter.next(self)? {
            if entry.is_volume_label() {
                continue;
            }
            let short = entry.short_name();
            if Some(short) == exclude {
                continue;
            }
            names.insert(short);
            // A long name that is itself a valid short name must not be handed out as another
            // entry's short name either, or lookups by that name would be ambiguous.
            if let Some(long) = &entry.long_name {
                let basis = name::Basis::new(long);
                if basis.fits() {
                    names.insert(basis.short_name(None));
                }
            }
        }
        Ok(names)
    }

    /// Finds `count` consecutive free slots in a directory, growing it by zeroed clusters if
    /// needed. Also returns a slot after the run that must be cleared to keep the
    /// end-of-directory marker in place (only when the run ends in never-used space that
    /// contains garbage).
    fn find_free_slots(&mut self, dir: u32, count: usize) -> Result<(Vec<SlotLoc>, Option<SlotLoc>)> {
        let mut reader = DirReader::new(dir);
        let mut run: Vec<SlotLoc> = Vec::with_capacity(count);
        let mut at_end = false;
        while let Some((loc, raw)) = reader.next_slot(self)? {
            if raw[0] == 0x00 {
                at_end = true;
            }
            if at_end || raw[0] == DELETED {
                run.push(loc);
                if run.len() == count {
                    let mut clear = None;
                    if at_end
                        && let Some((next, raw)) = reader.next_slot(self)?
                        && raw[0] != 0x00
                    {
                        clear = Some(next);
                    }
                    return Ok((run, clear));
                }
            } else {
                run.clear();
            }
        }
        // Not enough room: append clusters.
        let per_cluster = self.geo.cluster_size as usize / ENTRY_SIZE;
        let missing = count - run.len();
        let new_clusters = missing.div_ceil(per_cluster) as u32;
        let total_slots = reader.slots_read() as u64 + new_clusters as u64 * per_cluster as u64;
        if reader.slots_read() >= MAX_DIR_ENTRIES || total_slots > MAX_DIR_ENTRIES as u64 {
            return Err(Error::NoSpace);
        }
        let last = reader.cluster();
        let first_new = self.allocate(Some(last), new_clusters)?;
        let zeros = vec![0u8; self.geo.cluster_size as usize];
        let mut cluster = first_new;
        for i in 0..new_clusters {
            if let Err(error) = self.write_cluster(cluster, &zeros) {
                let _ = self.free_chain(first_new);
                let _ = self.fat_set(last, crate::fat::EOC);
                return Err(error);
            }
            for slot in 0..per_cluster {
                if run.len() < count {
                    run.push(SlotLoc { cluster, offset: (slot * ENTRY_SIZE) as u32 });
                }
            }
            if i + 1 < new_clusters {
                cluster = self.next_cluster(cluster)?.ok_or(Error::Corrupt("directory growth failed"))?;
            }
        }
        Ok((run, None))
    }

    /// Read-modify-writes 32-byte slots: `edit(i, slot_bytes)` is called for `slots[i]`. One
    /// sector write per distinct sector, sectors in order of first appearance in `slots`.
    pub(crate) fn edit_slots<F: FnMut(usize, &mut [u8])>(&mut self, slots: &[SlotLoc], mut edit: F) -> Result<()> {
        let mut groups: Vec<(u64, Vec<usize>)> = Vec::new();
        for (i, loc) in slots.iter().enumerate() {
            let lba = self.geo.cluster_lba(loc.cluster) + (loc.offset as usize / SECTOR_SIZE) as u64;
            match groups.iter_mut().find(|group| group.0 == lba) {
                Some(group) => group.1.push(i),
                None => groups.push((lba, vec![i])),
            }
        }
        for (lba, members) in groups {
            let mut sector = [0u8; SECTOR_SIZE];
            self.device.read_sectors(lba, &mut sector)?;
            for i in members {
                let offset = slots[i].offset as usize % SECTOR_SIZE;
                edit(i, &mut sector[offset..offset + ENTRY_SIZE]);
            }
            self.device.write_sectors(lba, &sector)?;
        }
        Ok(())
    }

    /// Writes whole 32-byte slots.
    pub(crate) fn write_slots(&mut self, writes: &[(SlotLoc, RawEntry)]) -> Result<()> {
        let slots: Vec<SlotLoc> = writes.iter().map(|w| w.0).collect();
        self.edit_slots(&slots, |i, raw| raw.copy_from_slice(&writes[i].1))
    }

    /// Creates the entries (long name + short entry) for `name` in directory `dir`. `template`
    /// supplies attributes, timestamps, first cluster and size; its name bytes are replaced.
    /// Flushes the FAT before writing, so every cluster the entry references (and any new
    /// directory cluster) is linked on disk before it becomes reachable.
    pub(crate) fn create_entry(
        &mut self,
        dir: u32,
        name: &str,
        template: RawEntry,
        exclude: Option<[u8; 11]>,
    ) -> Result<DirEntry> {
        name::validate_name(name)?;
        let existing = self.short_names_in(dir, exclude)?;
        let (short, needs_long) = name::generate_short_name(name, &existing)?;
        let mut raw = template;
        raw[..11].copy_from_slice(&short);
        raw[12] = 0;
        let mut entries = if needs_long { long_entries(name, lfn_checksum(&short)) } else { Vec::new() };
        entries.push(raw);
        let (slots, clear) = self.find_free_slots(dir, entries.len())?;
        self.flush_fat()?;
        let mut writes: Vec<(SlotLoc, RawEntry)> = slots.iter().copied().zip(entries.iter().copied()).collect();
        if let Some(clear) = clear {
            writes.push((clear, [0u8; ENTRY_SIZE]));
        }
        self.write_slots(&writes)?;
        Ok(DirEntry {
            raw,
            long_name: if needs_long { Some(String::from(name)) } else { None },
            dir_cluster: dir,
            slots,
        })
    }

    /// Marks an entry and its long-name entries deleted (short entry first).
    pub(crate) fn delete_entry(&mut self, entry: &DirEntry) -> Result<()> {
        let mut order = Vec::with_capacity(entry.slots.len());
        order.push(entry.sfn_slot());
        order.extend_from_slice(&entry.slots[..entry.slots.len() - 1]);
        self.edit_slots(&order, |_, raw| raw[0] = DELETED)
    }

    /// Rewrites an entry's short entry (size, first cluster, timestamps, attributes).
    pub(crate) fn update_entry(&mut self, entry: &DirEntry) -> Result<()> {
        self.write_slots(&[(entry.sfn_slot(), entry.raw)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(entries: &[RawEntry], short: &[u8; 11]) -> Option<String> {
        let mut state = LongNameState::default();
        for (i, raw) in entries.iter().enumerate() {
            assert_eq!(state.push(SlotLoc { cluster: 2, offset: i as u32 * 32 }, raw), 0);
        }
        let (name, slots, orphan) = state.finish(short);
        if name.is_some() {
            assert_eq!(slots.len(), entries.len());
            assert!(!orphan);
        }
        name
    }

    #[test]
    fn long_name_round_trip() {
        let short = *b"LONGFI~1TXT";
        for name in [
            "a",
            "exactly13char",
            "exactly13chars",
            "long file name.txt",
            "ünïcödé 😀 name with astral chars.data",
            &"x".repeat(255),
        ] {
            let entries = long_entries(name, lfn_checksum(&short));
            assert_eq!(entries.len(), name.encode_utf16().count().div_ceil(13));
            assert_eq!(entries[0][0] & LAST_LONG_ENTRY, LAST_LONG_ENTRY);
            assert_eq!(entries[0][0] & 0x1F, entries.len() as u8);
            assert_eq!(entries.last().unwrap()[0] & 0x1F, 1);
            for e in &entries {
                assert_eq!(e[11], ATTR_LONG_NAME);
                assert_eq!(e[13], lfn_checksum(&short));
                assert_eq!(le16(e, 26), 0);
            }
            assert_eq!(decode(&entries, &short).as_deref(), Some(name));
        }
    }

    #[test]
    fn long_name_padding() {
        let entries = long_entries("abc", 0);
        assert_eq!(le16(&entries[0], 1), b'a' as u16);
        assert_eq!(le16(&entries[0], 7), 0x0000);
        assert_eq!(le16(&entries[0], 9), 0xFFFF);
        assert_eq!(le16(&entries[0], 30), 0xFFFF);
        // Exactly 13 units: no terminator, no padding.
        let entries = long_entries("abcdefghijklm", 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(le16(&entries[0], 30), b'm' as u16);
    }

    #[test]
    fn checksum_mismatch_is_orphan() {
        let entries = long_entries("some long name", lfn_checksum(b"SOMELO~1   "));
        let mut state = LongNameState::default();
        for (i, raw) in entries.iter().enumerate() {
            state.push(SlotLoc { cluster: 2, offset: i as u32 * 32 }, raw);
        }
        let (name, _, orphan) = state.finish(b"OTHER   TXT");
        assert_eq!(name, None);
        assert!(orphan);
    }

    #[test]
    fn broken_sequence_is_orphan() {
        let entries = long_entries("a name that needs three entries!!!!", 7);
        assert_eq!(entries.len(), 3);
        let mut state = LongNameState::default();
        assert_eq!(state.push(SlotLoc { cluster: 2, offset: 0 }, &entries[0]), 0);
        // Skip the middle entry.
        assert_eq!(state.push(SlotLoc { cluster: 2, offset: 32 }, &entries[2]), 2);
        let (name, _, orphan) = state.finish(b"A       TXT");
        assert_eq!(name, None);
        assert!(!orphan);
    }

    #[test]
    fn short_entry_fields() {
        let t = time::encode(crate::Timestamp { year: 2030, month: 5, day: 6, hour: 7, minute: 8, second: 9 });
        let raw = short_entry(*b"FILE    TXT", ATTR_ARCHIVE, 0x0012_3456, 777, t, t);
        let entry = DirEntry { raw, long_name: None, dir_cluster: 2, slots: vec![SlotLoc { cluster: 2, offset: 0 }] };
        assert_eq!(entry.first_cluster(), 0x0012_3456);
        assert_eq!(entry.size(), 777);
        assert_eq!(entry.name(), "FILE.TXT");
        assert!(entry.matches("file.txt"));
        let meta = entry.metadata();
        assert_eq!(meta.created, crate::Timestamp { year: 2030, month: 5, day: 6, hour: 7, minute: 8, second: 9 });
        assert_eq!(meta.modified, crate::Timestamp { year: 2030, month: 5, day: 6, hour: 7, minute: 8, second: 8 });
        assert!(!meta.is_dir);
    }
}
