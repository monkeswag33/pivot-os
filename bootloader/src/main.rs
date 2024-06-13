#![no_std]
#![no_main]
use core::slice;

use uefi::{prelude::*, proto::{console::gop::{GraphicsOutput, Mode}, loaded_image::LoadedImage}, table::cfg::{ACPI2_GUID, ACPI_GUID}};

struct FrameBufferInfo {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub fb: &'static mut [u8]
}

#[entry]
fn main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    st.stdout().clear().expect("Error clearing the screen");
    configure_acpi(&st);
    // load_kernel(image, &st);
    // let fb_info = configure_graphics(&st);
    loop {};
    // loader::load_kernel(image, &mut st);
    Status::SUCCESS
}

fn load_kernel(image: Handle, st: &SystemTable<Boot>) {
    let loaded_image = st.boot_services()
        .open_protocol_exclusive::<LoadedImage>(image)
        .expect("Failed to open LoadedImage protocol");
    let device_handle = loaded_image.device();
}

fn configure_acpi(st: &SystemTable<Boot>) {
    let mut config_entries = st.config_table().iter();
    let table = config_entries
        .find(|t| t.guid == ACPI2_GUID)
        .or_else(|| config_entries.find(|t| t.guid == ACPI_GUID));
    let table = table.expect("Couldn't find an ACPI table");
    let address = table.address as *const u8;

    if !unsafe { validate_table(address, 20) } {
        panic!("RSDP was not valid");
    }
}

unsafe fn validate_table(table: *const u8, size: usize) -> bool {
    let mut sum = 0u8;
    for i in 0..size {
        sum = sum.wrapping_add(*table.add(i));
    }
    sum == 0
}

fn configure_graphics(st: &SystemTable<Boot>) -> FrameBufferInfo {
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
