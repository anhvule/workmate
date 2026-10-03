//! Supervision of the bundled `OpenCode` engine.
//!
//! Every rule in here is a scar taken from cowork-z rather than re-derived
//! (see `.wayfinder/research/cowork-z-architecture.md`):
//!
//! - Readiness is the engine's own announcement, **not** process spawn. A
//!   spawned process that has not bound its port yet will refuse every request.
//! - Shutdown happens on `ExitRequested`, never `Exit`. `Exit` arrives after the
//!   runtime has begun tearing down and the engine is left orphaned.
//! - The engine is unsecured unless `OPENCODE_SERVER_PASSWORD` is set — it says
//!   so on startup — so workmate always sets a fresh one per launch.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("engine binary not found at {0}")]
    BinaryMissing(PathBuf),
    #[error("engine failed to start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("engine did not announce a listening address within {0:?}")]
    ReadinessTimeout(Duration),
    #[error("engine exited during startup")]
    ExitedDuringStartup,
    #[error("engine state is locked by a panicked thread")]
    Poisoned,
}

/// How long to wait for the engine to announce its address before giving up.
const READINESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Where the engine is listening, and the secret needed to talk to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineAddress {
    pub base_url: String,
    pub password: String,
}

/// Extract the listening address from a line of engine output.
///
/// Readiness detection lives in its own function precisely so it can be tested
/// without spawning anything: getting this wrong means either a race at startup
/// or a hang with no diagnosis.
#[must_use]
pub fn parse_listening_url(line: &str) -> Option<String> {
    let idx = line.find("http://")?;
    let rest = &line[idx..];
    let end = rest
        .find(|c: char| c.is_whitespace())
        .unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches(['.', ',']);
    // A bare scheme with no host:port is not an address.
    if url.len() > "http://".len() && url.contains(':') {
        Some(url.to_owned())
    } else {
        None
    }
}

/// A fresh per-launch password, hex-encoded.
///
/// Per launch rather than persisted: a leaked password dies with the process,
/// and nothing needs it across restarts.
///
/// # Panics
/// Panics if the operating system cannot supply randomness. Continuing without
/// it would mean serving the engine unsecured, which is worse than failing.
#[must_use]
pub fn generate_password() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("system randomness is unavailable");
    bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// Where the engine keeps its auth file inside its private home.
fn auth_file(home: &Path) -> PathBuf {
    home.join("data").join("opencode").join("auth.json")
}

/// Remove the key file the engine writes when it is given a credential.
///
/// Keys live in the OS keychain; the engine is handed one for the length of its
/// run and persists it to this file. It is deleted before launch (a crash may
/// have left one) and after stop, so a key is never at rest in workmate's data.
fn wipe_auth(home: &Path) {
    let _ = std::fs::remove_file(auth_file(home));
}

#[derive(Debug)]
pub struct Engine {
    child: Child,
    address: EngineAddress,
    home: PathBuf,
}

