#include "bootparam.h"
#include "screen.h"
#include "gdt.h"
#include "idt.h"
#include "paging.h"
#include "mem.h"
#include "log.h"
#include "string.h"

bootparam_t *bootp;

#pragma pack(1)

struct rsdp_t {
    char signature[8];
    uint8_t checksum;
    char oemid[6];
    uint8_t revision;
    uint32_t rsdt_address;

    // ACPI 2.0 Fields
    uint32_t length;
    uint64_t xsdt_address;
    uint8_t ext_checksum;
    char rsv[3];
};

struct xsdt_header_t {
    char signature[4];
    uint32_t length;
    uint8_t revision;
    uint8_t checksum;
    char oemid[6];
    char oem_table_id[8];
    uint32_t oem_revision;
    uint32_t creator_id;
    uint32_t creator_revision;
};

struct madt_header_t {
    char signature[4];
    uint32_t length;
    uint8_t revision;
    uint8_t checksum;
    char oemid[6];
    char oem_table_id[8];
    uint32_t oem_revision;
    uint32_t creator_id;
    uint32_t creator_revision;
    uint32_t local_interrupt_controller_address;
    uint32_t flags;
};

#pragma pack()

int validate_checksum(char *table, uint32_t length) {
    uint8_t sum = 0;
    for (uint32_t i = 0; i < length; i++)
        sum += table[i];
    return sum;
}

__attribute__((noreturn))
void hcf(void) {
    asm volatile ("cli");
    while (1)
        asm volatile ("hlt");
}

struct rsdp_t *get_rsdp(char *rsdp_ptr) {
    struct rsdp_t *rsdp = (struct rsdp_t*) rsdp_ptr;
    if (memcmp(rsdp->signature, "RSD PTR ", 8) ||
        validate_checksum(rsdp_ptr, rsdp->length)) {
        log(Error, "KERNEL", "Found invalid RSDP");
        hcf();
    } else
        log(Info, "KERNEL", "Found a valid RSDP");

    return rsdp;
}

int validate_xsdt(struct xsdt_header_t *xsdt) {
    if (memcmp(xsdt->signature, "XSDT", 4))
        return 1;
    return validate_checksum((char*) xsdt, xsdt->length);
}

void handle_madt(struct madt_header_t *madt) {
    // Validate MADT
    // Already checked if signature is "APIC"
    if (validate_checksum((char*) madt, madt->length)) {
        log(Error, "KERNEL", "Found invalid MADT");
        hcf();
    } else
        log(Info, "KERNEL", "Found valid MADT");
}

void _start(bootparam_t *bootparam)
{
    bootp = bootparam;
    init_screen();
    load_gdt();
    load_idt();
    setup_paging();

    struct rsdp_t *rsdp = get_rsdp(bootp->config_table.rsdp_ptr);
    char *xsdt = (char*) rsdp->xsdt_address;
    struct xsdt_header_t *xsdt_header = (struct xsdt_header_t*) xsdt;
    if (validate_xsdt(xsdt_header)) {
        log(Error, "KERNEL", "Found invalid XSDT");
        hcf();
    } else
        log(Info, "KERNEL", "Found valid XSDT");
    uint64_t *xsdt_entries = (uint64_t*) (xsdt + sizeof(struct xsdt_header_t));
    uint32_t num_entries = (xsdt_header->length - sizeof(struct xsdt_header_t)) / 8;
    for (uint32_t i = 0; i < num_entries; i++) {
        char *table = (char*) xsdt_entries[i];
        if (!memcmp(table, "APIC", 4))
            handle_madt((struct madt_header_t*) table);
    }
    while (1);
}