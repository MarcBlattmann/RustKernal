use x86_64::structures::tss::TaskStateSegment;
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::VirtAddr;
use spin::Lazy;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    
    // Create the main kernel stack (for normal interrupt entry).
    const STACK_SIZE: usize = 4096 * 5;  // 20 KB
    static mut KERNEL_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
    
    // Get the top of the main kernel stack (stacks grow downward).
    let stack_start = VirtAddr::from_ptr(unsafe { &raw const KERNEL_STACK });
    let stack_end = stack_start + STACK_SIZE;
    
    // Set the privileged stack pointer (used when switching from user → kernel).
    tss.privilege_stack_table[0] = stack_end;
    
    // Create a SEPARATE double-fault stack (emergency stack).
    const DOUBLE_FAULT_STACK_SIZE: usize = 4096 * 5;  // 20 KB
    static mut DOUBLE_FAULT_STACK: [u8; DOUBLE_FAULT_STACK_SIZE] = [0; DOUBLE_FAULT_STACK_SIZE];
    
    // Get the top of the double-fault stack.
    let double_fault_stack_start = VirtAddr::from_ptr(unsafe { &raw const DOUBLE_FAULT_STACK });
    let double_fault_stack_end = double_fault_stack_start + DOUBLE_FAULT_STACK_SIZE;
    
    // Set IST[0] to the SEPARATE double-fault stack (its own safe memory).
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = double_fault_stack_end;
    
    return tss;
});

static GDT: Lazy<(GlobalDescriptorTable, SegmentSelector, SegmentSelector)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    
    // Add a kernel code segment descriptor.
    let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
    
    // Add the TSS descriptor (TSS takes 2 slots internally, but add_entry handles it).
    let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
    
    // Return the GDT and both selectors.
    return (gdt, code_selector, tss_selector)
});

pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::registers::segmentation::CS;
    use x86_64::instructions::segmentation::Segment;
    
    // Load the GDT into the CPU (lgdt instruction).
    GDT.0.load();
    
    unsafe {
        // Set the code segment register (CS) to point to the kernel code descriptor.
        CS::set_reg(GDT.1);
        
        // Load the TSS selector into the task register (ltr instruction).
        load_tss(GDT.2);
    }
}