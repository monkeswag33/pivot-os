#include <stddef.h>
#include "bootparam.h"
#include "paging.h"
#include "log.h"
#include "mem.h"

#define PAGE_BIT_P_PRESENT (1<<0)
#define PAGE_BIT_RW_WRITABLE (1<<1)
#define PAGE_BIT_US_USER (1<<2)
#define PAGE_ADDR_MASK 0x000ffffffffff000

extern void load_pml4(struct page_table_t * pml4);

uint64_t next_alloc_page;

__attribute__((aligned(4096)))
struct page_table_t pml4;

void map_page(uint64_t logical);

void *alloc_page(void) {
    void *page = (void*) next_alloc_page;
    next_alloc_page += 0x1000;
    return page;
}

static void add_entry(uint64_t *entry, int flags) {
    void *table = alloc_page();
    memset(table, 0, 4096);
    uint64_t addr = (uint64_t)(uintptr_t) table;
    *entry = (addr & PAGE_ADDR_MASK) | flags;
    map_page(addr);
}

void map_page(uint64_t logical) {
    int flags = PAGE_BIT_P_PRESENT | PAGE_BIT_RW_WRITABLE | PAGE_BIT_US_USER;

    int pml4_idx = (logical >> 39) & 0x1ff;
    int pdp_idx = (logical >> 30) & 0x1ff;
    int pd_idx = (logical >> 21) & 0x1ff;
    int pt_idx = (logical >> 12) & 0x1ff;

    if (!(pml4.entries[pml4_idx] & PAGE_BIT_P_PRESENT))
        add_entry(&(pml4.entries[pml4_idx]), flags);

    struct page_table_t * pdpt =
        (struct page_table_t*)(pml4.entries[pml4_idx] & PAGE_ADDR_MASK);
    if (!(pdpt->entries[pdp_idx] & PAGE_BIT_P_PRESENT))
        add_entry(&(pdpt->entries[pdp_idx]), flags);

    struct page_table_t * pdt =
        (struct page_table_t*)(pdpt->entries[pdp_idx] & PAGE_ADDR_MASK);
    if (!(pdt->entries[pd_idx] & PAGE_BIT_P_PRESENT))
        add_entry(&(pdt->entries[pd_idx]), flags);

    struct page_table_t * pt =
        (struct page_table_t*)(pdt->entries[pd_idx] & PAGE_ADDR_MASK);
    if (!(pt->entries[pt_idx] & PAGE_BIT_P_PRESENT))
        pt->entries[pt_idx] = (logical & PAGE_ADDR_MASK) | flags;
}

void setup_paging(void) {
    size_t total_memory = 0;
    struct mem_region_t *best_region = NULL;
    for (size_t i = 0; i < bootp->num_regions; i++) {
        struct mem_region_t *region = &bootp->mem_regions[i];
        total_memory += region->num_pages;
        if (region->mem_type == 7)
            if (!best_region || (region->num_pages > best_region->num_pages))
                best_region = region;
    }
    next_alloc_page = best_region->physical_start;
    memset((void*)&pml4, 0, 4096);
    for (size_t i = 0; i < total_memory * 1024 * 1024; i += 0x1000) {
        map_page(i);
    }
    load_pml4(&pml4);
    log(Info, "PAGING", "Paging enabled");
}