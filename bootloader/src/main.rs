#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(step_trait)]

use crate::memory_descriptor::UefiMemoryDescriptor;
use bootloader_api::info::{FrameBufferInfo, MemoryRegion};
use legacy_memory_region::{LegacyFrameAllocator, LegacyMemoryRegion};
use level_4_entries::UsedLevel4Entries;
use log::LevelFilter;
use xmas_elf::ElfFile;
use core::{
    alloc::Layout, arch::asm, mem::MaybeUninit, ptr, slice
};
use uefi::{
    cstr16, prelude::{entry, Boot, Handle, Status, SystemTable}, proto::{
        console::gop::{GraphicsOutput, PixelFormat},
        media::file::{File, FileAttribute, FileInfo, FileMode},
    }, table::boot::{
        AllocateType, MemoryMap, MemoryType, OpenProtocolAttributes, OpenProtocolParams
    }, CStr16
};
use x86_64::{
    structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTable, PageTableFlags, PageTableIndex, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

mod memory_descriptor;
mod framebuffer;
mod legacy_memory_region;
mod level_4_entries;
mod load_kernel;
mod logger;
mod serial;

const PAGE_SIZE: u64 = 4096;
const TARGET_FB_RES: (usize, usize) = (1024, 768);

#[entry]
fn efi_main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    let kernel = load_kernel(&mut st).expect("Failed to load kernel");

    let framebuffer = init_logger_uefi(image, &st);

    log::info!("UEFI bootloader started");

    if let Some(framebuffer) = framebuffer {
        log::info!("Using framebuffer at {:#x}", framebuffer.addr);
    }

    log::trace!("exiting boot services");
    let (system_table, mut memory_map) = st.exit_boot_services(MemoryType::LOADER_DATA);

    memory_map.sort();

    let mut frame_allocator =
        LegacyFrameAllocator::new(memory_map.entries().copied().map(UefiMemoryDescriptor));

    let page_tables = create_page_tables(&mut frame_allocator);
    let system_info = SystemInfo {
        framebuffer,
        rsdp_addr: {
            use uefi::table::cfg;
            let mut config_entries = system_table.config_table().iter();
            // look for an ACPI2 RSDP first
            let acpi2_rsdp = config_entries.find(|entry| matches!(entry.guid, cfg::ACPI2_GUID));
            // if no ACPI2 RSDP is found, look for a ACPI1 RSDP
            let rsdp = acpi2_rsdp
                .or_else(|| config_entries.find(|entry| matches!(entry.guid, cfg::ACPI_GUID)));
            rsdp.map(|entry| PhysAddr::new(entry.address as u64))
        },
    };

    load_and_switch_to_kernel(
        kernel,
        frame_allocator,
        page_tables,
        system_info,
    );
}

fn load_kernel(
    st: &mut SystemTable<Boot>,
) -> Option<Kernel<'static>> {
    let kernel_slice = load_file_from_disk(cstr16!("kernel.elf"), st)?;
    Some(Kernel::parse(kernel_slice))
}

fn load_file_from_disk(
    name: &CStr16,
    st: &SystemTable<Boot>,
) -> Option<&'static mut [u8]> {
    let mut fsp = st.boot_services().get_image_file_system(st.boot_services().image_handle())
        .expect("Failed to open SimpleFileSystem protocol");

    let mut root_dir = fsp.open_volume()
        .expect("Error opening root directory");

    let file = root_dir
        .open(name, FileMode::Read, FileAttribute::READ_ONLY)
        .expect("Error opening kernel ELF file");
    let mut file = file.into_regular_file().expect("Failed to turn FileHandle -> RegularFile");

    let mut buf = unsafe {
        slice::from_raw_parts_mut(
            st.boot_services().allocate_pool(MemoryType::LOADER_DATA, 500).unwrap(),
            500
        )
    };
    let file_info: &mut FileInfo = file.get_info(&mut buf).unwrap();
    let file_size = usize::try_from(file_info.file_size()).unwrap();

    let file_ptr = st
        .boot_services()
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            ((file_size - 1) / 4096) + 1,
        )
        .unwrap() as *mut u8;
    unsafe { ptr::write_bytes(file_ptr, 0, file_size) };
    let file_slice = unsafe { slice::from_raw_parts_mut(file_ptr, file_size) };
    file.read(file_slice).unwrap();

    Some(file_slice)
}

