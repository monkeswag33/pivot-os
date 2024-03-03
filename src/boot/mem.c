#include <boot/mem.h>
#include <sys.h>
#define MAX_MAPPED_ADDR 0x40000000
#define NUM_MAPPED_PAGES (128 * 1024 * 1024 / PAGE_SIZE)

EFI_STATUS AllocTable(UINT64 **table) {
    EFI_STATUS status;
    *table = (UINT64*) MAX_MAPPED_ADDR;
    status = uefi_call_wrapper(gBS->AllocatePages, 4, AllocateAnyPages, EfiLoaderData, 1, table);
    if (EFI_ERROR(status)) {
        Print(L"Error allocating a page under 1 MiB\n");
        return status;
    }

    status = uefi_call_wrapper(gBS->SetMem, 3, *table, EFI_PAGE_SIZE, 0);
    if (EFI_ERROR(status)) {
        Print(L"Error zeroing page table\n");
        return status;
    }

    return EFI_SUCCESS;
}

EFI_STATUS MapAddr(EFI_VIRTUAL_ADDRESS virt_addr, EFI_PHYSICAL_ADDRESS phys_addr, UINT64 *p4_tbl) {
    EFI_STATUS status;
    UINTN p4_idx = P4_ENTRY(virt_addr);
    UINTN p3_idx = P3_ENTRY(virt_addr);
    UINTN p2_idx = P2_ENTRY(virt_addr);
    UINTN p1_idx = P1_ENTRY(virt_addr);
    if (!(p4_tbl[p4_idx] & 1)) {
        UINT64 *table;
        status = AllocTable(&table);
        if (EFI_ERROR(status))
            return status;
        p4_tbl[p4_idx] = (EFI_PHYSICAL_ADDRESS) table | 0b11;
        status = MapAddr((EFI_PHYSICAL_ADDRESS) table, (EFI_PHYSICAL_ADDRESS) table, p4_tbl);
        if (EFI_ERROR(status)) {
            Print(L"Error identity mapping PDPT table\n");
            return status;
        }
        status = MapAddr(VADDR((EFI_PHYSICAL_ADDRESS) table), (EFI_PHYSICAL_ADDRESS) table, p4_tbl);
        if (EFI_ERROR(status)) {
            Print(L"Error mapping PDPT table in higher half\n");
            return status;
        }
    }

    UINT64 *p3_tbl = (UINT64*)(p4_tbl[p4_idx] & SIGN_MASK);
    if (!(p3_tbl[p3_idx] & 1)) {
        UINT64 *table;
        status = AllocTable(&table);
        if (EFI_ERROR(status))
            return status;
        p3_tbl[p3_idx] = (EFI_PHYSICAL_ADDRESS) table | 0b11;
        status = MapAddr((EFI_PHYSICAL_ADDRESS) table, (EFI_PHYSICAL_ADDRESS) table, p4_tbl);
        if (EFI_ERROR(status)) {
            Print(L"Error identity mapping PD table\n");
            return status;
        }
        status = MapAddr(VADDR((EFI_PHYSICAL_ADDRESS) table), (EFI_PHYSICAL_ADDRESS) table, p4_tbl);
        if (EFI_ERROR(status)) {
            Print(L"Error mapping PD table in higher half\n");
            return status;
        }
    }

    UINT64 *p2_tbl = (UINT64*)(p3_tbl[p3_idx] & SIGN_MASK);
    if (!(p2_tbl[p2_idx] & 1)) {
        UINT64 *table;
        status = AllocTable(&table);
        if (EFI_ERROR(status))
            return status;
        p2_tbl[p2_idx] = (EFI_PHYSICAL_ADDRESS) table | 0b11;
        status = MapAddr((EFI_PHYSICAL_ADDRESS) table, (EFI_PHYSICAL_ADDRESS) table, p4_tbl);
        if (EFI_ERROR(status)) {
            Print(L"Error identity mapping PT table\n");
            return status;
        }
        status = MapAddr(VADDR((EFI_PHYSICAL_ADDRESS) table), (EFI_PHYSICAL_ADDRESS) table, p4_tbl);
        if (EFI_ERROR(status)) {
            Print(L"Error mapping PT table in higher half\n");
            return status;
        }
    }

    UINT64 *p1_tbl = (UINT64*)(p2_tbl[p2_idx] & SIGN_MASK);
    p1_tbl[p1_idx] = phys_addr | 0b11;
    return EFI_SUCCESS;
}

