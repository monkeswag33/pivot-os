use uefi::table::boot::{MemoryDescriptor, MemoryMapIter, MemoryType};
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

/// A physical frame allocator based on a BIOS or UEFI provided memory map.
pub struct MMAPFrameAllocator<'a> {
    iter: MemoryMapIter<'a>,
    cur_desc: Option<&'a MemoryDescriptor>,
    next_frame: PhysFrame
}

impl<'a> MMAPFrameAllocator<'a> {
    pub fn new(iter: MemoryMapIter<'a>) -> Self {
        log::info!("Initialized MMAPFrameAllocator");
        Self {
            iter,
            cur_desc: None,
            next_frame: PhysFrame::containing_address(PhysAddr::new(0x1000))
        }
    }

    fn alloc_from_desc(&mut self, desc: &MemoryDescriptor) -> Option<PhysFrame> {
        let start = PhysFrame::containing_address(PhysAddr::new(desc.phys_start));
        let end = start + desc.page_count - 1;
        if self.next_frame < start {
            self.next_frame = start;
            return Some(start)
        } else if self.next_frame <= end {
            self.next_frame += 1;
            return Some(self.next_frame - 1);
        }
        None
    }

    pub fn next_frame(&self) -> PhysFrame { self.next_frame }
}

unsafe impl FrameAllocator<Size4KiB> for MMAPFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if let Some(desc) = self.cur_desc {
            if let Some(frame) = self.alloc_from_desc(desc) {
                return Some(frame);
            }
            self.cur_desc = None;
        }

        while let Some(desc) = self.iter.next() {
            if desc.ty == MemoryType::CONVENTIONAL {
                self.cur_desc = Some(desc);
                return self.alloc_from_desc(desc);
            }
        }
        None
    }
}
