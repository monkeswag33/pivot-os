#pragma once
#include <stdint.h>

#pragma pack(1)
struct page_table_t { uint64_t entries[512]; };
#pragma pack()

void map_page(uint64_t logical);
void setup_paging(void);
void *alloc_page(void);
