use linked_list_allocator::LockedHeap;

const MIB: usize = 1024 * 1024;
// Large enough for a 1920x1080 back buffer plus a full-screen off-screen surface.
const HEAP_SIZE: usize = 32 * MIB;
static mut HEAP_SPACE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

#[allow(static_mut_refs)]
pub fn init_heap() {
    unsafe {
        ALLOCATOR.lock().init(HEAP_SPACE.as_mut_ptr(), HEAP_SIZE);
    }
}

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();
