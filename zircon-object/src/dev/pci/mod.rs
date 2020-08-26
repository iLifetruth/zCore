// use super::*;
mod bus;
mod caps;
mod config;
mod constants;
mod nodes;
mod pio;

pub(crate) use nodes::*;
use pio::*;

pub use self::bus::{
    MmioPcieAddressProvider, PCIeBusDriver, PcieDeviceInfo, PcieDeviceKObject,
    PioPcieAddressProvider,
};
pub use self::constants::*;
pub use self::nodes::PcieIrqMode;
pub use self::pio::{pio_config_read, pio_config_write};

#[derive(PartialEq, Debug)]
pub enum PciAddrSpace {
    MMIO,
    PIO,
}

#[repr(C)]
#[derive(Debug)]
pub struct PciInitArgsAddrWindows {
    pub base: u64,
    pub size: usize,
    pub bus_start: u8,
    pub bus_end: u8,
    pub cfg_space_type: u8,
    pub has_ecam: bool,
    pub padding1: [u8; 4],
}

#[repr(C)]
pub struct PciInitArgsIrqs {
    pub global_irq: u32,
    pub level_triggered: bool,
    pub active_high: bool,
    pub padding1: [u8; 2],
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PciIrqSwizzleLut(
    [[[u32; PCI_MAX_LEGACY_IRQ_PINS]; PCI_MAX_FUNCTIONS_PER_DEVICE]; PCI_MAX_DEVICES_PER_BUS],
);

#[repr(C)]
pub struct PciInitArgsHeader {
    pub dev_pin_to_global_irq: PciIrqSwizzleLut,
    pub num_irqs: u32,
    pub irqs: [PciInitArgsIrqs; PCI_MAX_IRQS],
    pub addr_window_count: u32,
}

pub struct PciEcamRegion {
    pub phys_base: u64,
    pub size: usize,
    pub bus_start: u8,
    pub bus_end: u8,
}

pub struct MappedEcamRegion {
    pub ecam: PciEcamRegion,
    pub vaddr: u64,
}

use super::*;
use alloc::sync::*;
use kernel_hal::InterruptManager;

impl PciInitArgsHeader {
    pub fn configure_interrupt(&mut self) -> ZxResult {
        for i in 0..self.num_irqs as usize {
            let irq = &mut self.irqs[i];
            let global_irq = irq.global_irq;
            if !is_valid_interrupt(global_irq) {
                irq.global_irq = PCI_NO_IRQ_MAPPING;
                self.dev_pin_to_global_irq.remove_irq(global_irq);
            } else {
                irq_configure(
                    global_irq,
                    irq.level_triggered, /* Trigger mode */
                    irq.active_high,     /* Polarity */
                )?;
            }
        }
        Ok(())
    }
}

fn is_valid_interrupt(irq: u32) -> bool {
    InterruptManager::is_valid(irq)
}

fn irq_configure(irq: u32, level_trigger: bool, active_high: bool) -> ZxResult {
    // In fuchsia source code, 'BSP' stands for bootstrap processor
    let dest = kernel_hal::apic_local_id();
    if InterruptManager::configure(
        irq,
        0, // vector
        dest,
        level_trigger,
        active_high,
    ) {
        Ok(())
    } else {
        Err(ZxError::INVALID_ARGS)
    }
}

impl PciIrqSwizzleLut {
    fn swizzle(&self, dev_id: usize, func_id: usize, pin: usize) -> ZxResult<usize> {
        if dev_id >= PCI_MAX_DEVICES_PER_BUS
            || func_id >= PCI_MAX_FUNCTIONS_PER_DEVICE
            || pin >= PCI_MAX_LEGACY_IRQ_PINS
        {
            return Err(ZxError::INVALID_ARGS);
        }
        let irq = self.0[dev_id][func_id][pin];
        if irq == PCI_NO_IRQ_MAPPING {
            Err(ZxError::NOT_FOUND)
        } else {
            Ok(irq as usize)
        }
    }

    fn remove_irq(&mut self, irq: u32) {
        for dev in self.0.iter_mut() {
            for func in dev.iter_mut() {
                for pin in func.iter_mut() {
                    if *pin == irq {
                        *pin = PCI_NO_IRQ_MAPPING;
                    }
                }
            }
        }
    }
}
