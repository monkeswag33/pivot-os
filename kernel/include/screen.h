#pragma once
#include <stdarg.h>
#include "bootparam.h"

void init_screen(void);
void vprintf(const char *fmt, va_list args);
void printf(const char *fmt, ...);