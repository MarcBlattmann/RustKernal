use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use spin::Lazy;
use super::gdt;
use crate::drivers::{pic, keyboard, mouse};

const TIMER_INTERRUPT_VECTOR: usize = 32;
const KEYBOARD_INTERRUPT_VECTOR: usize = 33;
const MOUSE_INTERRUPT_VECTOR: usize = 44; // IRQ12

static INTERRUPT_DESCRIPTOR_TABLE: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    
    unsafe {
        idt.double_fault
            .set_handler_fn(handle_double_fault)
            .set_stack_index(gdt::DOUBLE_FAULT_STACK_INDEX);
    }

    idt[TIMER_INTERRUPT_VECTOR].set_handler_fn(handle_timer_interrupt);
    idt[KEYBOARD_INTERRUPT_VECTOR].set_handler_fn(handle_keyboard_interrupt);
    idt[MOUSE_INTERRUPT_VECTOR].set_handler_fn(handle_mouse_interrupt);

    idt
});

extern "x86-interrupt" fn handle_double_fault(stack_frame: InterruptStackFrame, error_code: u64) -> ! {
    panic!("DOUBLE FAULT (error code: {})\n{:#?}", error_code, stack_frame);
}

extern "x86-interrupt" fn handle_timer_interrupt(_stack_frame: InterruptStackFrame) {
    crate::drivers::timer::tick();
    pic::send_eoi(0);
}

extern "x86-interrupt" fn handle_keyboard_interrupt(_stack_frame: InterruptStackFrame) {
    let scancode = keyboard::read_scancode();
    keyboard::handle_scancode(scancode);
    pic::send_eoi(1);
}

extern "x86-interrupt" fn handle_mouse_interrupt(_stack_frame: InterruptStackFrame) {
    mouse::handle_interrupt();
    pic::send_eoi(12);
}

pub fn init() {
    INTERRUPT_DESCRIPTOR_TABLE.load();
}
