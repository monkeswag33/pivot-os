#![no_std]
#![no_main]

use core::panic::PanicInfo;

use uart_16550::SerialPort;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut serial_port = unsafe { SerialPort::new(0x3F8) };
    serial_port.init();
    serial_port.send(97);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}