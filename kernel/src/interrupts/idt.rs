use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Lazy;
use super::gdt;
use super::pic;

const TIMER_INTERRUPT_VECTOR: usize = 32;

static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

static INTERRUPT_DESCRIPTOR_TABLE: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    
    unsafe {
        idt.double_fault
            .set_handler_fn(handle_double_fault)
            .set_stack_index(gdt::DOUBLE_FAULT_STACK_INDEX);
    }

    idt[TIMER_INTERRUPT_VECTOR].set_handler_fn(handle_timer_interrupt);

    idt
});

extern "x86-interrupt" fn handle_double_fault(stack_frame: InterruptStackFrame, error_code: u64) -> ! {
    panic!("DOUBLE FAULT (error code: {})\n{:#?}", error_code, stack_frame);
}

extern "x86-interrupt" fn handle_timer_interrupt(_stack_frame: InterruptStackFrame) {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    pic::send_eoi(0);
}

pub fn init() {
    INTERRUPT_DESCRIPTOR_TABLE.load();
}

pub fn ticks() -> u64 {
    TIMER_TICKS.load(Ordering::SeqCst)
}
