#pragma once
#include <stdint.h>
#include <stddef.h>

struct mem_region_t {
    uint64_t physical_start;
    uint64_t num_pages;
    uint32_t mem_type;
};

struct configuration_table_t {
    void *rsdp_ptr;
    // Add more in the future
};

typedef struct {
    uint8_t    *framebuffer;
    uint32_t    width;
    uint32_t    height;
    uint32_t    pitch;

    size_t num_regions;
    struct mem_region_t *mem_regions;

    struct configuration_table_t config_table;
} bootparam_t;

extern bootparam_t* bootp;
