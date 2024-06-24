use core::{borrow::Borrow, cell::{Cell, RefCell}, fmt::Write, sync::atomic::{AtomicBool, Ordering}};
use spin::{Mutex, Once, RwLock};
use uart_16550::SerialPort;

use crate::framebuffer::{FrameBuffer, FrameBufferWriter};

/// The global logger instance used for the `log` crate.
pub static LOGGER: Once<LockedLogger> = Once::new();
/// A logger instance protected by a spinlock.
#[derive(Debug)]
pub struct LockedLogger {
    framebuffer: RefCell<Option<FrameBufferWriter>>,
    serial: RefCell<SerialPort>,
}

unsafe impl Sync for LockedLogger {}

impl LockedLogger {
    /// Create a new instance that logs to the given framebuffer.
    pub fn new(fb: Option<FrameBuffer>, serial: SerialPort) -> Self {
        let fb = RefCell::new(
            fb.map(|f| FrameBufferWriter::new(f))
        );
        LockedLogger {
            framebuffer: fb,
            serial: RefCell::new(serial)
        }
    }

    pub fn set_framebuffer(&self, fb: FrameBuffer) {
        self.framebuffer.replace_with(|_| Some(FrameBufferWriter::new(fb)));
    }
}

impl log::Log for LockedLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let target = record.target().split("::").last().unwrap();
        let mut fb = self.framebuffer.borrow_mut();
        fb.as_mut().map(|f| {
            writeln!(f, "[{}] {}: {}", record.level(), target, record.args()).unwrap();
        });

        writeln!(self.serial.borrow_mut(), "[{}] {}: {}", record.level(), target, record.args()).unwrap();
    }

    fn flush(&self) {}
}

pub fn init_logger(fb: Option<FrameBuffer>) {
    let mut port = unsafe { SerialPort::new(0x3F8) };
    port.init();
    let logger = LockedLogger::new(fb, port);
    log::set_logger(LOGGER.call_once(|| logger)).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
}