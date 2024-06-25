#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(step_trait)]

use common::{framebuffer::FrameBuffer, BootInfo, HIGHER_HALF, PAGE_SIZE};
use mem::MMAPFrameAllocator;
use loader::Kernel;
use core::slice;
use uefi::{
    prelude::{entry, Boot, Handle, Status, SystemTable}, table::{boot::{MemoryMap, MemoryType}, cfg}
};
use x86_64::{
    structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

mod mem;
mod loader;
mod logger;

const TARGET_FB_RES: (usize, usize) = (1024, 768);

/// Required system information that should be queried from the BIOS or UEFI firmware.
pub struct SystemInfo {
    pub framebuffer: Option<FrameBuffer>,
    pub rsdp_addr: Option<PhysAddr>,
    pub kernel_slice_start: Option<PhysAddr>,
    pub kernel_slice_len: Option<u64>,
    pub stack_top: Option<VirtAddr>,
    pub page_table: Option<OffsetPageTable<'static>>,
    pub entry: Option<VirtAddr>,
    pub mmap_start: Option<PhysAddr>,
    pub mmap_len: Option<u64>,
    pub mmap_ent_size: Option<u64>
}

#[entry]
fn efi_main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    common::logger::init_logger(None);
    log::info!("UEFI Bootloader started");
    let kernel = loader::load_elf(&mut st).expect("Failed to load kernel");

    let framebuffer = logger::init_logger(image, &st);

    let (system_table, mut mmap) = st.exit_boot_services(MemoryType::LOADER_DATA);
    log::info!("Exited boot services");
    
    let mmap_len = mmap.entries().len() as u64;
    let mmap_buf = mmap.get(0).unwrap() as *const _ as u64;
    let mmap_entry_size = mmap.get(1).unwrap() as *const _ as u64 - mmap_buf;
    let mmap_size = mmap_len * mmap_entry_size;
    mmap.sort();
    
    let mut frame_allocator = MMAPFrameAllocator::new(mmap.entries());
    let pml4_frame = frame_allocator.allocate_frame().unwrap();
    let pml4 = unsafe { &mut *(pml4_frame.start_address().as_u64() as *mut _) };
    *pml4 = PageTable::new();
    let mut page_table = unsafe { OffsetPageTable::new(pml4, VirtAddr::zero()) };
    unsafe { page_table.identity_map(pml4_frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE, &mut frame_allocator) }.unwrap().ignore();

    // let page_tables = mem::create_page_tables(&mut frame_allocator);
    let mut system_info = SystemInfo {
        framebuffer,
        rsdp_addr: {
            let mut config_entries = system_table.config_table().iter();
            // look for an ACPI2 RSDP first
            let acpi2_rsdp = config_entries.find(|entry| matches!(entry.guid, cfg::ACPI2_GUID));
            // if no ACPI2 RSDP is found, look for a ACPI1 RSDP
            let rsdp = acpi2_rsdp
                .or_else(|| config_entries.find(|entry| matches!(entry.guid, cfg::ACPI_GUID)));
            rsdp.map(|entry| PhysAddr::new(entry.address as u64))
        },
        page_table: Some(page_table),
        mmap_start: Some(PhysAddr::new(mmap_buf)),
        mmap_len: Some(mmap_size),
        mmap_ent_size: Some(mmap_entry_size),
        kernel_slice_len: None,
        kernel_slice_start: None,
        stack_top: None,
        entry: None,
    };

    loader::load_and_switch_to_kernel(
        kernel,
        &mut frame_allocator,
        &mut system_info,
    );
}