/// Creates page table abstraction types for both the bootloader and kernel page tables.
fn create_page_tables(
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> PageTables {
    // UEFI identity-maps all memory, so the offset between physical and virtual addresses is 0
    let phys_offset = VirtAddr::new(0);

    // copy the currently active level 4 page table, because it might be read-only
    log::trace!("switching to new level 4 table");
    let bootloader_page_table = {
        let old_table = {
            let frame = x86_64::registers::control::Cr3::read().0;
            let ptr: *const PageTable = (phys_offset + frame.start_address().as_u64()).as_ptr();
            unsafe { &*ptr }
        };
        let new_frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate frame for new level 4 table");
        let new_table: &mut PageTable = {
            let ptr: *mut PageTable =
                (phys_offset + new_frame.start_address().as_u64()).as_mut_ptr();
            // create a new, empty page table
            unsafe {
                ptr.write(PageTable::new());
                &mut *ptr
            }
        };

        // copy the first entry (we don't need to access more than 512 GiB; also, some UEFI
        // implementations seem to create an level 4 table entry 0 in all slots)
        new_table[0] = old_table[0].clone();

        // the first level 4 table entry is now identical, so we can just load the new one
        unsafe {
            x86_64::registers::control::Cr3::write(
                new_frame,
                x86_64::registers::control::Cr3Flags::empty(),
            );
            OffsetPageTable::new(&mut *new_table, phys_offset)
        }
    };

    // create a new page table hierarchy for the kernel
    let (kernel_page_table, kernel_level_4_frame) = {
        // get an unused frame for new level 4 page table
        let frame: PhysFrame = frame_allocator.allocate_frame().expect("no unused frames");
        log::info!("New page table at: {:#?}", &frame);
        // get the corresponding virtual address
        let addr = phys_offset + frame.start_address().as_u64();
        // initialize a new page table
        let ptr = addr.as_mut_ptr();
        unsafe { *ptr = PageTable::new() };
        let level_4_table = unsafe { &mut *ptr };
        (
            unsafe { OffsetPageTable::new(level_4_table, phys_offset) },
            frame,
        )
    };

    PageTables {
        bootloader: bootloader_page_table,
        kernel: kernel_page_table,
        kernel_level_4_frame,
    }
}

fn init_logger_uefi(
    image_handle: Handle,
    st: &SystemTable<Boot>,
) -> Option<RawFrameBufferInfo> {
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
    let slice = unsafe { slice::from_raw_parts_mut(framebuffer.as_mut_ptr(), framebuffer.size()) };
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

    log::info!("UEFI boot");

    init_logger(
        slice,
        info,
        LevelFilter::Trace,
        true, true
    );

    Some(RawFrameBufferInfo {
        addr: PhysAddr::new(framebuffer.as_mut_ptr() as u64),
        info,
    })
}

/// Initialize a text-based logger using the given pixel-based framebuffer as output.
pub fn init_logger(
    framebuffer: &'static mut [u8],
    info: FrameBufferInfo,
    log_level: LevelFilter,
    frame_buffer_logger_status: bool,
    serial_logger_status: bool,
) {
    let logger = logger::LOGGER.get_or_init(move || {
        logger::LockedLogger::new(
            framebuffer,
            info,
            frame_buffer_logger_status,
            serial_logger_status,
        )
    });
    log::set_logger(logger).expect("logger already set");
    log::set_max_level(convert_level(log_level));
    log::info!("Framebuffer info: {:?}", info);
}

fn convert_level(level: LevelFilter) -> log::LevelFilter {
    match level {
        LevelFilter::Off => log::LevelFilter::Off,
        LevelFilter::Error => log::LevelFilter::Error,
        LevelFilter::Warn => log::LevelFilter::Warn,
        LevelFilter::Info => log::LevelFilter::Info,
        LevelFilter::Debug => log::LevelFilter::Debug,
        LevelFilter::Trace => log::LevelFilter::Trace,
    }
}

/// Required system information that should be queried from the BIOS or UEFI firmware.
#[derive(Debug, Copy, Clone)]
pub struct SystemInfo {
    /// Information about the (still unmapped) framebuffer.
    pub framebuffer: Option<RawFrameBufferInfo>,
    /// Address of the _Root System Description Pointer_ structure of the ACPI standard.
    pub rsdp_addr: Option<PhysAddr>,
}

/// The physical address of the framebuffer and information about the framebuffer.
#[derive(Debug, Copy, Clone)]
pub struct RawFrameBufferInfo {
    /// Start address of the pixel-based framebuffer.
    pub addr: PhysAddr,
    /// Information about the framebuffer, including layout and pixel format.
    pub info: FrameBufferInfo,
}

pub struct Kernel<'a> {
    pub elf: ElfFile<'a>,
    pub start_address: *const u8,
    pub len: usize,
}

