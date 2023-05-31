#pragma once
#include <stddef.h>

size_t strlen(const char *s);
void itoa(int num, char* str, int len, int base);
char *ultoa(unsigned long num, char *str, int radix);
void strrev(char* str);
int strcmp (const char *p1, const char *p2);