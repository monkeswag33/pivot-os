use core::alloc::GlobalAlloc;

use uefi::{allocator::Allocator, table::boot::{AllocateType, BootServices, MemoryType}};

use crate::{vaddr, KERNEL_PT_ENTRY, PAGE_SIZE};

pub trait FrameAllocator {
    fn alloc(&self) -> u64;
}

pub struct PagingManager<'a, A: FrameAllocator + 'a> {
    pml4: *mut u64,
    alloc: &'a A
}

impl<'a, A: FrameAllocator + 'a> PagingManager<'a, A> {
    pub fn new(alloc: &'a A) -> Self {
        let pml4 = vaddr(alloc.alloc()) as *mut u64;
        Self { pml4, alloc }
    }

    pub fn map(&self, phys: u64, virt: u64, flags: u64) {
        let p4_idx = ((virt >> 39) & 0x1FF) as usize;
        let p3_idx = ((virt >> 30) & 0x1FF) as usize;
        let p2_idx = ((virt >> 21) & 0x1FF) as usize;
        let p1_idx = ((virt >> 12) & 0x1FF) as usize;

        unsafe {
            self.create_table(self.pml4, p4_idx);
            let p3_tbl = *self.pml4.add(p4_idx) as *mut u64;

            self.create_table(p3_tbl, p3_idx);
            let p2_tbl = *p3_tbl.add(p3_idx) as *mut u64;

            self.create_table(p2_tbl, p2_idx);
            let p1_tbl = *p2_tbl.add(p2_idx) as *mut u64;

            *p1_tbl.add(p1_idx) = phys | flags;
        }
    }

    unsafe fn create_table(&self, parent: *mut u64, idx: usize) {
        if (*parent.add(idx) & 1) == 0 {
            let table = self.alloc.alloc();
            *parent.add(idx) = table | KERNEL_PT_ENTRY;
            Self::clean_table(table as *mut u64);
            self.map(table, vaddr(table), KERNEL_PT_ENTRY);
        }
    }

    pub unsafe fn clean_table(table: *mut u64) {
        for i in 0..512 {
            *table.add(i as usize) = 0;
        }
    }
}


pub struct UEFIFrameAllocator<'a> {
    bs: &'a BootServices
}

impl<'a> UEFIFrameAllocator<'a> {
    pub fn new(bs: &'a BootServices) -> Self {
        Self { bs }
    }
}

impl<'a> FrameAllocator for UEFIFrameAllocator<'a> {
    fn alloc(&self) -> u64 {
        self.bs.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).unwrap()
    }
}

pub fn configure_paging(alloc: &UEFIFrameAllocator) {
    let mgr = PagingManager::new(alloc);
}