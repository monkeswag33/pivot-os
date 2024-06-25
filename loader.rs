use crate::{create_boot_info, mem::MMAPFrameAllocator, set_up_mappings, SystemInfo, PAGE_SIZE};
use core::{arch::asm, slice};

use common::BootInfo;
use uefi::{cstr16, proto::media::file::{File, FileAttribute, FileInfo, FileMode}, table::{boot::{AllocateType, MemoryType}, Boot, SystemTable}, CStr16};
use x86_64::{
    align_up,
    structures::paging::{
        mapper::{MappedFrame, TranslateResult}, FrameAllocator, Mapper, Page, PageSize, PageTableFlags as Flags, PhysFrame, Size4KiB, Translate
    },
    PhysAddr, VirtAddr,
};
use xmas_elf::{
    header,
    program::{self, ProgramHeader, Type},
    ElfFile,
};

/// Memory addresses required for the context switch.
pub struct Addresses {
    page_table: PhysFrame,
    stack_top: VirtAddr,
    entry_point: VirtAddr,
    boot_info: &'static BootInfo
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

struct Loader<'a, M, F> {
    elf_file: ElfFile<'a>,
    inner: Inner<'a, M, F>
}

struct Inner<'a, M, F> {
    kernel_offset: PhysAddr,
    virtual_address_offset: VirtAddr,
    page_table: &'a mut M,
    frame_allocator: &'a mut F,
}

pub fn load_elf(
    st: &mut SystemTable<Boot>,
) -> Option<Kernel<'static>> {
    let kernel_slice = load_file_from_disk(cstr16!("kernel.elf"), st)?;
    log::info!("Loaded kernel ELF");
    Some(Kernel::parse(kernel_slice))
}

impl<'a, M, F> Loader<'a, M, F>
where
    M: Mapper<Size4KiB> + Translate,
    F: FrameAllocator<Size4KiB>,
{
    fn new(
        kernel: Kernel<'a>,
        page_table: &'a mut M,
        frame_allocator: &'a mut F,
    ) -> Result<Self, &'static str> {
        log::info!("Elf file loaded at {:#p}", kernel.elf.input);
        let kernel_offset = PhysAddr::new(&kernel.elf.input[0] as *const u8 as u64);
        if !kernel_offset.is_aligned(PAGE_SIZE) {
            return Err("Loaded kernel ELF file is not sufficiently aligned");
        }

        let elf_file = kernel.elf;
        for program_header in elf_file.program_iter() {
            program::sanity_check(program_header, &elf_file)?;
        }

        let virtual_address_offset = match elf_file.header.pt2.type_().as_type() {
            header::Type::None => unimplemented!(),
            header::Type::Relocatable => unimplemented!(),
            header::Type::Executable => VirtAddr::zero(),
            header::Type::SharedObject => VirtAddr::new(0xFFFFFFFF80000000),
            header::Type::Core => unimplemented!(),
            header::Type::ProcessorSpecific(_) => unimplemented!(),
        };

        header::sanity_check(&elf_file)?;
        let loader = Loader {
            elf_file,
            inner: Inner {
                kernel_offset,
                virtual_address_offset,
                page_table,
                frame_allocator,
            },
        };

        Ok(loader)
    }

    fn load_segments(&mut self) -> Result<(), &'static str> {
        // Load the segments into virtual memory.
        for program_header in self.elf_file.program_iter() {
            if program_header.get_type()? == Type::Load {
                self.inner.handle_load_segment(program_header)?
            }
        }
        Ok(())
    }

    fn entry_point(&self) -> VirtAddr {
        self.inner.virtual_address_offset + self.elf_file.header.pt2.entry_point()
    }
}

