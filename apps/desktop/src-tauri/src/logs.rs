//! Local diagnostics: rotating log files, and nothing sent anywhere.
//!
//! workmate ships without telemetry (ticket 030). What it does keep is a record
//! a user can open when something goes wrong: its own warnings, and everything
//! the engine and sidecar write. Those two *must* be read continuously anyway —
//! a child process whose pipe is never drained blocks on its next write once the
//! pipe buffer fills, which is how the engine would have hung mid-session.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Rotate a log past this size, keeping one previous file.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

static DIR: OnceLock<PathBuf> = OnceLock::new();
static WRITE: Mutex<()> = Mutex::new(());

/// Send logs to `dir`. Until this is called (as in tests) they go to stderr.
pub fn init(dir: &Path) {
    if fs::create_dir_all(dir).is_ok() {
        let _ = DIR.set(dir.to_path_buf());
    }
}

/// Where logs are written, if anywhere.
#[must_use]
pub fn dir() -> Option<&'static Path> {
    DIR.get().map(PathBuf::as_path)
}

fn open(path: &Path) -> std::io::Result<File> {
    if fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(path, path.with_extension("log.1"));
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Append one line to `<stream>.log`, stamped.
pub fn line(stream: &str, text: &str) {
    let Some(dir) = DIR.get() else {
        eprintln!("{stream}: {text}");
        return;
    };
    let _guard = WRITE.lock();
    if let Ok(mut f) = open(&dir.join(format!("{stream}.log"))) {
        let _ = writeln!(f, "{} {}", crate::db::now_ms(), text.trim_end());
    }
}

/// workmate's own warnings.
pub fn warn(text: &str) {
    line("workmate", text);
}

/// Read a child's output until it closes, line by line, into `<stream>.log`.
///
/// On its own thread, so the child never waits on us.
pub fn drain<R: Read + Send + 'static>(stream: &'static str, from: R) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(from);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => return,
                Ok(_) => line(stream, &String::from_utf8_lossy(&buf)),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// The bug class this module exists for: a child that writes more than a
    /// pipe buffer holds must still finish when its output is drained.
    #[cfg(unix)]
    #[test]
    fn a_chatty_child_finishes_when_its_pipes_are_drained() {
        let mut child = Command::new("sh")
            .args(["-c", "i=0; while [ $i -lt 4000 ]; do echo 'a fairly long line of engine logging output, repeated' >&2; i=$((i+1)); done; echo done"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drain("test-stderr", child.stderr.take().unwrap());
        drain("test-stdout", child.stdout.take().unwrap());
        let start = Instant::now();
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "the child blocked on a full pipe");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_log_past_its_limit_rotates_and_keeps_one_previous_file() {
        let d = std::env::temp_dir().join(crate::ids::new_id("logs"));
        fs::create_dir_all(&d).unwrap();
        let path = d.join("x.log");
        fs::write(&path, vec![b'a'; usize::try_from(MAX_BYTES).unwrap() + 1]).unwrap();
        drop(open(&path).unwrap());
        assert!(d.join("x.log.1").exists(), "the full log moved aside");
        assert_eq!(fs::metadata(&path).unwrap().len(), 0, "a fresh log started");
        let _ = fs::remove_dir_all(&d);
    }
}
