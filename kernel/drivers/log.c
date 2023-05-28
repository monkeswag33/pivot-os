#include "log.h"
#include "screen.h"

void print_level_target(enum LogLevel level, const char *target) {
    char* str;
    switch (level) {
        case Error:
            str = "ERROR";
            break;
        case Warn:
            str = "WARN";
            break;
        case Info:
            str = "INFO";
            break;
        case Debug:
            str = "DEBUG";
            break;
        case Trace:
            str = "TRACE";
    }
    printf("(%s) %s: ", target, str);
}

void vlog(enum LogLevel level, const char *target, const char *fmt, va_list args) {
    print_level_target(level, target);
    vprintf(fmt, args);
}

void log_no_nl(enum LogLevel level, const char *target, const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    vlog(level, target, fmt, args);
    va_end(args);
}

void log(enum LogLevel level, const char* target, const char* fmt, ...) {
    va_list args;
    va_start(args, fmt);
    vlog(level, target, fmt, args);
    va_end(args);
    printf("\n");
}