impl Engine {
    /// Spawn the engine and wait until it announces an address.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the binary is missing, cannot be spawned,
    /// exits during startup, or never announces an address.
    pub fn start(binary: &Path, home: &Path) -> Result<Self, EngineError> {
        if !binary.exists() {
            return Err(EngineError::BinaryMissing(binary.to_path_buf()));
        }
        let password = generate_password();
        for d in ["data", "config", "cache", "state"] {
            std::fs::create_dir_all(home.join(d))?;
        }
        wipe_auth(home);

        // Private XDG directories: left alone, the engine would read and write
        // the user's own OpenCode data, config and credentials. `--port 0` makes
        // the engine take its preferred port if free and any free one if not, so
        // it never collides with an OpenCode the user already has running; the
        // announced address is what readiness trusts either way.
        let mut child = Command::new(binary)
            .args(["serve", "--hostname", "127.0.0.1", "--port", "0"])
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("OPENCODE_SERVER_PASSWORD", &password)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().ok_or(EngineError::ExitedDuringStartup)?;
        let deadline = Instant::now() + READINESS_TIMEOUT;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        let base_url = loop {
            if Instant::now() > deadline {
                let _ = child.kill();
                return Err(EngineError::ReadinessTimeout(READINESS_TIMEOUT));
            }
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = child.kill();
                    return Err(EngineError::ExitedDuringStartup);
                }
                Ok(_) => {
                    if let Some(url) = parse_listening_url(&line) {
                        break url;
                    }
                }
                Err(e) => {
                    let _ = child.kill();
                    return Err(EngineError::Spawn(e));
                }
            }
        };

        Ok(Self {
            child,
            address: EngineAddress { base_url, password },
            home: home.to_path_buf(),
        })
    }

    #[must_use]
    pub fn address(&self) -> &EngineAddress {
        &self.address
    }

    /// Stop the engine.
    ///
    /// # Errors
    /// Returns [`EngineError::Spawn`] if the process cannot be signalled.
    pub fn stop(&mut self) -> Result<(), EngineError> {
        self.child.kill()?;
        let _ = self.child.wait();
        wipe_auth(&self.home);
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// The engine handle managed by Tauri, plus the sessions that keep it pinned.
#[derive(Default)]
pub struct EngineState {
    inner: Mutex<Option<Engine>>,
    active_sessions: Mutex<usize>,
}

impl EngineState {
    /// Whether a restart may proceed.
    ///
    /// Restarts are deferred while sessions are live: cowork-z restarts only on
    /// a credential change and waits for sessions to drain, because tearing the
    /// engine out from under an in-flight run loses the user's work.
    ///
    /// # Errors
    /// Returns [`EngineError::Poisoned`] if a holder of the lock panicked.
    pub fn may_restart(&self) -> Result<bool, EngineError> {
        let n = self.active_sessions.lock().map_err(|_| EngineError::Poisoned)?;
        Ok(*n == 0)
    }

    /// # Errors
    /// Returns [`EngineError::Poisoned`] if a holder of the lock panicked.
    pub fn session_started(&self) -> Result<(), EngineError> {
        let mut n = self.active_sessions.lock().map_err(|_| EngineError::Poisoned)?;
        *n += 1;
        Ok(())
    }

    /// # Errors
    /// Returns [`EngineError::Poisoned`] if a holder of the lock panicked.
    pub fn session_ended(&self) -> Result<(), EngineError> {
        let mut n = self.active_sessions.lock().map_err(|_| EngineError::Poisoned)?;
        *n = n.saturating_sub(1);
        Ok(())
    }

    /// Replace the running engine, if any.
    ///
    /// # Errors
    /// Returns [`EngineError::Poisoned`] if a holder of the lock panicked.
    pub fn set(&self, engine: Engine) -> Result<(), EngineError> {
        let mut slot = self.inner.lock().map_err(|_| EngineError::Poisoned)?;
        *slot = Some(engine);
        Ok(())
    }

    /// The current address, if the engine is running.
    ///
    /// # Errors
    /// Returns [`EngineError::Poisoned`] if a holder of the lock panicked.
    pub fn address(&self) -> Result<Option<EngineAddress>, EngineError> {
        let slot = self.inner.lock().map_err(|_| EngineError::Poisoned)?;
        Ok(slot.as_ref().map(|e| e.address().clone()))
    }

    /// Shut the engine down. Called from `ExitRequested`, never `Exit`.
    ///
    /// # Errors
    /// Returns [`EngineError::Poisoned`] if a holder of the lock panicked.
    pub fn shutdown(&self) -> Result<(), EngineError> {
        let mut slot = self.inner.lock().map_err(|_| EngineError::Poisoned)?;
        if let Some(mut engine) = slot.take() {
            engine.stop()?;
        }
        Ok(())
    }
}

/// Tests that start the real engine take this: two engines at once contend
/// for the same on-disk state and one exits during startup.
#[cfg(test)]
pub(crate) static REAL_ENGINE: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_is_the_announced_url() {
        assert_eq!(
            parse_listening_url("opencode server listening on http://127.0.0.1:4096"),
            Some("http://127.0.0.1:4096".to_owned())
        );
    }

    #[test]
    fn a_trailing_full_stop_is_not_part_of_the_url() {
        assert_eq!(
            parse_listening_url("listening on http://127.0.0.1:4096."),
            Some("http://127.0.0.1:4096".to_owned())
        );
    }

    #[test]
    fn unrelated_output_is_not_mistaken_for_readiness() {
        assert_eq!(parse_listening_url("Warning: OPENCODE_SERVER_PASSWORD is not set"), None);
        assert_eq!(parse_listening_url("see http:// for details"), None);
        assert_eq!(parse_listening_url(""), None);
    }

    #[test]
    fn passwords_are_fresh_per_launch_and_long_enough_to_matter() {
        let a = generate_password();
        let b = generate_password();
        assert_eq!(a.len(), 64, "32 bytes, hex encoded");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "a per-launch password must not repeat");
    }

    #[test]
    fn a_missing_binary_is_reported_as_such_rather_than_as_a_spawn_failure() {
        let err = Engine::start(Path::new("/nonexistent/opencode"), Path::new("/h")).expect_err("must fail");
        assert!(matches!(err, EngineError::BinaryMissing(_)));
    }

    #[test]
    fn a_restart_is_deferred_while_a_session_is_live() {
        let state = EngineState::default();
        assert!(state.may_restart().expect("idle"));
        state.session_started().expect("start");
        assert!(!state.may_restart().expect("busy"), "must not restart mid-run");
        state.session_ended().expect("end");
        assert!(state.may_restart().expect("idle again"));
    }

    #[test]
    fn ending_more_sessions_than_started_does_not_underflow() {
        let state = EngineState::default();
        state.session_ended().expect("end");
        assert!(state.may_restart().expect("idle"));
    }

    /// Spawns the real bundled engine. Skipped when it has not been fetched.
    #[test]
    fn the_bundled_engine_starts_and_announces_an_address() {
        let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/opencode");
        if !binary.exists() {
            eprintln!("skipping: run `pnpm sidecar:fetch` to exercise this test");
            return;
        }
        let _serial = REAL_ENGINE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let home = std::env::temp_dir().join(crate::ids::new_id("engine-home"));
        std::fs::create_dir_all(auth_file(&home).parent().unwrap()).unwrap();
        std::fs::write(auth_file(&home), "{\"left\":\"behind\"}").unwrap();
        let mut engine = Engine::start(&binary, &home).expect("engine should start");
        assert!(!auth_file(&home).exists(), "a key left by a crash is wiped before launch");
        let port: u16 = engine.address().base_url.rsplit(':').next().unwrap().parse().unwrap();
        assert!(port > 0, "the announced address is a real port");
        std::fs::write(auth_file(&home), "{\"anthropic\":{\"key\":\"k\"}}").unwrap();
        engine.stop().unwrap();
        assert!(!auth_file(&home).exists(), "a key is not left at rest after the engine stops");
        assert!(home.join("data").is_dir(), "the engine lives in its own home");
        let _ = std::fs::remove_dir_all(&home);
        let engine = Engine::start(&binary, &std::env::temp_dir().join(crate::ids::new_id("engine-home"))).expect("engine should start");
        let addr = engine.address();
        assert!(addr.base_url.starts_with("http://127.0.0.1:"));
        assert_eq!(addr.password.len(), 64);
    }
}
