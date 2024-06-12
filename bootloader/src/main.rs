#![no_std]
#![no_main]
use uefi::prelude::*;

// pub fn open_protocol(image: Handle, st: SystemTable<Boot>) {
//     let services = st.boot_services();
//     let image = services.open_protocol_exclusive::<LoadedImage>(image);
//     if image.is_err() {
//         log::error!("Failed to open protocol LoadedImage");
//     }
// }

#[entry]
fn main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    log::info!("Hello World");
    st.boot_services().stall(10_000_000);
    // loader::load_kernel(image, &mut st);
    Status::SUCCESS
}
