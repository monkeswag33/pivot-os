#pragma once
#include <stddef.h>

void* malloc(size_t s);
void free(void*);
void* realloc(void*, size_t s);
void* memset (void *dest, register int val, register size_t len);
int memcmp(const void*, const void*, size_t);