use core::slice;

use uefi::table::boot::{AllocateType, BootServices, MemoryMap, MemoryType};
use x86_64::{registers::control::Cr3Flags, structures::paging::{mapper::{MapperFlush, PageTableFrameMapping}, FrameAllocator, MappedPageTable, Mapper, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB}, PhysAddr, VirtAddr};

use crate::{allocate_pages, HIGHER_HALF_OFFSET, KERNEL_PT_ENTRY, PAGE_SIZE};

pub struct UEFIFrameAllocator<'a> {
    bs: &'a BootServices
}

impl<'a> UEFIFrameAllocator<'a> {
    pub fn new(bs: &'a BootServices) -> Self {
        Self { bs }
    }
}

/// Common Into - Allows me to implement Into for types that I convert often  
/// Replica of Rust Into but since I implemented it I can impl it on external structs
pub trait CFromInto<T> {
    fn cinto(self) -> T;
    fn cfrom(value: T) -> Self;
}

impl CFromInto<u64> for Page {
    fn cfrom(value: u64) -> Self { Page::containing_address(VirtAddr::new(value)) }
    fn cinto(self) -> u64 { self.start_address().as_u64() }
}

impl CFromInto<u64> for PhysFrame {
    fn cfrom(value: u64) -> Self { PhysFrame::containing_address(PhysAddr::new(value)) }
    fn cinto(self) -> u64 { self.start_address().as_u64() }
}

unsafe impl<'a> FrameAllocator<Size4KiB> for UEFIFrameAllocator<'a> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        Some(PhysFrame::from_start_address(PhysAddr::new(
            self.bs.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).unwrap()
        )).unwrap())
    }
}

fn higher_half<T>(addr: T) -> T where T: CFromInto<u64> {
    T::cfrom(addr.cinto() | HIGHER_HALF_OFFSET)
}

pub struct KernelPageTableWalker;

unsafe impl PageTableFrameMapping for KernelPageTableWalker {
    fn frame_to_pointer(&self, frame: PhysFrame) -> *mut PageTable {
        frame.cinto() as *mut PageTable
    }
}

pub struct PagingManager<'a, A: FrameAllocator<Size4KiB>> {
    table: MappedPageTable<'static, KernelPageTableWalker>,
    alloc: &'a mut A
}

impl<'a, A: FrameAllocator<Size4KiB>> PagingManager<'a, A> {
    pub fn new(alloc: &'a mut A) -> Self {
        let frame = alloc.allocate_frame().unwrap();
        let table = unsafe { &mut *(frame.cinto() as *mut PageTable) };
        for entry in table.iter_mut() {
            entry.set_unused();
        }
        let walker = KernelPageTableWalker {};
        log::info!("Initialized PML4");
        Self {
            table: unsafe { MappedPageTable::new(table, walker) },
            alloc
        }
    }

    pub fn map_range(&mut self, phys: PhysFrame, virt: Page, flags: PageTableFlags, num_pages: u64) {
        // Ignore TLB flush for map_range
        // If caller wants to flush, they have to store the address themselves
        for i in 0..num_pages {
            let _ = self.map(phys + i as u64, virt + i as u64, flags);
        }
    }

    pub fn map(&mut self, phys: PhysFrame, virt: Page, flags: PageTableFlags) -> MapperFlush<Size4KiB> {
        unsafe {
            self.table.map_to_with_table_flags(
                virt,
                phys,
                flags,
                KERNEL_PT_ENTRY,
                self.alloc
            ).unwrap()
        }
    }

    pub unsafe fn load_table(&self) {
        x86_64::registers::control::Cr3::write(
            PhysFrame::from_start_address(PhysAddr::new(self.table.level_4_table() as *const PageTable as u64)).unwrap(),
            Cr3Flags::empty()
        );
    }
}

pub fn get_mmap(bs: &BootServices) -> MemoryMap {
    let mut mmap_size = bs.memory_map_size();
    mmap_size.map_size += mmap_size.entry_size * 4;

    let buffer: &mut [u8] = unsafe { slice::from_raw_parts_mut(bs.allocate_pool(MemoryType::LOADER_DATA, mmap_size.map_size).unwrap(), mmap_size.map_size) };
    bs.memory_map(buffer).expect("Failed to get MMAP")
}

pub fn parse_mmap(bs: &BootServices, mmap: &MemoryMap, pmgr: &mut PagingManager<UEFIFrameAllocator>) -> PhysFrame<Size4KiB> {
    let mut max_address = 0;
    for entry in mmap.entries() {
        let new_max = entry.phys_start + entry.page_count * PAGE_SIZE;
        if new_max > max_address {
            max_address = new_max;
        }
        let start = PhysFrame::cfrom(entry.phys_start);
        let virt = Page::cfrom(entry.phys_start);
        match entry.ty {
            MemoryType::LOADER_CODE | MemoryType::ACPI_RECLAIM | MemoryType::BOOT_SERVICES_CODE |
            MemoryType::BOOT_SERVICES_DATA | MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA 
                => pmgr.map_range(start, virt, KERNEL_PT_ENTRY, entry.page_count),
            MemoryType::LOADER_DATA => {
                pmgr.map_range(start, virt, KERNEL_PT_ENTRY, entry.page_count);
                pmgr.map_range(start, higher_half(virt), KERNEL_PT_ENTRY, entry.page_count);
            }
            MemoryType::CONVENTIONAL
                => pmgr.map_range(start, higher_half(virt), KERNEL_PT_ENTRY, entry.page_count),
            _ => {}
        }
    }
    log::info!("Mapped higher half");

    let mem_pages = max_address / PAGE_SIZE;
    let bitmap_size = mem_pages.div_ceil(64) * 8;
    log::debug!("Bitmap size: {}", bitmap_size);
    let num_pages = bitmap_size.div_ceil(PAGE_SIZE);
    let bitmap = allocate_pages(bs, num_pages as usize).start;
    pmgr.map_range(bitmap, Page::cfrom(bitmap.cinto()), KERNEL_PT_ENTRY, num_pages);

    let stack = allocate_pages(bs, 1).start;
    pmgr.map_range(stack, Page::cfrom(stack.cinto()), KERNEL_PT_ENTRY, 1);
    log::info!("Found bitmap location");
    stack + 1
}