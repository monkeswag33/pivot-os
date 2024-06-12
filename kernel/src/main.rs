#![no_std]
#![no_main]

use core::panic::PanicInfo;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};

const CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.kernel_stack_size = 4096;
    config
};

entry_point!(kernel_main, config = &CONFIG);

fn kernel_main(_bi: &'static mut BootInfo) -> ! {
    loop {}
}

#[panic_handler]
fn panic_handler(_info: &PanicInfo) -> ! {
    loop {}
}