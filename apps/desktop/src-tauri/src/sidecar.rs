//! Supervision of the workmate sidecar.
//!
//! The sidecar owns the engine connection and team orchestration; Rust owns the
//! window, the keychain and every write to SQLite (ticket 024). The engine's
//! launch password crosses this pipe and goes no further — it must never reach
//! the webview.
//!
//! Readiness is the sidecar's own `ready` message, never the spawn: the same
//! rule [`crate::engine`] applies to the engine, for the same reason.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::engine::EngineAddress;

#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("sidecar binary not found at {0}")]
    BinaryMissing(PathBuf),
    #[error("sidecar failed to start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("sidecar did not report ready within {0:?}")]
    ReadinessTimeout(Duration),
    #[error("sidecar exited during startup")]
    ExitedDuringStartup,
    #[error("sidecar reported a fault during startup: {0}")]
    Fault(String),
}

const READINESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Classify a line of sidecar stdout during the startup handshake.
///
/// Separated from the I/O so the handshake's decision table is testable without
/// spawning anything — the readiness bug class this guards against is the one
/// that produces either a race or a silent hang.
#[derive(Debug, PartialEq, Eq)]
pub enum Handshake {
    Ready { pid: i64 },
    Fault(String),
    Ignore,
}

#[must_use]
pub fn classify(line: &str) -> Handshake {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        // Not protocol traffic. The sidecar logs to stderr, so anything
        // unparseable on stdout is noise rather than a failure.
        return Handshake::Ignore;
    };
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("ready") => Handshake::Ready {
            pid: value.get("pid").and_then(serde_json::Value::as_i64).unwrap_or(0),
        },
        Some("fault") => Handshake::Fault(
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unspecified")
                .to_owned(),
        ),
        _ => Handshake::Ignore,
    }
}

/// The `hello` that hands the sidecar the engine's address and password.
///
/// # Errors
/// Returns [`SidecarError::Spawn`] if the message cannot be serialised.
pub fn hello(address: &EngineAddress) -> Result<String, SidecarError> {
    let msg = serde_json::json!({
        "type": "hello",
        "engine": { "baseUrl": address.base_url, "password": address.password },
    });
    let mut line = serde_json::to_string(&msg)
        .map_err(|e| SidecarError::Spawn(std::io::Error::other(e)))?;
    line.push('\n');
    Ok(line)
}

#[derive(Debug)]
pub struct Sidecar {
    child: Child,
    pub pid: i64,
}

impl Sidecar {
    /// Spawn the sidecar, hand it the engine address, and wait for `ready`.
    ///
    /// # Errors
    /// Returns [`SidecarError`] if the binary is missing, cannot be spawned,
    /// faults, exits during startup, or never reports ready.
    pub fn start(binary: &Path, address: &EngineAddress) -> Result<Self, SidecarError> {
        if !binary.exists() {
            return Err(SidecarError::BinaryMissing(binary.to_path_buf()));
        }
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        {
            let stdin = child.stdin.as_mut().ok_or(SidecarError::ExitedDuringStartup)?;
            stdin.write_all(hello(address)?.as_bytes())?;
            stdin.flush()?;
        }

        let stdout = child.stdout.take().ok_or(SidecarError::ExitedDuringStartup)?;
        let mut reader = BufReader::new(stdout);
        let deadline = Instant::now() + READINESS_TIMEOUT;
        let mut line = String::new();

        loop {
            if Instant::now() > deadline {
                let _ = child.kill();
                return Err(SidecarError::ReadinessTimeout(READINESS_TIMEOUT));
            }
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = child.kill();
                    return Err(SidecarError::ExitedDuringStartup);
                }
                Ok(_) => match classify(&line) {
                    Handshake::Ready { pid } => return Ok(Self { child, pid }),
                    Handshake::Fault(message) => {
                        let _ = child.kill();
                        return Err(SidecarError::Fault(message));
                    }
                    Handshake::Ignore => {}
                },
                Err(e) => {
                    let _ = child.kill();
                    return Err(SidecarError::Spawn(e));
                }
            }
        }
    }

    /// Ask the sidecar to stop, then make sure it did.
    ///
    /// Ordering matters at shutdown: the sidecar goes first, so it is never left
    /// talking to a dead engine.
    ///
    /// # Errors
    /// Returns [`SidecarError::Spawn`] if the process cannot be signalled.
    pub fn stop(&mut self) -> Result<(), SidecarError> {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n");
            let _ = stdin.flush();
        }
        self.child.kill()?;
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> EngineAddress {
        EngineAddress {
            base_url: "http://127.0.0.1:4096".to_owned(),
            password: "secret".to_owned(),
        }
    }

    #[test]
    fn ready_is_recognised_and_carries_the_pid() {
        assert_eq!(
            classify(r#"{"type":"ready","pid":42}"#),
            Handshake::Ready { pid: 42 }
        );
    }

    #[test]
    fn a_fault_during_startup_is_surfaced_rather_than_waited_out() {
        assert_eq!(
            classify(r#"{"type":"fault","message":"bad hello"}"#),
            Handshake::Fault("bad hello".to_owned())
        );
    }

    #[test]
    fn non_protocol_output_is_ignored_rather_than_treated_as_ready() {
        assert_eq!(classify("sidecar: started"), Handshake::Ignore);
        assert_eq!(classify(r#"{"type":"event","name":"x"}"#), Handshake::Ignore);
        assert_eq!(classify(""), Handshake::Ignore);
    }

    #[test]
    fn hello_carries_the_engine_address_and_password() {
        let line = hello(&address()).expect("hello");
        assert!(line.ends_with('\n'), "must be newline framed");
        let v: serde_json::Value = serde_json::from_str(line.trim()).expect("json");
        assert_eq!(v["type"], "hello");
        assert_eq!(v["engine"]["baseUrl"], "http://127.0.0.1:4096");
        assert_eq!(v["engine"]["password"], "secret");
    }

    #[test]
    fn a_missing_binary_is_reported_as_such() {
        let err = Sidecar::start(Path::new("/nonexistent/workmate-sidecar"), &address())
            .expect_err("must fail");
        assert!(matches!(err, SidecarError::BinaryMissing(_)));
    }

    /// Spawns the real compiled sidecar. Skipped when it has not been built.
    #[test]
    fn the_compiled_sidecar_completes_the_handshake() {
        let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/workmate-sidecar");
        if !binary.exists() {
            eprintln!("skipping: run `pnpm sidecar:build` to exercise this test");
            return;
        }
        let sidecar = Sidecar::start(&binary, &address()).expect("sidecar should start");
        assert!(sidecar.pid > 0, "ready must carry a real pid");
    }
}
