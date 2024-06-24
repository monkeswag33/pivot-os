use core::slice;

use common::PAGE_SIZE;
use uefi::table::boot::{MemoryMap, MemoryType};
use x86_64::{structures::paging::PhysFrame, PhysAddr};

pub struct BitmapFrameAllocator {
    bitmap: &'static mut [u8],
    mem_pages: u64
}

impl BitmapFrameAllocator {
    pub fn new(mmap: &MemoryMap, ffa: PhysFrame) -> Self {
        let mem_pages = Self::mem_pages(mmap);
        log::info!("Found {} pages of physical memory", mem_pages);
        let bm_size = mem_pages.div_ceil(8);
        let bm_pages = bm_size.div_ceil(PAGE_SIZE);
        let bm_location = Self::search_descriptors(mmap, ffa, bm_pages)
            .expect("Couldn't find a usable memory region to store the bitmap");
        let bm = unsafe {
            slice::from_raw_parts_mut(
                bm_location.start_address().as_u64() as *mut u8,
                bm_size as usize
            )
        };
        bm.fill(u8::MAX);

        Self {
            bitmap: bm,
            mem_pages
        }
    }

    fn search_descriptors<'a>(mmap: &'a MemoryMap<'a>, ffa: PhysFrame, pages: u64) -> Option<PhysFrame> {
        log::trace!("Searching descriptors for bitmap compatible area");
        for entry in mmap.entries() {
            let desc_start = PhysFrame::containing_address(PhysAddr::new(entry.phys_start));
            let end = PhysFrame::containing_address(PhysAddr::new(entry.phys_start)) + entry.page_count;
            if entry.ty == MemoryType::CONVENTIONAL && ffa >= end {
                let start = ffa.min(desc_start);
                let num_pages = end - start;
                if num_pages >= pages {
                    return Some(start)
                }
            }
        }
        None
    }

    fn mem_pages(mmap: &MemoryMap) -> u64 {
        mmap
            .entries()
            .enumerate()
            .map(|(i, m)| {
                // log::debug!("[{}] {:?}", i, );
                m.phys_start / PAGE_SIZE + m.page_count
            })
            .max()
            .unwrap()
    }
}