use x86_64::structures::tss::TaskStateSegment;
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::VirtAddr;
use spin::Lazy;

pub const DOUBLE_FAULT_STACK_INDEX: u16 = 0;

const KERNEL_STACK_SIZE: usize = 4096 * 5;
const DOUBLE_FAULT_STACK_SIZE: usize = 4096 * 5;

static TASK_STATE_SEGMENT: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    
    static KERNEL_STACK: [u8; KERNEL_STACK_SIZE] = [0; KERNEL_STACK_SIZE];
    let kernel_stack_bottom = VirtAddr::from_ptr(&raw const KERNEL_STACK);
    let kernel_stack_top = kernel_stack_bottom + KERNEL_STACK_SIZE;
    tss.privilege_stack_table[0] = kernel_stack_top;
    
    static DOUBLE_FAULT_STACK: [u8; DOUBLE_FAULT_STACK_SIZE] = [0; DOUBLE_FAULT_STACK_SIZE];
    let double_fault_stack_bottom = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
    let double_fault_stack_top = double_fault_stack_bottom + DOUBLE_FAULT_STACK_SIZE;
    tss.interrupt_stack_table[DOUBLE_FAULT_STACK_INDEX as usize] = double_fault_stack_top;
    
    tss
});

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    tss: SegmentSelector,
}

static GLOBAL_DESCRIPTOR_TABLE: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    
    let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
    let data_selector = gdt.add_entry(Descriptor::kernel_data_segment());
    let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TASK_STATE_SEGMENT));
    
    (gdt, Selectors { code: code_selector, data: data_selector, tss: tss_selector })
});

pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::registers::segmentation::{CS, DS, SS};
    use x86_64::instructions::segmentation::Segment;
    
    GLOBAL_DESCRIPTOR_TABLE.0.load();
    
    unsafe {
        CS::set_reg(GLOBAL_DESCRIPTOR_TABLE.1.code);
        DS::set_reg(GLOBAL_DESCRIPTOR_TABLE.1.data);
        SS::set_reg(GLOBAL_DESCRIPTOR_TABLE.1.data);
        load_tss(GLOBAL_DESCRIPTOR_TABLE.1.tss);
    }
}
