use common::FrameBufferInfo;
use uefi::{prelude::*, proto::console::gop::GraphicsOutput};

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
    log::info!("Configured graphics");
    FrameBufferInfo {
        width: info.resolution().0,
        height: info.resolution().1,
        stride: info.stride(),
        fb: framebuffer.as_mut_ptr()
    }
}