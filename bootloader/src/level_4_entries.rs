use crate::load_kernel::VirtualAddressOffset;
use x86_64::{
    structures::paging::{Page, PageTableIndex, Size4KiB},
    VirtAddr,
};
use xmas_elf::program::ProgramHeader;

/// Keeps track of used entries in a level 4 page table.
///
/// Useful for determining a free virtual memory block, e.g. for mapping additional data.
pub struct UsedLevel4Entries {
    /// Whether an entry is in use by the kernel.
    entry_state: [bool; 512],
}

impl UsedLevel4Entries {
    /// Initializes a new instance.
    ///
    /// Marks the statically configured virtual address ranges from the config as used.
    pub fn new() -> Self {
        let mut used = UsedLevel4Entries {
            entry_state: [false; 512],
        };

        used.entry_state[0] = true; // TODO: Can we do this dynamically?

        used
    }

    /// Marks all p4 entries in the range `[address..address+size)` as used.
    ///
    /// `size` can be a `u64` or `usize`.
    fn mark_range_as_used<S>(&mut self, address: u64, size: S)
    where
        VirtAddr: core::ops::Add<S, Output = VirtAddr>,
    {
        let start = VirtAddr::new(address);
        let end_inclusive = (start + size) - 1;
        let start_page = Page::<Size4KiB>::containing_address(start);
        let end_page_inclusive = Page::<Size4KiB>::containing_address(end_inclusive);

        for p4_index in u16::from(start_page.p4_index())..=u16::from(end_page_inclusive.p4_index())
        {
            self.mark_p4_index_as_used(PageTableIndex::new(p4_index));
        }
    }

    fn mark_p4_index_as_used(&mut self, p4_index: PageTableIndex) {
        self.entry_state[usize::from(p4_index)] = true;
    }

    /// Marks the virtual address range of all segments as used.
    pub fn mark_segments<'a>(
        &mut self,
        segments: impl Iterator<Item = ProgramHeader<'a>>,
        virtual_address_offset: VirtualAddressOffset,
    ) {
        for segment in segments.filter(|s| s.mem_size() > 0) {
            self.mark_range_as_used(
                virtual_address_offset + segment.virtual_addr(),
                segment.mem_size(),
            );
        }
    }

    /// Returns the first index of a `num` contiguous unused level 4 entries and marks them as
    /// used. If `CONFIG.aslr` is enabled, this will return random contiguous available entries.
    ///
    /// Since this method marks each returned index as used, it can be used multiple times
    /// to determine multiple unused virtual memory regions.
    pub fn get_free_entries(&mut self, num: u64) -> PageTableIndex {
        // Create an iterator over all available p4 indices with `num` contiguous free entries.
        let mut free_entries = self
            .entry_state
            .windows(num as usize)
            .enumerate()
            .filter(|(_, entries)| entries.iter().all(|used| !used))
            .map(|(idx, _)| idx);

        // Choose the free entry index.
        let idx_opt = free_entries.next();
        let Some(idx) = idx_opt else {
            panic!("no usable level 4 entries found ({num} entries requested)");
        };

        // Mark the entries as used.
        for i in 0..num as usize {
            self.entry_state[idx + i] = true;
        }

        PageTableIndex::new(idx.try_into().unwrap())
    }

    /// Returns a virtual address in one or more unused level 4 entries and marks them as used.
    ///
    /// This function calls [`get_free_entries`] internally, so all of its docs applies here
    /// too.
    pub fn get_free_address(&mut self, size: u64, alignment: u64) -> VirtAddr {
        assert!(alignment.is_power_of_two());

        const LEVEL_4_SIZE: u64 = 4096 * 512 * 512 * 512;

        let level_4_entries = (size + (LEVEL_4_SIZE - 1)) / LEVEL_4_SIZE;
        let base = Page::from_page_table_indices_1gib(
            self.get_free_entries(level_4_entries),
            PageTableIndex::new(0),
        )
        .start_address();

        let offset = 0;

        base + offset
    }
}
