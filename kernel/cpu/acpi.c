#include <stdint.h>
#include "bootparam.h"
#include "log.h"
#include "mem.h"
#include "screen.h"
#include "acpi.h"

#define ID      (0x0020/4)   // ID
#define VER     (0x0030/4)   // Version
#define TPR     (0x0080/4)   // Task Priority
#define EOI     (0x00B0/4)   // EOI
#define SVR     (0x00F0/4)   // Spurious Interrupt Vector
#define DFR     (0x00E0/4)   // Destination Format Register
  #define ENABLE     0x00000100   // Unit Enable
#define ESR     (0x0280/4)   // Error Status
#define ICRLO   (0x0300/4)   // Interrupt Command
  #define INIT       0x00000500   // INIT/RESET
  #define STARTUP    0x00000600   // Startup IPI
  #define DELIVS     0x00001000   // Delivery status
  #define ASSERT     0x00004000   // Assert interrupt (vs deassert)
  #define DEASSERT   0x00000000
  #define LEVEL      0x00008000   // Level triggered
  #define BCAST      0x00080000   // Send to all APICs, including self.
  #define AES        0x000C0000   // Send to all APICs, excluding self.
  #define BUSY       0x00001000
  #define FIXED      0x00000000
#define ICRHI   (0x0310/4)   // Interrupt Command [63:32]
#define TIMER   (0x0320/4)   // Local Vector Table 0 (TIMER)
  #define X1         0x0000000B   // divide counts by 1
  #define PERIODIC   0x00020000   // Periodic
#define PCINT   (0x0340/4)   // Performance Counter LVT
#define LINT0   (0x0350/4)   // Local Vector Table 1 (LINT0)
#define LINT1   (0x0360/4)   // Local Vector Table 2 (LINT1)
#define ERROR   (0x0370/4)   // Local Vector Table 3 (ERROR)
  #define MASKED     0x00010000   // Interrupt masked
#define TICR    (0x0380/4)   // Timer Initial Count
#define TCCR    (0x0390/4)   // Timer Current Count
#define TDCR    (0x03E0/4)   // Timer Divide Configuration

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

struct ioapic_t {
    uint8_t type;
    uint8_t length;
    uint8_t ioapic_id;
    uint8_t rsv;
    uint32_t ioapic_address;
    uint32_t global_system_interrupt_base;
};

#pragma pack()

volatile uint32_t *lapic;

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
    log(Debug, "ACPI", "Local interrupt controller address: %x", madt->local_interrupt_controller_address);
    char *entry = (char*) madt + sizeof(*madt);
    char *end = (char*) madt + madt->length;
    uint8_t id;
    while (entry < end) {
        switch (*entry) {
            case 0x0:
                // LAPIC
                id = entry[3];
                log(Debug, "ACPI", "Found LAPIC (id: %u)", id);
                break;

            case 0x1:
                struct ioapic_t *ioapic = (struct ioapic_t*) entry;
                id = ioapic->ioapic_id;
                uint32_t address = ioapic->ioapic_address;
                log(Debug, "ACPI", "Found I/O APIC (id: %u, address: %x)", id, address);
                break;
        }
        entry += *(entry + 1);
    }
    lapic = (volatile uint32_t*)(uintptr_t) madt->local_interrupt_controller_address;
    lapic[SVR] = ENABLE | 33;
    lapic[ERROR] = 34;
    lapic[DFR] = 0xFFFFFFFF;
    lapic[TPR] = 0;
    lapic[LINT0] = MASKED;
    lapic[LINT1] = MASKED;
    lapic[TDCR] = X1;
    lapic[TIMER] = PERIODIC | 32;
    lapic[ESR] = 0;
    lapic[ESR] = 0;
    lapic[TICR] = 100000;
}

void apic_eoi(void) {
    lapic[EOI] = 1;
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