EFI_STATUS ConfigurePaging(mem_info_t *mem_info) {
    EFI_STATUS status;
    UINT64 *p4_tbl = NULL;
    uefi_call_wrapper(gBS->AllocatePages, 4, AllocateAnyPages, EfiLoaderData, 1, &p4_tbl);
    uefi_call_wrapper(gBS->SetMem, 3, p4_tbl, EFI_PAGE_SIZE, 0);
    mem_info->pml4 = p4_tbl;
    Print(L"PML4 Address: 0x%x\n", (EFI_PHYSICAL_ADDRESS) p4_tbl);

    for (UINTN i = 0; i < (0xFFFFFFFF / 4096); i++) { // 64 mb
        EFI_PHYSICAL_ADDRESS addr = i * EFI_PAGE_SIZE;
        status = MapAddr(addr, addr, p4_tbl);
        if (EFI_ERROR(status))
            return status;
        
        status = MapAddr(VADDR(addr), addr, p4_tbl);
        if (EFI_ERROR(status))
            return status;
    }

    Print(L"Mapped first 4GB\n");

    status = MapAddr((uintptr_t) mem_info->pml4, (uintptr_t) mem_info->pml4, mem_info->pml4);
    if (EFI_ERROR(status)) {
        Print(L"Error identity mapping PML4\n");
        return status;
    }

    status = MapAddr(VADDR((uintptr_t) mem_info->pml4), (uintptr_t) mem_info->pml4, mem_info->pml4);
    if (EFI_ERROR(status)) {
        Print(L"Error mapping PML4 in higher half\n");
        return status;
    }
    Print(L"Mapped PML4\n");

    return EFI_SUCCESS;
}

void LoadCr3(mem_info_t *mem_info) {
    asm volatile (
        "mov %0, %%rax\n\t"
        "mov %%rax, %%cr3"
        : : "r" ((EFI_PHYSICAL_ADDRESS) mem_info->pml4) : "rax"
    );
}

EFI_STATUS GetMMAP(mem_info_t *mem_info, UINTN *mmap_key) {
    EFI_MEMORY_DESCRIPTOR *mmap = NULL;
    UINTN mmap_size = 0;
    UINTN descriptor_size = 0;
    UINT32 descriptor_version = 0;
    EFI_STATUS status;
    status = uefi_call_wrapper(gBS->GetMemoryMap, 5, &mmap_size, mmap, mmap_key, &descriptor_size, &descriptor_version);
    if (EFI_ERROR(status) && status != EFI_BUFFER_TOO_SMALL) {
        Print(L"Error getting memory map\n");
        return status;
    }

    mmap_size += 2 * descriptor_size;
    status = uefi_call_wrapper(gBS->AllocatePool, 3, EfiLoaderData, mmap_size, &mmap);
    if (EFI_ERROR(status)) {
        Print(L"Error allocating memory for MMAP\n");
        return status;
    }

    status = uefi_call_wrapper(gBS->GetMemoryMap, 5, &mmap_size, mmap, mmap_key, &descriptor_size, &descriptor_version);
    if (EFI_ERROR(status)) {
        Print(L"Error getting MMAP\n");
        return status;
    }

    mem_info->mmap = (mmap_descriptor_t*) (mmap);
    mem_info->mmap_size = mmap_size;
    mem_info->mmap_descriptor_size = descriptor_size;

    return EFI_SUCCESS;
}

EFI_STATUS ParseMMAP(mem_info_t *mem_info) {
    EFI_STATUS status;
    UINTN num_entries = mem_info->mmap_size / mem_info->mmap_descriptor_size;
    mmap_descriptor_t *cur_desc = mem_info->mmap;
    UINTN mem_pages = 0;
    for (UINTN i = 0; i < num_entries; i++) {
        mem_pages += cur_desc->count;
        cur_desc = (mmap_descriptor_t*) ((UINT8*) cur_desc + mem_info->mmap_descriptor_size);
    }

    cur_desc = mem_info->mmap;
    EFI_VIRTUAL_ADDRESS bitmap_location = 0;
    size_t bitmap_size = mem_pages / 8 + 1;
    for (UINTN i = 0; i < num_entries; i++) {
        if (cur_desc->type == 7 && cur_desc->count >= SIZE_TO_PAGES(bitmap_size) && (bitmap_location == 0 || cur_desc->physical_start < bitmap_location)) {
            bitmap_location = cur_desc->physical_start;
        }
        cur_desc = (mmap_descriptor_t*) ((UINT8*) cur_desc + mem_info->mmap_descriptor_size);
    }

    for (UINTN i = 0; i < SIZE_TO_PAGES(bitmap_size); i++) {
        status = MapAddr(VADDR(bitmap_location) + i * PAGE_SIZE, bitmap_location + i * PAGE_SIZE, mem_info->pml4);
        if (EFI_ERROR(status))
            return status;
    }
    
    mem_info->bitmap = (uint64_t*) bitmap_location;
    mem_info->mem_pages = mem_pages;
    return EFI_SUCCESS;
}

EFI_STATUS FreeMMAP(mem_info_t *mem_info) {
    EFI_STATUS status = uefi_call_wrapper(gBS->FreePool, 1, mem_info->mmap);
    if (EFI_ERROR(status)) {
        Print(L"Error freeing memory map\n");
        return status;
    }

    return EFI_SUCCESS;
}
