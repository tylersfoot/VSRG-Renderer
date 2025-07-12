use std::time::{SystemTime, UNIX_EPOCH};
use std::io::{self, Write};

enum LogLevel {
    Info,
    Warning,
    Error,
}

fn current_time_string() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let tm = chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default();
    tm.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn log(level: LogLevel, msg: &str) {
    let (level_str, color_code) = match level {
        LogLevel::Info => ("INFO", "\x1b[32m"),    // Green
        LogLevel::Warning => ("WARN", "\x1b[33m"), // Yellow
        LogLevel::Error => ("ERROR", "\x1b[31m"),  // Red
    };
    let reset = "\x1b[0m";
    let time = current_time_string();
    println!(
        "[{time}] {color_code}{level_str}{reset}: {msg}"
    );
    io::stdout().flush().unwrap();
}

pub fn info(msg: &str) {
    log(LogLevel::Info, msg);
}

pub fn warning(msg: &str) {
    log(LogLevel::Warning, msg);
}

pub fn error(msg: &str) {
    log(LogLevel::Error, msg);
}