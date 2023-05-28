#pragma once

enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error
};

void log(enum LogLevel level, const char* target, const char* fmt, ...);
void log_no_nl(enum LogLevel level, const char* target, const char* fmt, ...);
