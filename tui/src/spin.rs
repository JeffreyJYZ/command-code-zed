use std::io::Write;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

/// Spinner animates one line on /dev/tty. Only enabled when /dev/tty
/// is openable (pipes get no animation).
pub struct Spin {
    stop_flag: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spin {
    pub fn new() -> Option<Spin> {
        // isatty via /dev/tty availability check — cheap, no libc dep
        if std::fs::File::open("/dev/tty").is_err() {
            return None;
        }
        Some(Spin {
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
        })
    }

    pub fn start(&mut self, msg: &str) {
        self.stop(); // never two at once
        let stop = self.stop_flag.clone();
        let msg = msg.to_string();
        self.handle = Some(std::thread::spawn(move || {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut e = std::fs::OpenOptions::new().write(true).open("/dev/tty").ok();
            let mut i = 0;
            while !stop.load(Ordering::Relaxed) {
                if let Some(f) = e.as_mut() {
                    write!(f, "\r{CYAN}{msg} {}{RESET}", frames[i % frames.len()]).ok();
                    f.flush().ok();
                }
                i += 1;
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
            if let Some(f) = e.as_mut() {
                write!(f, "\r\x1b[2K").ok();
                f.flush().ok();
            }
        }));
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Global spinner behind Mutex so callers don't need &mut.
/// Option<Spin> = None when not a TTY.
pub struct Spinner(std::sync::Mutex<Option<Spin>>);

impl Spinner {
    pub fn global() -> &'static Spinner {
        static S: std::sync::OnceLock<Spinner> = std::sync::OnceLock::new();
        S.get_or_init(|| Spinner(std::sync::Mutex::new(Spin::new())))
    }

    pub fn start(&self, msg: &str) {
        if let Ok(mut g) = self.0.lock() {
            if let Some(s) = g.as_mut() {
                s.start(msg);
            }
        }
    }

    pub fn stop(&self) {
        if let Ok(mut g) = self.0.lock() {
            if let Some(s) = g.as_mut() {
                s.stop();
            }
        }
    }
}
