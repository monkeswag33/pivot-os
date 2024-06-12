// use spin::Mutex;
// use uart_16550::SerialPort;

// use crate::logger;

// pub fn init() {
//     let mut log = logger::LOGGER.write();
//     let mut serial = unsafe { SerialPort::new(0x3F8) };
//     serial.init();
//     log.serial = Some(Mutex::new(serial));
// }