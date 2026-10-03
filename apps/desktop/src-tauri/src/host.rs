//! Operations only Rust can perform, served to the sidecar by name.
//!
//! The sidecar orchestrates but owns no filesystem authority, no keychain and no
//! database. It asks for a worktree, a permission ruleset or a provisioned
//! credential and gets back only what it needs: paths and rules, never a key
//! (tickets 017, 024 and 026). Anything that is not a host operation falls
//! through to [`crate::ops`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::credentials::{self, CredentialStore, Resolved};
use crate::db::Db;
use crate::engine::EngineAddress;
use crate::{ops, permissions, repo, workspace};

/// Everything a host operation may touch, borrowed for one call.
pub struct Host<'a, S: CredentialStore> {
    pub db: &'a Db,
    pub worktree_root: &'a Path,
    /// `None` until the engine is up.
    pub engine: Option<EngineAddress>,
    pub store: &'a S,
}

fn s<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string argument `{key}`"))
}

fn flag(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn to_row<T: serde::Serialize>(v: &T) -> Result<Vec<Value>, String> {
    serde_json::to_value(v).map(|v| vec![v]).map_err(|e| e.to_string())
}

impl<S: CredentialStore> Host<'_, S> {
    fn workspace_checkout(&self, args: &Value) -> Result<workspace::Workspace, String> {
        workspace::open(self.db, s(args, "workspaceId")?).map_err(|e| e.to_string())
    }

    fn worktree(&self, args: &Value) -> Result<PathBuf, String> {
        Ok(repo::worktree_path(self.worktree_root, s(args, "workspaceId")?, s(args, "runId")?))
    }

    /// Run one operation. Host operations are namespaced `repo.*`,
    /// `permission.*` and `credentials.*`; everything else is a database op.
    ///
    /// # Errors
    /// A message the sidecar receives as a `db.error`.
    pub fn dispatch(&self, op: &str, args: &Value) -> Result<Vec<Value>, String> {
        match op {
            "repo.createWorktree" => {
                let ws = self.workspace_checkout(args)?;
                let made = repo::create_worktree(&ws.directory, &self.worktree(args)?, s(args, "runId")?)
                    .map_err(|e| e.to_string())?;
                to_row(&made)
            }
            "repo.removeWorktree" => {
                let ws = self.workspace_checkout(args)?;
                repo::remove_worktree(&ws.directory, s(args, "runId")?, flag(args, "abandon"), flag(args, "force"))
                    .map_err(|e| e.to_string())?;
                Ok(vec![])
            }
            "repo.merge" => {
                let ws = self.workspace_checkout(args)?;
                to_row(&repo::merge(&ws.directory, s(args, "runId")?).map_err(|e| e.to_string())?)
            }
            "repo.diff" => to_row(&repo::diff(&self.worktree(args)?, s(args, "base")?).map_err(|e| e.to_string())?),
            "repo.status" => to_row(&repo::status(&self.worktree(args)?).map_err(|e| e.to_string())?),
            "permission.ruleset" => {
                let ws = self.workspace_checkout(args)?;
                let rules = permissions::ruleset(self.db, &ws, &self.worktree(args)?).map_err(|e| e.to_string())?;
                // Nobody is watching an unattended run, so nothing may ask.
                to_row(&if flag(args, "unattended") { permissions::without_prompts(rules) } else { rules })
            }
            "credentials.provision" => self.provision(args),
            other => ops::dispatch(self.db, other, args),
        }
    }

    /// Resolve a role's credential and hand it to the engine.
    ///
    /// Returns `{status:"ok"}` or `{status:"missing", tried:[…]}`. A miss is a
    /// normal answer, not an error: the run pauses and the user is asked, rather
    /// than the team's finished work being discarded (ticket 009). The secret is
    /// read, sent to the engine and dropped; it is in no return value.
    fn provision(&self, args: &Value) -> Result<Vec<Value>, String> {
        let provider = s(args, "providerId")?;
        let role = args.get("roleId").and_then(Value::as_str);
        let ws = args.get("workspaceId").and_then(Value::as_str);
        let found = credentials::resolve(self.store, provider, role, ws).map_err(|e| e.to_string())?;
        let account = match found {
            Resolved::Missing { tried } => return Ok(vec![json!({"status": "missing", "tried": tried})]),
            Resolved::Found { account, .. } => account,
        };
        let secret = self
            .store
            .secret(&account)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "the credential disappeared between lookup and read".to_owned())?;
        let engine = self.engine.as_ref().ok_or("the engine is not running")?;
        put_auth(engine, provider, &secret)?;
        Ok(vec![json!({"status": "ok", "account": account})])
    }
}

/// `PUT /auth/{provider}` with an API key. Errors never include the key.
fn put_auth(engine: &EngineAddress, provider: &str, secret: &str) -> Result<(), String> {
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("opencode:{}", engine.password));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let body = json!({"type": "api", "key": secret}).to_string();
    agent
        .put(format!("{}/auth/{provider}", engine.base_url))
        .header("authorization", format!("Basic {auth}"))
        .header("content-type", "application/json")
        .send(body)
        .map(|_| ())
        .map_err(|e| format!("the engine refused the credential for {provider}: {}", redact(&e.to_string(), secret)))
}