impl<'a, M, F> Inner<'a, M, F>
where
    M: Mapper<Size4KiB> + Translate,
    F: FrameAllocator<Size4KiB>,
{
    fn handle_load_segment(&mut self, segment: ProgramHeader) -> Result<(), &'static str> {
        log::info!("Handling Segment: {:x?}", segment);

        let phys_start_addr = self.kernel_offset + segment.offset();
        let start_frame: PhysFrame = PhysFrame::containing_address(phys_start_addr);
        let end_frame: PhysFrame =
            PhysFrame::containing_address(phys_start_addr + segment.file_size() - 1u64);

        let virt_start_addr = self.virtual_address_offset + segment.virtual_addr();
        let start_page: Page = Page::containing_address(virt_start_addr);

        let mut segment_flags = Flags::PRESENT;
        if !segment.flags().is_execute() {
            segment_flags |= Flags::NO_EXECUTE;
        }
        if segment.flags().is_write() {
            segment_flags |= Flags::WRITABLE;
        }

        // map all frames of the segment at the desired virtual address
        for frame in PhysFrame::range_inclusive(start_frame, end_frame) {
            let offset = frame - start_frame;
            let page = start_page + offset;
            let flusher = unsafe {
                // The parent table flags need to be both readable and writable to
                // support recursive page tables.
                // See https://github.com/rust-osdev/bootloader/issues/443#issuecomment-2130010621
                self.page_table
                    .map_to_with_table_flags(
                        page,
                        frame,
                        segment_flags,
                        Flags::PRESENT | Flags::WRITABLE,
                        self.frame_allocator,
                    )
                    .map_err(|_err| "map_to failed")?
            };
            // we operate on an inactive page table, so there's no need to flush anything
            flusher.ignore();
        }

        // Handle .bss section (mem_size > file_size)
        if segment.mem_size() > segment.file_size() {
            // .bss section (or similar), which needs to be mapped and zeroed
            self.handle_bss_section(&segment, segment_flags)?;
        }

        Ok(())
    }

    fn handle_bss_section(
        &mut self,
        segment: &ProgramHeader,
        segment_flags: Flags,
    ) -> Result<(), &'static str> {
        log::info!("Mapping bss section");

        let virt_start_addr = self.virtual_address_offset + segment.virtual_addr();
        let mem_size = segment.mem_size();
        let file_size = segment.file_size();

        // calculate virtual memory region that must be zeroed
        let zero_start = virt_start_addr + file_size;
        let zero_end = virt_start_addr + mem_size;

        // a type alias that helps in efficiently clearing a page
        type PageArray = [u64; Size4KiB::SIZE as usize / 8];
        const ZERO_ARRAY: PageArray = [0; Size4KiB::SIZE as usize / 8];

        // In some cases, `zero_start` might not be page-aligned. This requires some
        // special treatment because we can't safely zero a frame of the original file.
        let data_bytes_before_zero = zero_start.as_u64() & 0xfff;
        if data_bytes_before_zero != 0 {
            // The last non-bss frame of the segment consists partly of data and partly of bss
            // memory, which must be zeroed. Unfortunately, the file representation might have
            // reused the part of the frame that should be zeroed to store the next segment. This
            // means that we can't simply overwrite that part with zeroes, as we might overwrite
            // other data this way.
            //
            // Example:
            //
            //   XXXXXXXXXXXXXXX000000YYYYYYY000ZZZZZZZZZZZ     virtual memory (XYZ are data)
            //   |·············|     /·····/   /·········/
            //   |·············| ___/·····/   /·········/
            //   |·············|/·····/‾‾‾   /·········/
            //   |·············||·····|/·̅·̅·̅·̅·̅·····/‾‾‾‾
            //   XXXXXXXXXXXXXXXYYYYYYYZZZZZZZZZZZ              file memory (zeros are not saved)
            //   '       '       '       '        '
            //   The areas filled with dots (`·`) indicate a mapping between virtual and file
            //   memory. We see that the data regions `X`, `Y`, `Z` have a valid mapping, while
            //   the regions that are initialized with 0 have not.
            //
            //   The ticks (`'`) below the file memory line indicate the start of a new frame. We
            //   see that the last frames of the `X` and `Y` regions in the file are followed
            //   by the bytes of the next region. So we can't zero these parts of the frame
            //   because they are needed by other memory regions.
            //
            // To solve this problem, we need to allocate a new frame for the last segment page
            // and copy all data content of the original frame over. Afterwards, we can zero
            // the remaining part of the frame since the frame is no longer shared with other
            // segments now.

            let last_page: Page<> = Page::containing_address(virt_start_addr + file_size - 1u64);
            let new_frame = unsafe { self.make_mut(last_page) };
            let new_bytes_ptr = new_frame.start_address().as_u64() as *mut u8;
            unsafe {
                core::ptr::write_bytes(
                    new_bytes_ptr.add(data_bytes_before_zero as usize),
                    0,
                    (Size4KiB::SIZE - data_bytes_before_zero) as usize,
                );
            }
        }

        // map additional frames for `.bss` memory that is not present in source file
        let start_page: Page =
            Page::containing_address(VirtAddr::new(align_up(zero_start.as_u64(), Size4KiB::SIZE)));
        let end_page = Page::containing_address(zero_end - 1u64);
        for page in Page::range_inclusive(start_page, end_page) {
            // allocate a new unused frame
            let frame = self.frame_allocator.allocate_frame().unwrap();

            // zero frame, utilizing identity-mapping
            let frame_ptr = frame.start_address().as_u64() as *mut PageArray;
            unsafe { frame_ptr.write(ZERO_ARRAY) };

            // map frame
            let flusher = unsafe {
                // The parent table flags need to be both readable and writable to
                // support recursive page tables.
                // See https://github.com/rust-osdev/bootloader/issues/443#issuecomment-2130010621
                self.page_table
                    .map_to_with_table_flags(
                        page,
                        frame,
                        segment_flags,
                        Flags::PRESENT | Flags::WRITABLE,
                        self.frame_allocator,
                    )
                    .map_err(|_err| "Failed to map new frame for bss memory")?
            };
            // we operate on an inactive page table, so we don't need to flush our changes
            flusher.ignore();
        }

        Ok(())
    }

    unsafe fn make_mut(&mut self, page: Page) -> PhysFrame {
        let (frame, flags) = match self.page_table.translate(page.start_address()) {
            TranslateResult::Mapped {
                frame,
                offset: _,
                flags,
            } => (frame, flags),
            TranslateResult::NotMapped => panic!("{:?} is not mapped", page),
            TranslateResult::InvalidFrameAddress(_) => unreachable!(),
        };
        let frame = if let MappedFrame::Size4KiB(frame) = frame {
            frame
        } else {
            // We only map 4k pages.
            unreachable!()
        };

        // Allocate a new frame and copy the memory, utilizing that both frames are identity mapped.
        let new_frame = self.frame_allocator.allocate_frame().unwrap();
        let frame_ptr = frame.start_address().as_u64() as *const u8;
        let new_frame_ptr = new_frame.start_address().as_u64() as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(frame_ptr, new_frame_ptr, Size4KiB::SIZE as usize);
        }

        // Replace the underlying frame and update the flags.
        self.page_table.unmap(page).unwrap().1.ignore();
        let new_flags = flags;
        unsafe {
            self.page_table
                .map_to(page, new_frame, new_flags, self.frame_allocator)
                .unwrap()
                .ignore();
        }

        new_frame
    }
}

