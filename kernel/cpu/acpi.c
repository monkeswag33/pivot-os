#include <stdint.h>
#include "bootparam.h"
#include "log.h"
#include "mem.h"
#include "screen.h"
#include "acpi.h"

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

static inline void outb(uint16_t port, uint8_t val)
{
    asm volatile ( "outb %0, %1" : : "a"(val), "Nd"(port) :"memory");
    /* There's an outb %al, $imm8  encoding, for compile-time constant port numbers that fit in 8b.  (N constraint).
     * Wider immediate constants would be truncated at assemble-time (e.g. "i" constraint).
     * The  outb  %al, %dx  encoding is the only option for all other cases.
     * %1 expands to %dx because  port  is a uint16_t.  %w1 could be used if we had the port number a wider C type */
}

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
        log(Error, "ACPI", "Found invalid RSDP");
        hcf();
    } else
        log(Info, "ACPI", "Found valid RSDP");

    return rsdp;
}

int validate_xsdt(struct xsdt_header_t *xsdt) {
    if (memcmp(xsdt->signature, "XSDT", 4))
        return 1;
    return validate_checksum((char*) xsdt, xsdt->length);
}

static inline void disable_pic(void) {
    outb(0xa1, 0xff);
    outb(0x21, 0xff);
}

void handle_madt(struct madt_header_t *madt) {
    // Validate MADT
    // Already checked if signature is "APIC"
    if (validate_checksum((char*) madt, madt->length)) {
        log(Error, "ACPI", "Found invalid MADT");
        hcf();
    } else
        log(Info, "ACPI", "Found valid MADT");
    log_no_nl(Info, "ACPI", "Dual 8259 PIC setup... ");
    if (madt->flags) {
        printf("[yes]\n");
        disable_pic();
        log(Info, "ACPI", "Disabled PIC");
    } else
        printf("[no]\n");
    printf("Local interrupt controller address: %d\n", (long long) madt->local_interrupt_controller_address);
}

void init_acpi(void) {
    struct rsdp_t *rsdp = get_rsdp(bootp->config_table.rsdp_ptr);
    char *xsdt = (char*) rsdp->xsdt_address;
    struct xsdt_header_t *xsdt_header = (struct xsdt_header_t*) xsdt;
    if (validate_xsdt(xsdt_header)) {
        log(Error, "ACPI", "Found invalid XSDT");
        hcf();
    } else
        log(Info, "ACPI", "Found valid XSDT");
    uint64_t *xsdt_entries = (uint64_t*) (xsdt + sizeof(struct xsdt_header_t));
    uint32_t num_entries = (xsdt_header->length - sizeof(struct xsdt_header_t)) / 8;
    for (uint32_t i = 0; i < num_entries; i++) {
        char *table = (char*) xsdt_entries[i];
        if (!memcmp(table, "APIC", 4))
            handle_madt((struct madt_header_t*) table);
    }
}
