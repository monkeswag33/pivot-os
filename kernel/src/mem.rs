use core::slice;

use common::PAGE_SIZE;
use uefi::table::boot::{MemoryDescriptor, MemoryMap, MemoryType};
use x86_64::{structures::paging::{FrameAllocator, PhysFrame, Size4KiB}, PhysAddr};

pub struct BitmapFrameAllocator {
    bitmap: &'static mut [u8],
    mem_pages: u64,
    ffa: u64
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
        let mut bitmap = Self {
            bitmap: bm,
            mem_pages,
            ffa: ffa.start_address().as_u64() / PAGE_SIZE
        };

        for entry in mmap.entries().filter(|m| Self::usable_descriptor(m, ffa)) {
            bitmap.clear_area(PhysFrame::containing_address(PhysAddr::new(entry.phys_start)), entry.page_count);
        }
        bitmap
    }

    fn usable_descriptor(m: &MemoryDescriptor, ffa: PhysFrame) -> bool {
        match m.ty {
            MemoryType::CONVENTIONAL
            | MemoryType::BOOT_SERVICES_CODE
            | MemoryType::BOOT_SERVICES_DATA
            | MemoryType::LOADER_CODE
            | MemoryType::LOADER_DATA
                => ffa >= (PhysFrame::containing_address(PhysAddr::new(m.phys_start)) + m.page_count),
            _ => false
        }
    }

    fn search_descriptors<'a>(mmap: &'a MemoryMap<'a>, ffa: PhysFrame, pages: u64) -> Option<PhysFrame> {
        log::trace!("Searching descriptors for bitmap compatible area");
        let desc = mmap.entries().filter(|m| Self::usable_descriptor(m, ffa)).next().unwrap();
        let desc_start = PhysFrame::containing_address(PhysAddr::new(desc.phys_start));
        let end = PhysFrame::containing_address(PhysAddr::new(desc.phys_start)) + desc.page_count;
        let start = ffa.min(desc_start);
        let num_pages = end - start;
        if num_pages >= pages {
            return Some(start)
        }
        None
    }

    fn mem_pages(mmap: &MemoryMap) -> u64 {
        mmap
            .entries()
            .map(|m| {
                m.phys_start / PAGE_SIZE + m.page_count
            })
            .max()
            .unwrap()
    }

    pub fn set_bit(&mut self, frame: PhysFrame) {
        let i = frame.start_address().as_u64() / PAGE_SIZE;
        let row = i / 8;
        let col = i % 8;
        self.bitmap[row as usize] |= 1 << col;
    }

    pub fn clear_bit(&mut self, frame: PhysFrame) {
        let i = frame.start_address().as_u64() / PAGE_SIZE;
        let row = i / 8;
        let col = i % 8;
        self.bitmap[row as usize] &= !(1 << col);
    }

    pub fn set_area(&mut self, frame: PhysFrame, num_pages: u64) {
        for i in 0..num_pages {
            self.set_bit(frame + i);
        }
    }

    pub fn clear_area(&mut self, frame: PhysFrame, num_pages: u64) {
        for i in 0..num_pages {
            self.clear_bit(frame + i);
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        for i in self.ffa..self.bitmap.len() as u64 {
            let row = i / 8;
            let col = i % 8;
            if (self.bitmap[row as usize] & (1 << col)) == 0 {
                let frame = PhysFrame::containing_address(PhysAddr::new(i * PAGE_SIZE));
                self.set_bit(frame);
                return Some(frame);
            }
        }
        None
    }
}