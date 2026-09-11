use std::path::PathBuf;

/// `$HOME`, falling back to `/` (matches the rest of the CLI).
pub fn home() -> PathBuf {
    std::env::var("HOME").unwrap_or_else(|_| "/".into()).into()
}
