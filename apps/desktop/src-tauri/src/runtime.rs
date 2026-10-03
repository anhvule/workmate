//! The two supervised children, and the order they start and stop in.
//!
//! Start: engine first, then the sidecar — the sidecar is handed the engine's
//! address in its `hello`, so it cannot start before the engine has one.
//! Stop: sidecar first, then the engine, so the sidecar is never left talking to
//! a dead engine (ticket 025).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::engine::{Engine, EngineAddress, EngineError};
use crate::sidecar::{Handler, Sidecar, SidecarError};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error("runtime state is locked by a panicked thread")]
    Poisoned,
}

/// Where the bundled binaries live inside the app.
#[derive(Clone, Debug)]
pub struct Binaries {
    pub engine: PathBuf,
    pub sidecar: PathBuf,
}

impl Binaries {
    #[must_use]
    pub fn in_dir(dir: &Path) -> Self {
        let exe = |name: &str| {
            dir.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_owned()
            })
        };
        Self {
            engine: exe("opencode"),
            sidecar: exe("workmate-sidecar"),
        }
    }
}

#[derive(Default)]
struct Children {
    engine: Option<Engine>,
    sidecar: Option<Sidecar>,
}

/// The runtime, managed by Tauri.
#[derive(Default)]
pub struct Runtime {
    children: Mutex<Children>,
}

impl Runtime {
    /// Start the engine, then the sidecar. Idempotent: a running pair is left
    /// alone rather than restarted, since a restart would lose in-flight work.
    ///
    /// # Errors
    /// Returns [`RuntimeError`] if either child fails to start. The engine is
    /// stopped again if the sidecar fails, so a half-started runtime is never
    /// left behind.
    pub fn start(&self, binaries: &Binaries, handler: Handler) -> Result<EngineAddress, RuntimeError> {
        let mut children = self.children.lock().map_err(|_| RuntimeError::Poisoned)?;
        if let Some(engine) = children.engine.as_ref() {
            if children.sidecar.is_some() {
                return Ok(engine.address().clone());
            }
        }

        let engine = Engine::start(&binaries.engine)?;
        let address = engine.address().clone();

        match Sidecar::start(&binaries.sidecar, &address, handler) {
            Ok(sidecar) => {
                children.engine = Some(engine);
                children.sidecar = Some(sidecar);
                Ok(address)
            }
            Err(e) => {
                // Dropping the engine stops it; leaving it running with no
                // sidecar would orphan a server nobody is talking to.
                drop(engine);
                Err(e.into())
            }
        }
    }

    /// Stop both, sidecar first.
    ///
    /// # Errors
    /// Returns [`RuntimeError::Poisoned`] if a holder of the lock panicked.
    /// Failure to signal a child is not an error: it is already gone.
    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        let mut children = self.children.lock().map_err(|_| RuntimeError::Poisoned)?;
        if let Some(mut sidecar) = children.sidecar.take() {
            let _ = sidecar.stop();
        }
        if let Some(mut engine) = children.engine.take() {
            let _ = engine.stop();
        }
        Ok(())
    }

    /// The engine address, if the runtime is up.
    ///
    /// # Errors
    /// Returns [`RuntimeError::Poisoned`] if a holder of the lock panicked.
    pub fn address(&self) -> Result<Option<EngineAddress>, RuntimeError> {
        let children = self.children.lock().map_err(|_| RuntimeError::Poisoned)?;
        Ok(children.engine.as_ref().map(|e| e.address().clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_ops() -> Handler {
        std::sync::Arc::new(|_, _| Ok(vec![]))
    }

    fn bundled() -> Binaries {
        Binaries::in_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries"))
    }

    #[test]
    fn binary_names_are_resolved_under_one_directory() {
        let b = Binaries::in_dir(Path::new("/x"));
        assert!(b.engine.ends_with("opencode") || b.engine.ends_with("opencode.exe"));
        assert!(
            b.sidecar.ends_with("workmate-sidecar") || b.sidecar.ends_with("workmate-sidecar.exe")
        );
    }

    #[test]
    fn shutdown_on_a_runtime_that_never_started_is_harmless() {
        Runtime::default().shutdown().expect("no children to stop");
    }

    #[test]
    fn a_missing_engine_fails_before_the_sidecar_is_ever_spawned() {
        let runtime = Runtime::default();
        let err = runtime
            .start(&Binaries::in_dir(Path::new("/nonexistent")), no_ops())
            .expect_err("must fail");
        assert!(matches!(err, RuntimeError::Engine(EngineError::BinaryMissing(_))));
        assert!(runtime.address().expect("address").is_none());
    }

    /// Starts the real bundled pair. Skipped until both have been built.
    #[test]
    fn the_bundled_pair_starts_and_stops_in_order() {
        let binaries = bundled();
        if !binaries.engine.exists() || !binaries.sidecar.exists() {
            eprintln!("skipping: run `pnpm sidecar:fetch && pnpm sidecar:build`");
            return;
        }
        let runtime = Runtime::default();
        let address = runtime.start(&binaries, no_ops()).expect("runtime should start");
        assert!(address.base_url.starts_with("http://127.0.0.1:"));
        assert!(runtime.address().expect("address").is_some());

        // Idempotent: a second start returns the same address, not a new engine.
        assert_eq!(runtime.start(&binaries, no_ops()).expect("second start"), address);

        runtime.shutdown().expect("shutdown");
        assert!(runtime.address().expect("address").is_none());
    }
}
