#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use common::BootInfo;
use mem::BitmapFrameAllocator;
use uart_16550::SerialPort;

mod mem;
mod gdt;

#[no_mangle]
pub extern "C" fn _start(boot_info: &'static BootInfo) -> ! {
    common::logger::init_logger(None);
    // gdt::init_gdt();
    // let frame_allocator = BitmapFrameAllocator::new(&boot_info.mmap, boot_info.ffa);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}