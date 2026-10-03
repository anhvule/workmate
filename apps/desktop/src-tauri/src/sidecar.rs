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
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

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

/// Serves a named persistence operation: `(op, args)` to rows or a message.
pub type Handler = Arc<dyn Fn(&str, &Value) -> Result<Vec<Value>, String> + Send + Sync>;

/// The reply to one `db.call`, by request id.
///
/// Both outcomes are replies: a failing operation must answer, or the sidecar
/// would wait on it until its own timeout.
#[must_use]
pub fn reply(id: &str, outcome: Result<Vec<Value>, String>) -> String {
    let msg = match outcome {
        Ok(rows) => json!({"type": "db.result", "id": id, "rows": rows}),
        Err(message) => json!({"type": "db.error", "id": id, "message": message}),
    };
    format!("{msg}\n")
}

/// Serve one line of post-handshake sidecar output. Returns the reply to write,
/// if the line was a `db.call`.
///
/// Anything else is logged and dropped: `fault` is the sidecar's own account of
/// itself, and `event` forwarding belongs to the event stream (ticket 023).
#[must_use]
pub fn serve(line: &str, handler: &Handler) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    match value.get("type").and_then(Value::as_str)? {
        "db.call" => {
            let id = value.get("id").and_then(Value::as_str)?;
            let op = value.get("op").and_then(Value::as_str).unwrap_or_default();
            let args = value.get("args").cloned().unwrap_or(Value::Null);
            Some(reply(id, handler(op, &args)))
        }
        "fault" => {
            eprintln!("sidecar fault: {}", value.get("message").and_then(Value::as_str).unwrap_or("?"));
            None
        }
        _ => None,
    }
}

#[derive(Debug)]
pub struct Sidecar {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pub pid: i64,
}

impl Sidecar {
    /// Spawn the sidecar, hand it the engine address, and wait for `ready`.
    ///
    /// # Errors
    /// Returns [`SidecarError`] if the binary is missing, cannot be spawned,
    /// faults, exits during startup, or never reports ready.
    pub fn start(
        binary: &Path,
        address: &EngineAddress,
        handler: Handler,
    ) -> Result<Self, SidecarError> {
        if !binary.exists() {
            return Err(SidecarError::BinaryMissing(binary.to_path_buf()));
        }
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let mut stdin = child.stdin.take().ok_or(SidecarError::ExitedDuringStartup)?;
        stdin.write_all(hello(address)?.as_bytes())?;
        stdin.flush()?;
        let stdin = Arc::new(Mutex::new(stdin));

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
                    Handshake::Ready { pid } => {
                        Self::pump(reader, &stdin, handler);
                        return Ok(Self { child, stdin, pid });
                    }
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

    /// Serve the sidecar's persistence calls until its stdout closes.
    ///
    /// One thread, so calls are handled in the order they were sent: writes
    /// within a run cannot reorder. A slow operation delays later ones, which is
    /// the price of that guarantee.
    fn pump(
        mut reader: BufReader<std::process::ChildStdout>,
        stdin: &Arc<Mutex<ChildStdin>>,
        handler: Handler,
    ) {
        let stdin = Arc::clone(stdin);
        std::thread::spawn(move || {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
                let Some(out) = serve(&line, &handler) else { continue };
                let Ok(mut w) = stdin.lock() else { return };
                if w.write_all(out.as_bytes()).and_then(|()| w.flush()).is_err() {
                    return;
                }
            }
        });
    }

    /// Ask the sidecar to stop, then make sure it did.
    ///
    /// Ordering matters at shutdown: the sidecar goes first, so it is never left
    /// talking to a dead engine.
    ///
    /// # Errors
    /// Returns [`SidecarError::Spawn`] if the process cannot be signalled.
    pub fn stop(&mut self) -> Result<(), SidecarError> {
        if let Ok(mut stdin) = self.stdin.lock() {
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

    fn no_ops() -> Handler {
        Arc::new(|_, _| Ok(vec![]))
    }

    #[test]
    fn a_db_call_is_answered_by_id_with_rows_or_an_error() {
        let h: Handler = Arc::new(|op, args| {
            if op == "ok" { Ok(vec![args.clone()]) } else { Err(format!("no {op}")) }
        });
        let ok = serve(r#"{"type":"db.call","id":"7","op":"ok","args":{"a":1}}"#, &h).unwrap();
        let v: Value = serde_json::from_str(ok.trim()).unwrap();
        assert_eq!((v["type"].as_str(), v["id"].as_str()), (Some("db.result"), Some("7")));
        assert_eq!(v["rows"][0]["a"], 1);
        let bad = serve(r#"{"type":"db.call","id":"8","op":"nope"}"#, &h).unwrap();
        assert!(bad.contains("db.error") && bad.contains("no nope"));
    }

    #[test]
    fn other_traffic_and_noise_get_no_reply() {
        let h = no_ops();
        assert!(serve(r#"{"type":"event","name":"x"}"#, &h).is_none());
        assert!(serve(r#"{"type":"fault","message":"m"}"#, &h).is_none());
        assert!(serve("not json", &h).is_none());
    }

    /// A stand-in sidecar over real pipes: it announces ready, makes a call,
    /// and records the reply it reads back.
    #[cfg(unix)]
    #[test]
    fn a_call_makes_the_round_trip_over_real_pipes() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(crate::ids::new_id("pipe"));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("reply.txt");
        let script = dir.join("fake-sidecar");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nread hello\necho '{{\"type\":\"ready\",\"pid\":1}}'\n\
                 echo '{{\"type\":\"db.call\",\"id\":\"a\",\"op\":\"echo\",\"args\":{{\"n\":5}}}}'\n\
                 read reply\necho \"$reply\" > {}\n",
                out.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let h: Handler = Arc::new(|_, args| Ok(vec![args.clone()]));
        let _sidecar = Sidecar::start(&script, &address(), h).unwrap();
        for _ in 0..100 {
            if out.exists() && std::fs::metadata(&out).unwrap().len() > 0 { break; }
            std::thread::sleep(Duration::from_millis(50));
        }
        let got = std::fs::read_to_string(&out).unwrap();
        assert!(got.contains(r#""id":"a""#) && got.contains(r#""n":5"#), "got {got}");
        std::fs::remove_dir_all(&dir).ok();
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
        let err = Sidecar::start(Path::new("/nonexistent/workmate-sidecar"), &address(), no_ops())
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
        let sidecar = Sidecar::start(&binary, &address(), no_ops()).expect("sidecar should start");
        assert!(sidecar.pid > 0, "ready must carry a real pid");
    }
}
