//! The local log file (spec section 13): `pinaxcv.log` in the app's log
//! directory, which the user can open and attach to an issue. Nothing is
//! uploaded. Log lines name commands, timings and error codes only, never
//! CV content, job descriptions, prompts, file paths or secrets.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The current log file's name.
pub const LOG_FILE: &str = "pinaxcv.log";
/// The previous log, kept after rotation.
pub const OLD_LOG_FILE: &str = "pinaxcv.1.log";
/// A log larger than this is rotated at startup.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// Panic messages are cut to this many characters.
const MAX_PANIC_CHARS: usize = 300;

/// Moves an oversized log aside (replacing the older one), then opens the
/// log for appending. Returns the log's path.
pub fn open(dir: &Path) -> io::Result<(PathBuf, File)> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_FILE);
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        fs::rename(&path, dir.join(OLD_LOG_FILE))?;
    }
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    Ok((path, file))
}

/// Sends `tracing` events to the log file and logs panics there too. Call
/// once at startup. A log that cannot be opened is not fatal: the app runs
/// without one.
pub fn init(dir: &Path) {
    let Ok((_, file)) = open(dir) else {
        return;
    };
    let level = if cfg!(debug_assertions) {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_target(false)
        .with_max_level(level)
        .finish();
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return;
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        tracing::error!(location, message = panic_message(info.payload()), "panic");
        default_hook(info);
    }));
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "PinaxCV started");
}

/// The panic's message, shortened. Panic messages are written by
/// developers, but are cut short in case one formats a value.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(no message)");
    match text.char_indices().nth(MAX_PANIC_CHARS) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn opens_appends_and_rotates_large_logs() {
        let dir = tempfile::tempdir().expect("dir");
        let (path, mut file) = open(dir.path()).expect("open");
        assert_eq!(path, dir.path().join(LOG_FILE));
        writeln!(file, "first").expect("write");
        drop(file);
        let (_, mut file) = open(dir.path()).expect("reopen");
        writeln!(file, "second").expect("write");
        drop(file);
        assert_eq!(fs::read_to_string(&path).expect("read"), "first\nsecond\n");

        fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).expect("fill");
        let (_, file) = open(dir.path()).expect("rotate");
        assert_eq!(file.metadata().expect("meta").len(), 0);
        let old = fs::metadata(dir.path().join(OLD_LOG_FILE)).expect("old log");
        assert_eq!(old.len(), MAX_LOG_BYTES + 1);
    }

    #[test]
    fn panic_messages_are_read_and_shortened() {
        assert_eq!(panic_message(&"boom"), "boom");
        assert_eq!(panic_message(&String::from("bang")), "bang");
        assert_eq!(panic_message(&42), "(no message)");
        let long = "é".repeat(400);
        let short = panic_message(&long);
        assert_eq!(short.chars().count(), MAX_PANIC_CHARS + 1);
        assert!(short.ends_with('…'));
    }
}
