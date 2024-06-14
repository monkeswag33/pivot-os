use core::slice;

use uefi::{prelude::*, proto::console::gop::GraphicsOutput};

pub struct FrameBufferInfo {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub fb: &'static mut [u8]
}

pub fn configure_graphics(st: &SystemTable<Boot>) -> FrameBufferInfo {
    let gop_handle = st.boot_services()
        .get_handle_for_protocol::<GraphicsOutput>()
        .expect("Error getting GOP handle");
    let mut gop = st.boot_services()
        .open_protocol_exclusive::<GraphicsOutput>(gop_handle)
        .expect("Error getting GOP protocol");
    let mode = gop.modes(st.boot_services())
        .filter(|m| m.info().resolution() == (1024, 768)).next()
        .expect("Couldn't find a video mode with the target resolution");
    gop.set_mode(&mode).expect("Error setting video mode");

    let info = mode.info();
    let mut framebuffer = gop.frame_buffer();
    let fb_slice = unsafe { slice::from_raw_parts_mut(framebuffer.as_mut_ptr(), framebuffer.size()) };
    FrameBufferInfo {
        width: info.resolution().0,
        height: info.resolution().1,
        stride: info.stride(),
        fb: fb_slice
    }
}