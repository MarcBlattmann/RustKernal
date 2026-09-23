use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use spin::Mutex;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

const PAGE_SIZE: u64 = 4096;
const LEVEL_4_ENTRY_SPAN: u64 = 1 << 39;

struct Paging {
    mapper: OffsetPageTable<'static>,
    frame_allocator: BootInfoFrameAllocator,
    /// Start of an otherwise unused 512 GiB region reserved for device memory mappings.
    mmio_region_start: u64,
    mmio_region_used: u64,
}

static PAGING: Mutex<Option<Paging>> = Mutex::new(None);

/// Hands out the usable frames listed in the bootloader memory map, one after another.
struct BootInfoFrameAllocator {
    memory_regions: &'static MemoryRegions,
    next_frame: usize,
}

impl BootInfoFrameAllocator {
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.memory_regions
            .iter()
            .filter(|region| region.kind == MemoryRegionKind::Usable)
            .flat_map(|region| {
                let first = (region.start + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                (first..region.end.saturating_sub(PAGE_SIZE - 1)).step_by(PAGE_SIZE as usize)
            })
            .map(|address| PhysFrame::containing_address(PhysAddr::new(address)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next_frame);
        self.next_frame += 1;
        frame
    }
}

/// Sets up access to the active page table. Requires the bootloader to map all physical memory
/// at `physical_memory_offset`.
pub unsafe fn init(physical_memory_offset: u64, memory_regions: &'static MemoryRegions) {
    let (level_4_frame, _) = Cr3::read();
    let level_4_address = physical_memory_offset + level_4_frame.start_address().as_u64();
    let level_4_table = unsafe { &mut *(level_4_address as *mut PageTable) };

    let free_entry = level_4_table
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, entry)| entry.is_unused())
        .map(|(index, _)| index as u64)
        .expect("no free level 4 page table entry for device mappings");
    let mmio_region_start = VirtAddr::new_truncate(free_entry * LEVEL_4_ENTRY_SPAN).as_u64();

    let mapper = unsafe { OffsetPageTable::new(level_4_table, VirtAddr::new(physical_memory_offset)) };
    *PAGING.lock() = Some(Paging {
        mapper,
        frame_allocator: BootInfoFrameAllocator { memory_regions, next_frame: 0 },
        mmio_region_start,
        mmio_region_used: 0,
    });
}

/// Maps `size` bytes of device memory (for example video RAM) starting at `physical_address`
/// and returns the virtual address it can be accessed through.
pub fn map_device_memory(physical_address: u64, size: usize) -> Option<*mut u8> {
    let mut paging = PAGING.lock();
    let paging = paging.as_mut()?;

    let page_offset = physical_address % PAGE_SIZE;
    let mapped_size = (page_offset + size as u64 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    if paging.mmio_region_used + mapped_size > LEVEL_4_ENTRY_SPAN {
        return None;
    }

    let virtual_start = paging.mmio_region_start + paging.mmio_region_used;
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;

    for offset in (0..mapped_size).step_by(PAGE_SIZE as usize) {
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_start + offset));
        let frame = PhysFrame::containing_address(PhysAddr::new(physical_address - page_offset + offset));
        let flush = unsafe { paging.mapper.map_to(page, frame, flags, &mut paging.frame_allocator) }.ok()?;
        flush.flush();
    }

    paging.mmio_region_used += mapped_size;
    Some((virtual_start + page_offset) as *mut u8)
}
