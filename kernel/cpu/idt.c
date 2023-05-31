#include <stdint.h>
#include "idt.h"
#include "isr.h"
#include "log.h"
#include "screen.h"
#include "acpi.h"
#define NUM_INTERRUPTS 256
#define IRQ(n) (32 + n)

struct __attribute__((packed)) idt_entry_t {
    uint16_t isr_low;
    uint16_t kernel_cs;
    uint8_t ist;
    uint8_t attributes;
    uint16_t isr_mid;
    uint32_t isr_high;
    uint32_t rsv;
};

struct __attribute__((packed)) idtr_t {
    uint16_t limit;
    uint64_t base;
};

extern void* isr_stub_table[];
static struct idt_entry_t idt[NUM_INTERRUPTS];
static struct idtr_t idtr;

static void set_entry(struct idt_entry_t *entry, void* isr, uint8_t flags) {
    uint64_t isr_addr = (uint64_t)(uintptr_t) isr;
    entry->isr_low = isr_addr & 0xFFFF;
    entry->kernel_cs = 0x8;
    entry->ist = 0;
    entry->attributes = flags;
    entry->isr_mid = (isr_addr >> 16) & 0xFFFF;
    entry->isr_high = (isr_addr >> 32) & 0xFFFFFFFF;
    entry->rsv = 0;
}

void timer_handler() {
    printf(".");
    apic_eoi();
}

void spurious_interrupt() {
    log(Info, "ISR", "Spurious Interrupt\n");
}

void apic_error_handler() {
    printf("Apic Error handler triggered\n");
}

void load_idt(void) {
    idtr.base = (uintptr_t) &idt[0];
    idtr.limit = sizeof(struct idt_entry_t) * NUM_INTERRUPTS - 1;
    for (int idx = 0; idx < NUM_INTERRUPTS; idx++)
        idt[idx] = (struct idt_entry_t) { 0 };
    for (uint8_t idx = 0; idx < 32; idx++)
        set_entry(&idt[idx], isr_stub_table[idx], 0x8E);
    set_entry(&idt[IRQ(0)], (void*)(uintptr_t) timer_handler, 0x8E);
    set_entry(&idt[IRQ(1)], (void*)(uintptr_t) spurious_interrupt, 0x8E);
    set_entry(&idt[IRQ(2)], (void*)(uintptr_t) apic_error_handler, 0x8E);
    asm volatile ("lidt %0" : : "m" (idtr));
    asm volatile ("sti");

    log(Info, "IDT", "Initialized IDT");
}
