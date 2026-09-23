use x86_64::instructions::port::Port;

const CONFIG_ADDRESS_PORT: u16 = 0xCF8;
const CONFIG_DATA_PORT: u16 = 0xCFC;

const NO_DEVICE_VENDOR_ID: u16 = 0xFFFF;
const MULTI_FUNCTION_HEADER_BIT: u32 = 0x80;
const COMMAND_MEMORY_SPACE_ENABLED: u32 = 0x02;

const VENDOR_DEVICE_OFFSET: u8 = 0x00;
const COMMAND_OFFSET: u8 = 0x04;
const HEADER_TYPE_OFFSET: u8 = 0x0C;
const FIRST_BAR_OFFSET: u8 = 0x10;

#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub function: u8,
}

fn config_address(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | (bus as u32) << 16
        | (slot as u32) << 11
        | (function as u32) << 8
        | (offset as u32 & 0xFC)
}

fn read_config(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    unsafe {
        Port::<u32>::new(CONFIG_ADDRESS_PORT).write(config_address(bus, slot, function, offset));
        Port::<u32>::new(CONFIG_DATA_PORT).read()
    }
}

fn write_config(bus: u8, slot: u8, function: u8, offset: u8, value: u32) {
    unsafe {
        Port::<u32>::new(CONFIG_ADDRESS_PORT).write(config_address(bus, slot, function, offset));
        Port::<u32>::new(CONFIG_DATA_PORT).write(value);
    }
}

/// Scans every bus for a device with the given vendor and device id.
pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    for bus in 0..=255u8 {
        for slot in 0..32u8 {
            if read_config(bus, slot, 0, VENDOR_DEVICE_OFFSET) as u16 == NO_DEVICE_VENDOR_ID {
                continue;
            }

            let header_type = read_config(bus, slot, 0, HEADER_TYPE_OFFSET) >> 16;
            let function_count = if header_type & MULTI_FUNCTION_HEADER_BIT != 0 { 8 } else { 1 };

            for function in 0..function_count {
                let ids = read_config(bus, slot, function, VENDOR_DEVICE_OFFSET);
                if ids as u16 == vendor_id && (ids >> 16) as u16 == device_id {
                    return Some(PciDevice { bus, slot, function });
                }
            }
        }
    }
    None
}

impl PciDevice {
    fn read(&self, offset: u8) -> u32 {
        read_config(self.bus, self.slot, self.function, offset)
    }

    fn write(&self, offset: u8, value: u32) {
        write_config(self.bus, self.slot, self.function, offset, value)
    }

    /// Returns the physical address and size of a 32-bit memory BAR.
    pub fn memory_bar(&self, index: u8) -> Option<(u64, usize)> {
        let offset = FIRST_BAR_OFFSET + index * 4;
        let original = self.read(offset);
        let is_io_space = original & 0x1 != 0;
        if is_io_space {
            return None;
        }

        // Writing all ones and reading back reveals which address bits are fixed, i.e. the size.
        let command = self.read(COMMAND_OFFSET);
        let size_mask = x86_64::instructions::interrupts::without_interrupts(|| {
            self.write(COMMAND_OFFSET, command & !COMMAND_MEMORY_SPACE_ENABLED);
            self.write(offset, 0xFFFF_FFFF);
            let size_mask = self.read(offset);
            self.write(offset, original);
            self.write(COMMAND_OFFSET, command | COMMAND_MEMORY_SPACE_ENABLED);
            size_mask
        });

        let address = (original & 0xFFFF_FFF0) as u64;
        let size = (!(size_mask & 0xFFFF_FFF0)).wrapping_add(1) as usize;
        if address == 0 || size == 0 {
            return None;
        }
        Some((address, size))
    }
}