fn redact(message: &str, secret: &str) -> String {
    if secret.is_empty() { message.to_owned() } else { message.replace(secret, "[redacted]") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::CredentialError;
    use crate::ids::new_id;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    struct Store(HashMap<String, String>);
    impl CredentialStore for Store {
        fn contains(&self, a: &str) -> Result<bool, CredentialError> {
            Ok(self.0.contains_key(a))
        }
        fn secret(&self, a: &str) -> Result<Option<String>, CredentialError> {
            Ok(self.0.get(a).cloned())
        }
    }

    fn repo_workspace(db: &Db) -> (workspace::Workspace, PathBuf, PathBuf) {
        let root = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(new_id("host"));
        let checkout = root.join("project");
        std::fs::create_dir_all(&checkout).unwrap();
        let r = git2::Repository::init(&checkout).unwrap();
        std::fs::write(checkout.join("a.txt"), "x").unwrap();
        let mut idx = r.index().unwrap();
        idx.add_path(Path::new("a.txt")).unwrap();
        let tree = r.find_tree(idx.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@t").unwrap();
        r.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();
        (workspace::create(db, "p", &checkout).unwrap(), root.join("wts"), root)
    }

    #[test]
    fn a_worktree_and_its_ruleset_come_from_ids_not_paths() {
        let db = Db::open_in_memory().unwrap();
        let (ws, wts, root) = repo_workspace(&db);
        let store = Store(HashMap::new());
        let host = Host { db: &db, worktree_root: &wts, engine: None, store: &store };
        let args = json!({"workspaceId": ws.id, "runId": "run_ab"});

        let made = host.dispatch("repo.createWorktree", &args).unwrap();
        assert_eq!(made[0]["branch"], "workmate/run-ab");
        let path = made[0]["path"].as_str().unwrap();
        assert!(path.starts_with(wts.to_str().unwrap()), "outside the user's tree");

        let rules = host.dispatch("permission.ruleset", &args).unwrap();
        let text = rules[0].to_string();
        assert!(text.contains(path), "the ruleset widens exactly this worktree");

        host.dispatch("repo.removeWorktree", &json!({"workspaceId": ws.id, "runId": "run_ab", "abandon": true})).unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unknown_names_fall_through_to_the_database_operations() {
        let db = Db::open_in_memory().unwrap();
        let store = Store(HashMap::new());
        let host = Host { db: &db, worktree_root: Path::new("/x"), engine: None, store: &store };
        assert!(host.dispatch("run.load", &json!({"id": "none"})).unwrap().is_empty());
        assert!(host.dispatch("repo.nope", &json!({})).unwrap_err().contains("unknown operation"));
    }

    #[test]
    fn a_missing_credential_is_an_answer_that_lists_what_was_tried() {
        let db = Db::open_in_memory().unwrap();
        let store = Store(HashMap::new());
        let host = Host { db: &db, worktree_root: Path::new("/x"), engine: None, store: &store };
        let out = host
            .dispatch("credentials.provision", &json!({"providerId":"anthropic","roleId":"coder","workspaceId":"w"}))
            .unwrap();
        assert_eq!(out[0]["status"], "missing");
        assert_eq!(out[0]["tried"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_found_key_goes_to_the_engine_and_nowhere_in_the_reply() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(String::new()));
        let got = Arc::clone(&seen);
        std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = c.read(&mut buf).unwrap();
            *got.lock().unwrap() = String::from_utf8_lossy(&buf[..n]).into_owned();
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 4\r\nConnection: close\r\n\r\ntrue");
        });

        let db = Db::open_in_memory().unwrap();
        let acct = credentials::account(&credentials::Scope::Global, "anthropic");
        let store = Store(HashMap::from([(acct.clone(), "sk-live-12345".to_owned())]));
        let host = Host {
            db: &db,
            worktree_root: Path::new("/x"),
            engine: Some(EngineAddress { base_url, password: "pw".into() }),
            store: &store,
        };
        let out = host.dispatch("credentials.provision", &json!({"providerId":"anthropic"})).unwrap();
        assert_eq!(out[0], json!({"status": "ok", "account": acct}));
        assert!(!out[0].to_string().contains("sk-live"), "the key is in no reply");

        let req = seen.lock().unwrap().clone();
        assert!(req.starts_with("PUT /auth/anthropic "), "{req}");
        assert!(req.contains(r#""key":"sk-live-12345""#));
        let want = base64::engine::general_purpose::STANDARD.encode("opencode:pw");
        assert!(req.contains(&want));
    }

    #[test]
    fn errors_never_echo_the_key() {
        assert_eq!(redact("bad key sk-abc in body", "sk-abc"), "bad key [redacted] in body");
    }
}