/// Loads the kernel ELF file given in `bytes` in the given `page_table`.
///
/// Returns the kernel entry point address, it's thread local storage template (if any),
/// and a structure describing which level 4 page table entries are in use.  
pub fn load_kernel(
    kernel: Kernel<'_>,
    page_table: &mut (impl Mapper<Size4KiB> + Translate),
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<VirtAddr, &'static str> {
    let mut loader = Loader::new(kernel, page_table, frame_allocator)?;
    loader.load_segments()?;

    Ok(loader.entry_point())
}

/// Switches to the kernel address space and jumps to the kernel entry point.
pub fn switch_to_kernel(
    system_info: &SystemInfo,
    boot_info: &'static BootInfo,
) -> ! {
    let addresses = Addresses {
        page_table: PhysFrame::containing_address(PhysAddr::new(system_info.page_table.as_ref().unwrap().level_4_table() as *const _ as u64)),
        stack_top: system_info.stack_top.unwrap(),
        entry_point: system_info.entry.unwrap(),
        boot_info
    };

    log::info!(
        "Jumping to kernel entry point at {:?}",
        addresses.entry_point
    );

    unsafe {
        context_switch(addresses);
    }
}

/// Performs the actual context switch.
pub unsafe fn context_switch(addresses: Addresses) -> ! {
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

/// Loads the kernel ELF executable into memory and switches to it.
///
/// This function is a convenience function that first calls [`set_up_mappings`], then
/// [`create_boot_info`], and finally [`switch_to_kernel`]. The given arguments are passed
/// directly to these functions, so see their docs for more info.
pub fn load_and_switch_to_kernel(
    kernel: Kernel,
    frame_allocator: &mut MMAPFrameAllocator,
    system_info: &mut SystemInfo,
) -> ! {
    set_up_mappings(
        kernel,
        frame_allocator,
        system_info
    );
    let boot_info = create_boot_info(
        frame_allocator,
        system_info,
    ).unwrap();
    switch_to_kernel(system_info, boot_info);
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
    unsafe { st.boot_services().free_pool(buf.as_mut_ptr()) }.unwrap();
    let file_ptr = st
        .boot_services()
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            ((file_size - 1) / 4096) + 1,
        )
        .unwrap() as *mut u8;
    unsafe { file_ptr.write_bytes(0, file_size) };
    let file_slice = unsafe { slice::from_raw_parts_mut(file_ptr, file_size) };
    file.read(file_slice).unwrap();

    Some(file_slice)
}
