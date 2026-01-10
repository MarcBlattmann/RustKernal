use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use spin::Lazy;
use crate::gdt;

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
    }

    idt.divide_error.set_handler_fn(divide_by_zero_handler);
    
    return idt;
});

extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT (error code: {})\n{:#?}", error_code, stack_frame);
}

extern "x86-interrupt" fn divide_by_zero_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: DIVIDE BY ZERO\n{:#?}", stack_frame);
}

pub fn init() {
    IDT.load();
}