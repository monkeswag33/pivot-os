use core::slice;
use elf::{abi::PT_LOAD, endian::LittleEndian, segment::ProgramHeader, ElfBytes};
use uefi::{prelude::*, proto::media::file::{File, FileAttribute, FileInfo, FileMode, RegularFile}, table::boot::MemoryType};
use x86_64::structures::paging::Page;

use crate::{allocate_pages, mem::{CFromInto, PagingManager, UEFIFrameAllocator}, KERNEL_OFFSET, KERNEL_PT_ENTRY, PAGE_SIZE};

pub fn load_kernel(bs: &BootServices, pmgr: &mut PagingManager<UEFIFrameAllocator>) -> u64 {
    let mut file = get_file(bs);
    let buffer = unsafe {
        // TODO: Handle buffer too small
        slice::from_raw_parts_mut(
            bs
                .allocate_pool(MemoryType::LOADER_DATA, 128)
                .expect("Error allocating file info buffer"),
            128
        )
    };
    let info = file
        .get_info::<FileInfo>(buffer)
        .expect("Error retrieving file info");

    let buffer_size: usize = info.file_size().try_into().unwrap();
    let buffer = unsafe {
        slice::from_raw_parts_mut(bs
            .allocate_pool(MemoryType::LOADER_DATA, buffer_size)
            .expect("Error allocating ELF Header buffer"), buffer_size)
    };

    file.set_position(0).unwrap();
    file.read(buffer).expect("Error reading ELF Header from file");
    let elf_file = ElfBytes::<LittleEndian>::minimal_parse(buffer)
        .expect("Error parsing ELF");
    for segment in {
        elf_file.segments().expect("Error reading ELF segments")
            .iter().filter(|s| s.p_type == PT_LOAD)
    } {
        load_segment(bs, pmgr, segment, &mut file);
    }
    log::info!("Loaded kernel segments");
    elf_file.ehdr.e_entry
}

fn get_file(bs: &BootServices) -> RegularFile {
    let mut fsp = bs.get_image_file_system(bs.image_handle())
        .expect("Failed to open SimpleFileSystem protocol");

    let mut root_dir = fsp.open_volume()
        .expect("Error opening root directory");

    let file = root_dir
        .open(cstr16!("kernel.elf"), FileMode::Read, FileAttribute::READ_ONLY)
        .expect("Error opening kernel ELF file");
    file.into_regular_file().expect("Failed to turn FileHandle -> RegularFile")
}

fn load_segment(bs: &BootServices, pmgr: &mut PagingManager<UEFIFrameAllocator>, segment: ProgramHeader, file: &mut RegularFile) {
    let mem_offset = segment.p_vaddr % PAGE_SIZE;
    let num_pages = (segment.p_memsz + mem_offset).div_ceil(PAGE_SIZE);
    let range = allocate_pages(bs, num_pages as usize);
    let buffer = unsafe {
        slice::from_raw_parts_mut(
            (range.start.cinto() + mem_offset) as *mut u8,
            segment.p_memsz as usize
        )
    };
    file.set_position(segment.p_offset).expect("Error setting position of file");
    file.read(buffer).expect("Error reading segment data");
    let vaddr = Page::cfrom(KERNEL_OFFSET + segment.p_vaddr);
    pmgr.map_range(
        range.start,
        vaddr,
        KERNEL_PT_ENTRY,
        range.end - range.start
    );
    log::debug!("Mapped {} pages {:?} to {:?}", num_pages, range.start, vaddr);
}
