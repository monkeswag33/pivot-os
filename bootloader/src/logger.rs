// use spin::{Mutex, RwLock};
// use uart_16550::SerialPort;
// use core::fmt::Write;

// pub static LOGGER: RwLock<LockedLogger> = RwLock::new(LockedLogger::new());

// pub struct LockedLogger {
//     pub serial: Option<Mutex<SerialPort>>
// }

// impl LockedLogger {
//     pub const fn new() -> Self {
//         Self {
//             serial: None
//         }
//     }

//     pub fn set_serial(&mut self, serial: SerialPort) {
//         self.serial = Some(Mutex::new(serial));
//     }
// }

// impl log::Log for LockedLogger {
//     fn enabled(&self, _metadata: &log::Metadata) -> bool {
//         true
//     }

//     fn log(&self, record: &log::Record) {
//         if let Some(serial) = &self.serial {
//             let mut serial = serial.lock();
//             writeln!(serial, "{:5}: {}", record.level(), record.args());
//         }
//     }

//     fn flush(&self) {}
// }
