#![no_std]
#![no_main]
use core::arch::asm;

use common::BootInfo;
use mem::CFromInto;
use uart_16550::SerialPort;
use uefi::{prelude::*, table::boot::{AllocateType, MemoryType}};
use x86_64::structures::paging::{frame::PhysFrameRange, PageTableFlags, PhysFrame};

pub const PAGE_SIZE: u64 = 4096;
pub const HIGHER_HALF_OFFSET: u64 = 0xFFFF800000000000;
pub const KERNEL_PT_ENTRY: PageTableFlags = PageTableFlags::from_bits_retain(0b11);
pub const KERNEL_OFFSET: u64 = 0xFFFFFFFF80000000;

mod acpi;
mod graphics;
mod loader;
mod mem;

pub fn allocate_pages(bs: &BootServices, num_pages: usize) -> PhysFrameRange {
    let addr = PhysFrame::cfrom(bs.allocate_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        num_pages
    ).expect("Failed to allocate pages"));
    PhysFrame::range(addr, addr + num_pages as u64)
}

#[entry]
fn efi_main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    st.stdout().clear().expect("Error clearing the screen");
    let bs = st.boot_services();
    let mut alloc = mem::UEFIFrameAllocator::new(bs);
    let acpi_info = acpi::configure_acpi(&st);
    let mut mgr = mem::PagingManager::new(&mut alloc);
    let entry_addr = loader::load_kernel(bs, &mut mgr) + KERNEL_OFFSET;
    log::debug!("Kernel entry point: {:#x}", entry_addr);
    let stack_top = mem::parse_mmap(bs, &mem::get_mmap(bs), &mut mgr);
    let fb_info = graphics::configure_graphics(&st);
    unsafe { mgr.load_table() };
    let entry: extern "C" fn(&BootInfo) -> ! = unsafe { core::mem::transmute(0xffffffff800013d0 as u64) };
    let (rs, mmap) = st.exit_boot_services(MemoryType::LOADER_DATA);
    let boot_info = BootInfo {
        fb: fb_info,
        acpi: acpi_info,
        runtime: rs,
        mmap
    };
    // entry(&boot_info);
    unsafe {
        // asm!("jmp {}", in(reg) entry_addr);
        asm!(
            r#"
            xor rbp, rbp
            mov rsp, {}
            push 0
            jmp {}
            "#,
            in(reg) stack_top.cinto(),
            in(reg) entry_addr
        );
    }
    loop {}
}
