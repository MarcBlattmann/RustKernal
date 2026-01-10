const APIC_BASE_MODEL_SPECIFIC_REGISTER: u32 = 0x1B;
const APIC_ENABLED_BIT: u64 = 1 << 11;

pub fn disable() {
    unsafe {
        let current_value = read_model_specific_register(APIC_BASE_MODEL_SPECIFIC_REGISTER);
        let value_with_apic_disabled = current_value & !APIC_ENABLED_BIT;
        write_model_specific_register(APIC_BASE_MODEL_SPECIFIC_REGISTER, value_with_apic_disabled);
    }
}

unsafe fn read_model_specific_register(register: u32) -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") register,
            out("eax") low,
            out("edx") high,
        );
    }
    ((high as u64) << 32) | (low as u64)
}

unsafe fn write_model_specific_register(register: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") register,
            in("eax") low,
            in("edx") high,
        );
    }
}