impl<'a> Kernel<'a> {
    pub fn parse(kernel_slice: &'a [u8]) -> Self {
        let kernel_elf = ElfFile::new(kernel_slice).unwrap();
        Kernel {
            elf: kernel_elf,
            start_address: kernel_slice.as_ptr(),
            len: kernel_slice.len(),
        }
    }
}

/// Loads the kernel ELF executable into memory and switches to it.
///
/// This function is a convenience function that first calls [`set_up_mappings`], then
/// [`create_boot_info`], and finally [`switch_to_kernel`]. The given arguments are passed
/// directly to these functions, so see their docs for more info.
pub fn load_and_switch_to_kernel<I, D>(
    kernel: Kernel,
    mut frame_allocator: LegacyFrameAllocator<I, D>,
    mut page_tables: PageTables,
    system_info: SystemInfo,
) -> !
where
    I: ExactSizeIterator<Item = D> + Clone,
    D: LegacyMemoryRegion,
{
    let mut mappings = set_up_mappings(
        kernel,
        &mut frame_allocator,
        &mut page_tables,
    );
    let boot_info = create_boot_info(
        frame_allocator,
        &mut page_tables,
        &mut mappings,
        system_info,
    );
    switch_to_kernel(page_tables, mappings, boot_info);
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
pub fn set_up_mappings<I, D>(
    kernel: Kernel,
    frame_allocator: &mut LegacyFrameAllocator<I, D>,
    page_tables: &mut PageTables,
) -> Mappings
where
    I: ExactSizeIterator<Item = D> + Clone,
    D: LegacyMemoryRegion,
{
    let kernel_stack_size = PAGE_SIZE;
    let kernel_page_table = &mut page_tables.kernel;

    let mut used_entries = UsedLevel4Entries::new();

    // Enable support for the no-execute bit in page tables.
    enable_nxe_bit();
    // Make the kernel respect the write-protection bits even when in ring 0 by default
    enable_write_protect_bit();

    let kernel_slice_start = PhysAddr::new(kernel.start_address as _);
    let kernel_slice_len = u64::try_from(kernel.len).unwrap();

    let (kernel_image_offset, entry_point, tls_template) = load_kernel::load_kernel(
        kernel,
        kernel_page_table,
        frame_allocator,
        &mut used_entries,
    )
    .expect("no entry point");
    log::info!("Entry point at: {:#x}", entry_point.as_u64());
    // create a stack
    let stack_start = {
        // we need page-alignment because we want a guard page directly below the stack
        let guard_page = mapping_addr_page_aligned(
            // allocate an additional page as a guard page
            Size4KiB::SIZE + kernel_stack_size,
            &mut used_entries,
            "kernel stack start",
        );
        guard_page + 1
    };
    let stack_end_addr = stack_start.start_address() + kernel_stack_size;

    let stack_end = Page::containing_address(stack_end_addr - 1u64);
    for page in Page::range_inclusive(stack_start, stack_end) {
        let frame = frame_allocator
            .allocate_frame()
            .expect("frame allocation failed when mapping a kernel stack");
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
        unsafe { kernel_page_table.map_to(page, frame, flags, frame_allocator).unwrap().ignore() };
    }

    // identity-map context switch function, so that we don't get an immediate pagefault
    // after switching the active page table
    let context_switch_function = PhysAddr::new(context_switch as *const () as u64);
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

    Mappings {
        entry_point,
        // Use the configured stack size, even if it's not page-aligned. However, we
        // need to align it down to the next 16-byte boundary because the System V
        // ABI requires a 16-byte stack alignment.
        stack_top: stack_end_addr.align_down(16u8),
        used_entries,
        kernel_slice_start,
        kernel_slice_len,
        kernel_image_offset,
    }
}

pub struct BootInfo {
    pub framebuffer: VirtAddr,
    pub rsdp: VirtAddr,
    pub mmap: MemoryMap<'static>
}

/// Contains the addresses of all memory mappings set up by [`set_up_mappings`].
pub struct Mappings {
    pub framebuffer: VirtAddr,
    /// The entry point address of the kernel.
    pub entry_point: VirtAddr,
    /// The (exclusive) end address of the kernel stack.
    pub stack_top: VirtAddr,
    /// Keeps track of used entries in the level 4 page table, useful for finding a free
    /// virtual memory when needed.
    pub used_entries: UsedLevel4Entries,
    /// Start address of the kernel slice allocation in memory.
    pub kernel_slice_start: PhysAddr,
    /// Size of the kernel slice allocation in memory.
    pub kernel_slice_len: u64,
    /// Relocation offset of the kernel image in virtual memory.
    pub kernel_image_offset: VirtAddr,
}

/// Allocates and initializes the boot info struct and the memory map.
///
/// The boot info and memory map are mapped to both the kernel and bootloader
/// address space at the same address. This makes it possible to return a Rust
/// reference that is valid in both address spaces. The necessary physical frames
/// are taken from the given `frame_allocator`.
pub fn create_boot_info<I, D>(
    mut frame_allocator: LegacyFrameAllocator<I, D>,
    page_tables: &mut PageTables,
    mappings: &mut Mappings,
    system_info: SystemInfo,
) -> &'static mut BootInfo
where
    I: ExactSizeIterator<Item = D> + Clone,
    D: LegacyMemoryRegion,
{
    log::info!("Allocate bootinfo");

    // allocate and map space for the boot info
    let (boot_info, memory_regions) = {
        let boot_info_layout = Layout::new::<BootInfo>();
        let regions = frame_allocator.len() + 4; // up to 4 regions might be split into used/unused
        let memory_regions_layout = Layout::array::<MemoryRegion>(regions).unwrap();
        let (combined, memory_regions_offset) =
            boot_info_layout.extend(memory_regions_layout).unwrap();

        let boot_info_addr = mapping_addr(
            combined.size() as u64,
            combined.align() as u64,
            &mut mappings.used_entries,
        )
        .expect("boot info addr is not properly aligned");

        let memory_map_regions_addr = boot_info_addr + memory_regions_offset as u64;
        let memory_map_regions_end = boot_info_addr + combined.size() as u64;

        let start_page = Page::containing_address(boot_info_addr);
        let end_page = Page::containing_address(memory_map_regions_end - 1u64);
        for page in Page::range_inclusive(start_page, end_page) {
            let flags =
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
            let frame = frame_allocator
                .allocate_frame()
                .expect("frame allocation for boot info failed");
            match unsafe {
                page_tables
                    .kernel
                    .map_to(page, frame, flags, &mut frame_allocator)
            } {
                Ok(tlb) => tlb.flush(),
                Err(err) => panic!("failed to map page {:?}: {:?}", page, err),
            }
            // we need to be able to access it too
            match unsafe {
                page_tables
                    .bootloader
                    .map_to(page, frame, flags, &mut frame_allocator)
            } {
                Ok(tlb) => tlb.flush(),
                Err(err) => panic!("failed to map page {:?}: {:?}", page, err),
            }
        }

        let boot_info: &'static mut MaybeUninit<BootInfo> =
            unsafe { &mut *boot_info_addr.as_mut_ptr() };
        let memory_regions: &'static mut [MaybeUninit<MemoryRegion>] =
            unsafe { slice::from_raw_parts_mut(memory_map_regions_addr.as_mut_ptr(), regions) };
        (boot_info, memory_regions)
    };

    log::info!("Create Memory Map");

    // build memory map
    let memory_regions = frame_allocator.construct_memory_map(
        memory_regions,
        mappings.kernel_slice_start,
        mappings.kernel_slice_len,
    );

    log::info!("Create bootinfo");

    // create boot info
    let boot_info = boot_info.write({
        let mut info = BootInfo::new(memory_regions.into());
        info.framebuffer = mappings;
        info.rsdp_addr = system_info.rsdp_addr.map(|addr| addr.as_u64()).into();
        info.kernel_addr = mappings.kernel_slice_start.as_u64();
        info.kernel_len = mappings.kernel_slice_len as _;
        info.kernel_image_offset = mappings.kernel_image_offset.as_u64();
        info
    });

    boot_info
}

/// Switches to the kernel address space and jumps to the kernel entry point.
pub fn switch_to_kernel(
    page_tables: PageTables,
    mappings: Mappings,
    boot_info: &'static mut BootInfo,
) -> ! {
    let PageTables {
        kernel_level_4_frame,
        ..
    } = page_tables;
    let addresses = Addresses {
        page_table: kernel_level_4_frame,
        stack_top: mappings.stack_top,
        entry_point: mappings.entry_point,
        boot_info,
    };

    log::info!(
        "Jumping to kernel entry point at {:?}",
        addresses.entry_point
    );

    unsafe {
        context_switch(addresses);
    }
}

/// Provides access to the page tables of the bootloader and kernel address space.
pub struct PageTables {
    /// Provides access to the page tables of the bootloader address space.
    pub bootloader: OffsetPageTable<'static>,
    /// Provides access to the page tables of the kernel address space (not active).
    pub kernel: OffsetPageTable<'static>,
    /// The physical frame where the level 4 page table of the kernel address space is stored.
    ///
    /// Must be the page table that the `kernel` field of this struct refers to.
    ///
    /// This frame is loaded into the `CR3` register on the final context switch to the kernel.
    pub kernel_level_4_frame: PhysFrame,
}

/// Performs the actual context switch.
unsafe fn context_switch(addresses: Addresses) -> ! {
    unsafe {
        asm!(
            r#"
            xor rbp, rbp
            mov cr3, {}
            mov rsp, {}
            push 0
            jmp {}
            "#,
            in(reg) addresses.page_table.start_address().as_u64(),
            in(reg) addresses.stack_top.as_u64(),
            in(reg) addresses.entry_point.as_u64(),
            in("rdi") addresses.boot_info as *const _ as usize,
        );
    }
    unreachable!();
}

/// Memory addresses required for the context switch.
struct Addresses {
    page_table: PhysFrame,
    stack_top: VirtAddr,
    entry_point: VirtAddr,
    boot_info: &'static mut BootInfo,
}

fn mapping_addr_page_aligned(
    size: u64,
    used_entries: &mut UsedLevel4Entries,
    kind: &str,
) -> Page {
    match mapping_addr(size, Size4KiB::SIZE, used_entries) {
        Ok(addr) => Page::from_start_address(addr).unwrap(),
        Err(addr) => panic!("{kind} address must be page-aligned (is `{addr:?})`"),
    }
}

fn mapping_addr(
    size: u64,
    alignment: u64,
    used_entries: &mut UsedLevel4Entries,
) -> Result<VirtAddr, VirtAddr> {
    let addr = used_entries.get_free_address(size, alignment);
    if addr.is_aligned(alignment) {
        Ok(addr)
    } else {
        Err(addr)
    }
}

fn enable_nxe_bit() {
    use x86_64::registers::control::{Efer, EferFlags};
    unsafe { Efer::update(|efer| *efer |= EferFlags::NO_EXECUTE_ENABLE) }
}

fn enable_write_protect_bit() {
    use x86_64::registers::control::{Cr0, Cr0Flags};
    unsafe { Cr0::update(|cr0| *cr0 |= Cr0Flags::WRITE_PROTECT) };
}