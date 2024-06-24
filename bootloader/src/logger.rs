use bootloader_api::info::FrameBufferInfo;
use common::framebuffer::FrameBuffer;
use uefi::{proto::console::gop::{GraphicsOutput, PixelFormat}, table::{boot::{OpenProtocolAttributes, OpenProtocolParams}, Boot, SystemTable}, Handle};

use crate::TARGET_FB_RES;

pub fn init_logger(
    image_handle: Handle,
    st: &SystemTable<Boot>,
) -> Option<FrameBuffer> {
    let gop_handle = st
        .boot_services()
        .get_handle_for_protocol::<GraphicsOutput>()
        .ok()?;
    let mut gop = unsafe {
        st.boot_services()
            .open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: gop_handle,
                    agent: image_handle,
                    controller: None,
                },
                OpenProtocolAttributes::Exclusive,
            )
            .ok()?
    };

    let mode = {
        let modes = gop.modes(st.boot_services());
        modes.filter(|m| {
            let res = m.info().resolution();
            res.0 == TARGET_FB_RES.0 && res.1 == TARGET_FB_RES.1
        }).last()
    };
    if let Some(mode) = mode {
        gop.set_mode(&mode)
            .expect("Failed to apply the desired display mode");
    }

    let mode_info = gop.current_mode_info();
    let mut framebuffer = gop.frame_buffer();
    let info = FrameBufferInfo {
        byte_len: framebuffer.size(),
        width: mode_info.resolution().0,
        height: mode_info.resolution().1,
        pixel_format: match mode_info.pixel_format() {
            PixelFormat::Rgb => bootloader_api::info::PixelFormat::Rgb,
            PixelFormat::Bgr => bootloader_api::info::PixelFormat::Bgr,
            PixelFormat::Bitmask | PixelFormat::BltOnly => {
                panic!("Bitmask and BltOnly framebuffers are not supported")
            }
        },
        bytes_per_pixel: 4,
        stride: mode_info.stride(),
    };

    let fb = FrameBuffer {
        buffer: framebuffer.as_mut_ptr(),
        size: framebuffer.size(),
        info
    };

    common::logger::LOGGER.get()?.set_framebuffer(fb);
    log::debug!("{:?}", fb.info);
    log::info!("Loaded framebuffer");
    Some(fb)
}
