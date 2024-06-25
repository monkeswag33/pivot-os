#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(naked_functions)]
#![feature(asm_const)]
use core::panic::PanicInfo;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};

mod logger;
mod drivers;
mod cpu;

const CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.kernel_stack_size = 4096;
    config
};

entry_point!(kernel_main, config = &CONFIG);
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    logger::init_logger(boot_info.framebuffer.as_mut());
    log::info!("Entered kernel");

    cpu::gdt::init_gdt();
    cpu::idt::init_idt();
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}