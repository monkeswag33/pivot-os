#pragma once
#include <stdint.h>
#include <stdatomic.h>
#include <kernel/multiboot.h>
#include <stdbool.h>
#include <stdarg.h>
#define TAB_SIZE 4
#define FRAMEBUFFER_START 0xFFFFFFFFA0000000
#define BUF_SIZE 1024
#define acquire_fb() atomic_flag_test_and_set(&FRAMEBUFFER_LOCK)
#define release_fb() atomic_flag_clear(&FRAMEBUFFER_LOCK)

typedef struct {
    uint8_t *address;
    uint8_t bpp;
    uint32_t pitch;
    uint32_t memory_size;
    uint32_t width;
    uint32_t height;
    uintptr_t phys_addr;
} framebuffer_info_t;

typedef struct {
    uint32_t magic;
    uint32_t version;
    uint32_t headersize;
    uint32_t flags;
    uint32_t numglyph;
    uint32_t bytesperglyph;
    uint32_t height;
    uint32_t width;
} psf_font_t;

typedef struct {
    uint32_t x;
    uint32_t y;
    uint32_t fg;
    uint32_t bg;
    uint32_t num_rows;
    uint32_t num_cols;
} screen_info_t;

extern bool FRAMEBUFFER_INITIALIZED;
extern atomic_flag FRAMEBUFFER_LOCK;
extern screen_info_t screen_info;
extern framebuffer_info_t fbinfo;

void clear_screen(void);
void init_framebuffer(mb_framebuffer_data_t*);
void print_char(char);