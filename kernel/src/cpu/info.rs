use alloc::string::String;
use alloc::vec::Vec;
use core::arch::x86_64::{CpuidResult, __cpuid};

pub struct CpuInfo {
    pub vendor: String,
    pub brand: String,
    pub family: u32,
    pub model: u32,
    pub logical_processors: u32,
    pub features: Vec<&'static str>,
}

#[allow(unused_unsafe)]
fn cpuid(leaf: u32) -> CpuidResult {
    unsafe { __cpuid(leaf) }
}

fn registers_to_string(registers: &[u32]) -> String {
    let bytes: Vec<u8> = registers.iter().flat_map(|register| register.to_le_bytes()).collect();
    String::from_utf8_lossy(&bytes).trim_matches(|c: char| c == '\0' || c == ' ').into()
}

pub fn read() -> CpuInfo {
    let leaf_0 = cpuid(0);
    let vendor = registers_to_string(&[leaf_0.ebx, leaf_0.edx, leaf_0.ecx]);

    let leaf_1 = cpuid(1);
    let base_family = (leaf_1.eax >> 8) & 0xF;
    let family = if base_family == 0xF { base_family + ((leaf_1.eax >> 20) & 0xFF) } else { base_family };
    let base_model = (leaf_1.eax >> 4) & 0xF;
    let model = if base_family >= 0x6 { base_model | ((leaf_1.eax >> 12) & 0xF0) } else { base_model };
    let logical_processors = (leaf_1.ebx >> 16) & 0xFF;

    let brand = if cpuid(0x8000_0000).eax >= 0x8000_0004 {
        let parts: Vec<u32> = (0x8000_0002..=0x8000_0004u32)
            .flat_map(|leaf| {
                let result = cpuid(leaf);
                [result.eax, result.ebx, result.ecx, result.edx]
            })
            .collect();
        registers_to_string(&parts)
    } else {
        String::from("unknown")
    };

    let edx_features = [(0, "fpu"), (4, "tsc"), (5, "msr"), (9, "apic"), (23, "mmx"), (25, "sse"), (26, "sse2"), (28, "htt")];
    let ecx_features = [(0, "sse3"), (9, "ssse3"), (19, "sse4.1"), (20, "sse4.2"), (23, "popcnt"), (25, "aes"), (28, "avx"), (30, "rdrand"), (31, "hypervisor")];
    let mut features = Vec::new();
    for (bit, name) in edx_features {
        if leaf_1.edx & (1 << bit) != 0 {
            features.push(name);
        }
    }
    for (bit, name) in ecx_features {
        if leaf_1.ecx & (1 << bit) != 0 {
            features.push(name);
        }
    }

    CpuInfo { vendor, brand, family, model, logical_processors: logical_processors.max(1), features }
}
