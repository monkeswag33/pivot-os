#![no_std]
#![no_main]
use uefi::prelude::*;

pub const PAGE_SIZE: u64 = 4096;
pub const HIGHER_HALF_OFFSET: u64 = 0xFFFF800000000000;
pub const KERNEL_PT_ENTRY: u64 = 0b11;

mod acpi;
mod graphics;
mod loader;
mod mem;

#[inline]
pub fn align_addr(address: u64) -> u64 {
    address & !(PAGE_SIZE as u64 - 1)
}

#[inline]
pub fn align_addr_up(address: u64) -> u64 {
    align_addr(address + PAGE_SIZE - 1)
}

#[inline]
pub fn vaddr(address: u64) -> u64 {
    address | HIGHER_HALF_OFFSET
}

#[inline]
pub fn paddr(address: u64) -> u64 {
    address & !HIGHER_HALF_OFFSET
}

#[entry]
fn main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    st.stdout().clear().expect("Error clearing the screen");
    let alloc = mem::UEFIFrameAllocator::new(st.boot_services());
    let acpi_info = acpi::configure_acpi(&st);
    mem::configure_paging(&alloc);
    // loader::load_kernel(st.boot_services());
    // let fb_info = configure_graphics(&st);
    loop {};
    // loader::load_kernel(image, &mut st);
    Status::SUCCESS
}