/// Sets up mappings for a kernel stack and the framebuffer.
///
/// The `kernel_bytes` slice should contain the raw bytes of the kernel ELF executable. The
/// `frame_allocator` argument should be created from the memory map. The `page_tables`
/// argument should point to the bootloader and kernel page tables. The function tries to parse
/// the ELF file and create all specified mappings in the kernel-level page table.
///
/// The `framebuffer_addr` and `framebuffer_size` fields should be set to the start address and
/// byte length the pixel-based framebuffer. These arguments are required because the functions
/// maps this framebuffer in the kernel-level page table, unless the `map_framebuffer` config
/// option is disabled.
///
/// This function reacts to unexpected situations (e.g. invalid kernel ELF file) with a panic, so
/// errors are not recoverable.
pub fn set_up_mappings(
    kernel: Kernel,
    frame_allocator: &mut MMAPFrameAllocator,
    system_info: &mut SystemInfo
) -> Option<()> {
    let kernel_stack_size = PAGE_SIZE;
    let kernel_page_table = system_info.page_table.as_mut()?;

    // Enable support for the no-execute bit in page tables.
    use x86_64::registers::control::{Efer, EferFlags};
    unsafe { Efer::update(|efer| *efer |= EferFlags::NO_EXECUTE_ENABLE) }
    // Make the kernel respect the write-protection bits even when in ring 0 by default
    use x86_64::registers::control::{Cr0, Cr0Flags};
    unsafe { Cr0::update(|cr0| *cr0 |= Cr0Flags::WRITE_PROTECT) };

    system_info.kernel_slice_start = Some(PhysAddr::new(kernel.start_address as _));
    system_info.kernel_slice_len = Some(u64::try_from(kernel.len).unwrap());

    let entry_point = loader::load_kernel(
        kernel,
        kernel_page_table,
        frame_allocator,
    ).expect("no entry point");
    log::info!("Entry point at: {:#x}", entry_point.as_u64());
    system_info.entry = Some(entry_point);
    // create a stack
    let stack_phys_start = frame_allocator.allocate_frame().unwrap();
    let stack_virt_start = Page::containing_address(VirtAddr::new(stack_phys_start.start_address().as_u64()) + HIGHER_HALF);
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    system_info.stack_top = Some(stack_virt_start.start_address() + kernel_stack_size);
    unsafe { kernel_page_table.map_to(stack_virt_start, stack_phys_start, flags, frame_allocator).unwrap().ignore() };

    // identity-map context switch function, so that we don't get an immediate pagefault
    // after switching the active page table
    let context_switch_function = PhysAddr::new(loader::context_switch as *const () as u64);
    let context_switch_function_start_frame: PhysFrame =
        PhysFrame::containing_address(context_switch_function);
    for frame in PhysFrame::range_inclusive(
        context_switch_function_start_frame,
        context_switch_function_start_frame + 1,
    ) {
        let page = Page::containing_address(VirtAddr::new(frame.start_address().as_u64()));
        unsafe {
            // The parent table flags need to be both readable and writable to
            // support recursive page tables.
            // See https://github.com/rust-osdev/bootloader/issues/443#issuecomment-2130010621
            kernel_page_table.map_to_with_table_flags(
                page,
                frame,
                PageTableFlags::PRESENT,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                frame_allocator,
            ).unwrap().ignore();
        }
    }

    Some(())
}

/// Allocates and initializes the boot info struct and the memory map.
///
/// The boot info and memory map are mapped to both the kernel and bootloader
/// address space at the same address. This makes it possible to return a Rust
/// reference that is valid in both address spaces. The necessary physical frames
/// are taken from the given `frame_allocator`.
pub fn create_boot_info(
    frame_allocator: &mut MMAPFrameAllocator,
    system_info: &mut SystemInfo,
) -> Option<&'static BootInfo> {
    let kernel_table = system_info.page_table.as_mut()?;
    let mmap_start = system_info.mmap_start?;
    let mmap_frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(mmap_start);
    let mmap_end = mmap_start + system_info.mmap_len? - 1;
    let mmap_end_frame = PhysFrame::containing_address(mmap_end);
    for frame in PhysFrame::range_inclusive(mmap_frame, mmap_end_frame) {
        let page = Page::containing_address(VirtAddr::new(frame.start_address().as_u64()));
        unsafe {
            kernel_table.map_to(page, frame, PageTableFlags::WRITABLE | PageTableFlags::PRESENT, frame_allocator).unwrap().ignore()
        };
    }

    let boot_info_frame = frame_allocator.allocate_frame().unwrap();
    let boot_info_page = Page::containing_address(VirtAddr::new(boot_info_frame.start_address().as_u64()));
    unsafe { kernel_table.map_to(boot_info_page, boot_info_frame, PageTableFlags::WRITABLE | PageTableFlags::PRESENT, frame_allocator).unwrap().ignore() };

    let boot_info = unsafe { &mut *(boot_info_frame.start_address().as_u64() as *mut BootInfo) };
    boot_info.framebuffer = system_info.framebuffer;
    boot_info.mmap = MemoryMap::from_raw(unsafe {
        slice::from_raw_parts_mut(system_info.mmap_start?.as_u64() as *mut u8, system_info.mmap_len? as usize)
    }, system_info.mmap_ent_size? as usize);
    boot_info.rsdp = system_info.rsdp_addr;
    boot_info.ffa = frame_allocator.next_frame();
    log::info!("Created boot info");
    Some(boot_info)
}
