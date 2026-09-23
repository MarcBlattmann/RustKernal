//! Byte-granular I/O on the device and along cluster chains.
//!
//! Aligned whole sectors are transferred directly into/out of the caller's buffer; only the
//! partial sectors at the edges of a range go through a bounce buffer (read-modify-write for
//! writes). Consecutive clusters of a chain are merged into a single device request.

use crate::{BlockDevice, Error, FileSystem, Result, SECTOR_SIZE};
use alloc::vec;

const SECTOR: u64 = SECTOR_SIZE as u64;
/// Largest zero-fill request.
const ZERO_CHUNK: usize = 64 * 1024;

impl<D: BlockDevice> FileSystem<D> {
    /// Reads `buffer.len()` bytes starting at byte `position` of the device.
    pub(crate) fn read_bytes(&mut self, position: u64, buffer: &mut [u8]) -> Result<()> {
        let mut position = position;
        let mut done = 0usize;
        while done < buffer.len() {
            let offset = (position % SECTOR) as usize;
            let remaining = buffer.len() - done;
            let lba = position / SECTOR;
            if offset == 0 && remaining >= SECTOR_SIZE {
                let whole = remaining - remaining % SECTOR_SIZE;
                self.device.read_sectors(lba, &mut buffer[done..done + whole])?;
                done += whole;
                position += whole as u64;
            } else {
                let mut sector = [0u8; SECTOR_SIZE];
                self.device.read_sectors(lba, &mut sector)?;
                let take = (SECTOR_SIZE - offset).min(remaining);
                buffer[done..done + take].copy_from_slice(&sector[offset..offset + take]);
                done += take;
                position += take as u64;
            }
        }
        Ok(())
    }

    /// Writes `data` at byte `position` of the device (read-modify-write for partial sectors).
    pub(crate) fn write_bytes(&mut self, position: u64, data: &[u8]) -> Result<()> {
        let mut position = position;
        let mut done = 0usize;
        while done < data.len() {
            let offset = (position % SECTOR) as usize;
            let remaining = data.len() - done;
            let lba = position / SECTOR;
            if offset == 0 && remaining >= SECTOR_SIZE {
                let whole = remaining - remaining % SECTOR_SIZE;
                self.device.write_sectors(lba, &data[done..done + whole])?;
                done += whole;
                position += whole as u64;
            } else {
                let mut sector = [0u8; SECTOR_SIZE];
                self.device.read_sectors(lba, &mut sector)?;
                let take = (SECTOR_SIZE - offset).min(remaining);
                sector[offset..offset + take].copy_from_slice(&data[done..done + take]);
                self.device.write_sectors(lba, &sector)?;
                done += take;
                position += take as u64;
            }
        }
        Ok(())
    }

    /// Writes `length` zero bytes at byte `position` of the device.
    pub(crate) fn zero_bytes(&mut self, position: u64, length: u64) -> Result<()> {
        if length == 0 {
            return Ok(());
        }
        let zeros = vec![0u8; (length.min(ZERO_CHUNK as u64)) as usize];
        let mut done = 0u64;
        while done < length {
            let take = (length - done).min(zeros.len() as u64) as usize;
            self.write_bytes(position + done, &zeros[..take])?;
            done += take as u64;
        }
        Ok(())
    }

    pub(crate) fn read_cluster(&mut self, cluster: u32, buffer: &mut [u8]) -> Result<()> {
        debug_assert_eq!(buffer.len(), self.geo.cluster_size as usize);
        let lba = self.geo.cluster_lba(cluster);
        self.device.read_sectors(lba, buffer)?;
        Ok(())
    }

    pub(crate) fn write_cluster(&mut self, cluster: u32, buffer: &[u8]) -> Result<()> {
        debug_assert_eq!(buffer.len(), self.geo.cluster_size as usize);
        let lba = self.geo.cluster_lba(cluster);
        self.device.write_sectors(lba, buffer)?;
        Ok(())
    }

    /// Calls `f(fs, device_byte_position, length, offset_into_range)` for each physically
    /// contiguous piece of the byte range `offset..offset + length` of the chain starting at
    /// `first`. The chain must be long enough to hold the range.
    pub(crate) fn for_each_extent<F>(&mut self, first: u32, offset: u64, length: u64, mut f: F) -> Result<()>
    where
        F: FnMut(&mut Self, u64, usize, u64) -> Result<()>,
    {
        if length == 0 {
            return Ok(());
        }
        let cluster_size = self.geo.cluster_size as u64;
        let mut index = (offset / cluster_size) as u32;
        let mut in_cluster = offset % cluster_size;
        let mut cluster = self.cluster_at(first, index)?;
        let mut done = 0u64;
        loop {
            let start = cluster;
            let remaining = length - done;
            let mut available = cluster_size - in_cluster;
            let mut last = cluster;
            let mut following = None;
            while available < remaining {
                match self.next_cluster(last)? {
                    Some(next) if next == last + 1 => {
                        last = next;
                        index += 1;
                        available += cluster_size;
                    }
                    Some(next) => {
                        following = Some(next);
                        break;
                    }
                    None => return Err(Error::Corrupt("cluster chain is shorter than the file")),
                }
            }
            let take = available.min(remaining);
            let position = self.geo.cluster_pos(start) + in_cluster;
            f(self, position, take as usize, done)?;
            done += take;
            self.set_chain_hint(first, index, last);
            if done >= length {
                return Ok(());
            }
            cluster = match following {
                Some(next) => next,
                None => self.next_cluster(last)?.ok_or(Error::Corrupt("cluster chain is shorter than the file"))?,
            };
            index += 1;
            in_cluster = 0;
        }
    }

    /// Reads `buffer.len()` bytes at `offset` of the chain starting at `first`.
    pub(crate) fn read_chain(&mut self, first: u32, offset: u64, buffer: &mut [u8]) -> Result<()> {
        let length = buffer.len() as u64;
        self.for_each_extent(first, offset, length, |fs, position, len, at| {
            let at = at as usize;
            fs.read_bytes(position, &mut buffer[at..at + len])
        })
    }

    /// Writes `data` at `offset` of the chain starting at `first`.
    pub(crate) fn write_chain(&mut self, first: u32, offset: u64, data: &[u8]) -> Result<()> {
        self.for_each_extent(first, offset, data.len() as u64, |fs, position, len, at| {
            let at = at as usize;
            fs.write_bytes(position, &data[at..at + len])
        })
    }

    /// Writes `length` zero bytes at `offset` of the chain starting at `first`.
    pub(crate) fn zero_chain(&mut self, first: u32, offset: u64, length: u64) -> Result<()> {
        self.for_each_extent(first, offset, length, |fs, position, len, _| fs.zero_bytes(position, len as u64))
    }
}
