//! Starter packs: workmate's own file trees that set a project up.
//!
//! A pack is a `pack.json` plus the files it ships, and — the part that makes it
//! worth more than a folder template — it can ship a **team**: a planner,
//! builder and reviewer configured and ready, not just directories (ticket 020).
//!
//! Every pack here is written for workmate. Cowork-z's corpora have unclear
//! provenance and are deliberately not copied (ticket 002).
//!
//! Applying a pack writes into the user's checkout, which is the one place agents
//! may not write — so it is only ever the user's explicit action, shows what it
//! will write first, and **never overwrites**: an existing file is skipped and
//! reported, not replaced.

use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::team::{self, Role, Team};
use crate::workspace::Workspace;

/// A shipped file bigger than this is almost certainly a mistake.
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("pack `{0}`: {1}")]
    Invalid(String, String),
    #[error("{0} is not a safe path inside a project")]
    UnsafePath(String),
    #[error("no such pack: {0}")]
    Unknown(String),
    #[error("filesystem: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Team(#[from] team::TeamError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackFile {
    pub path: String,
    pub from: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSpec {
    pub name: String,
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub team: TeamSpec,
    #[serde(default)]
    pub files: Vec<PackFile>,
}

/// A relative path that stays inside the project and never reaches `.git`.
///
/// # Errors
/// [`PackError::UnsafePath`] for an absolute path, `..`, an empty path or a
/// `.git` component.
pub fn safe_relative(p: &str) -> Result<PathBuf, PackError> {
    let path = Path::new(p);
    let ok = !p.is_empty()
        && path.components().all(|c| match c {
            Component::Normal(n) => n != ".git",
            _ => false,
        });
    if ok { Ok(path.to_owned()) } else { Err(PackError::UnsafePath(p.to_owned())) }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Read and validate one pack directory.
///
/// # Errors
/// [`PackError::Invalid`] naming what is wrong, or an I/O failure.
pub fn load(dir: &Path) -> Result<Pack, PackError> {
    let label = dir.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let bad = |why: &str| PackError::Invalid(label.clone(), why.to_owned());
    let pack: Pack = serde_json::from_str(&fs::read_to_string(dir.join("pack.json"))?)
        .map_err(|e| bad(&e.to_string()))?;
    if !valid_id(&pack.id) {
        return Err(bad("id must be lowercase letters, digits and `-`"));
    }
    if pack.team.roles.is_empty() {
        return Err(bad("a pack ships a team of at least one role"));
    }
    if pack.team.roles.iter().any(|r| !valid_id(&r.id)) {
        return Err(bad("role ids must be lowercase letters, digits and `-`"));
    }
    for f in &pack.files {
        safe_relative(&f.path)?;
        safe_relative(&f.from)?;
        let meta = fs::metadata(dir.join(&f.from)).map_err(|_| bad(&format!("missing file {}", f.from)))?;
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            return Err(bad(&format!("{} is not a reasonable file", f.from)));
        }
    }
    Ok(pack)
}

/// Every valid pack under `root`, sorted by id. An invalid pack is reported on
/// stderr and left out rather than hiding the rest.
#[must_use]
pub fn load_all(root: &Path) -> Vec<(Pack, PathBuf)> {
    let Ok(entries) = fs::read_dir(root) else { return vec![] };
    let mut out: Vec<(Pack, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| match load(&e.path()) {
            Ok(p) => Some((p, e.path())),
            Err(err) => {
                eprintln!("packs: skipping {}: {err}", e.path().display());
                None
            }
        })
        .collect();
    out.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    out
}

/// What applying a pack would do, without doing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub will_write: Vec<String>,
    pub will_skip: Vec<String>,
}

fn exists(p: &Path) -> bool {
    // `symlink_metadata`: a dangling symlink still counts as "there".
    fs::symlink_metadata(p).is_ok()
}

/// # Errors
/// [`PackError::UnsafePath`] if a shipped path is unsafe.
pub fn preview(pack: &Pack, workspace: &Path) -> Result<Preview, PackError> {
    let (mut write, mut skip) = (vec![], vec![]);
    for f in &pack.files {
        let rel = safe_relative(&f.path)?;
        if exists(&workspace.join(&rel)) { skip.push(f.path.clone()) } else { write.push(f.path.clone()) }
    }
    Ok(Preview { will_write: write, will_skip: skip })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub team: Team,
    pub written: Vec<String>,
    pub skipped: Vec<String>,
}

/// Write one shipped file without overwriting or leaving the project.
fn write_new(workspace: &Path, rel: &Path, bytes: &[u8]) -> Result<bool, PackError> {
    let target = workspace.join(rel);
    if exists(&target) {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
        // A symlinked directory inside the project must not carry the write out of it.
        if !fs::canonicalize(parent)?.starts_with(fs::canonicalize(workspace)?) {
            return Err(PackError::UnsafePath(rel.display().to_string()));
        }
    }
    // `create_new` closes the race between the check above and the write.
    match fs::OpenOptions::new().write(true).create_new(true).open(&target) {
        Ok(mut f) => f.write_all(bytes).map(|()| true).map_err(Into::into),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Apply a pack: create its team (roles namespaced by pack id, so two packs'
/// `builder`s never collide) and write its files that are not already there.
///
/// # Errors
/// [`PackError`] for an unsafe path, an unwritable file or a failed team write.
pub fn apply(db: &Db, pack_dir: &Path, pack: &Pack, ws: &Workspace) -> Result<Applied, PackError> {
    // Validate every path before the first write so a bad pack writes nothing.
    preview(pack, &ws.directory)?;
    let (mut written, mut skipped) = (vec![], vec![]);
    for f in &pack.files {
        let bytes = fs::read(pack_dir.join(safe_relative(&f.from)?))?;
        if write_new(&ws.directory, &safe_relative(&f.path)?, &bytes)? {
            written.push(f.path.clone());
        } else {
            skipped.push(f.path.clone());
        }
    }
    let roles: Vec<Role> = pack
        .team
        .roles
        .iter()
        .map(|r| Role { id: format!("{}.{}", pack.id, r.id), ..r.clone() })
        .collect();
    let team = team::create(db, &pack.team.name, &roles)?;
    Ok(Applied { team, written, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::new_id;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let p = fs::canonicalize(std::env::temp_dir()).unwrap().join(new_id("pack"));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn shipped() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("packs")
    }

    fn workspace(db: &Db, dir: &Path) -> Workspace {
        crate::workspace::create(db, "p", dir).unwrap()
    }

    #[test]
    fn every_shipped_pack_loads_and_ships_a_team() {
        let packs = load_all(&shipped());
        let ids: Vec<_> = packs.iter().map(|(p, _)| p.id.as_str()).collect();
        assert_eq!(ids, ["docs-pair", "plan-build-review", "solo"]);
        assert!(packs.iter().all(|(p, _)| !p.team.roles.is_empty()));
        assert_eq!(packs.iter().find(|(p, _)| p.id == "solo").unwrap().0.team.roles.len(), 1, "the solo pack is a plain chat");
        let pbr = &packs.iter().find(|(p, _)| p.id == "plan-build-review").unwrap().0;
        assert_eq!(pbr.team.roles.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["planner", "builder", "reviewer"]);
    }

    #[test]
    fn applying_creates_the_team_namespaced_and_writes_the_files() {
        let db = Db::open_in_memory().unwrap();
        let t = Tmp::new();
        let ws = workspace(&db, &t.0);
        let (pack, dir) = load_all(&shipped()).into_iter().find(|(p, _)| p.id == "plan-build-review").unwrap();
        let out = apply(&db, &dir, &pack, &ws).unwrap();
        assert_eq!(out.written, ["AGENTS.md"]);
        assert!(t.0.join("AGENTS.md").exists());
        assert_eq!(out.team.roles[0].id, "plan-build-review.planner");
        assert_eq!(team::list(&db).unwrap()[0].roles.len(), 3);
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let db = Db::open_in_memory().unwrap();
        let t = Tmp::new();
        fs::write(t.0.join("AGENTS.md"), "mine").unwrap();
        let ws = workspace(&db, &t.0);
        let (pack, dir) = load_all(&shipped()).into_iter().find(|(p, _)| p.id == "solo").unwrap();
        assert_eq!(preview(&pack, &t.0).unwrap(), Preview { will_write: vec![], will_skip: vec!["AGENTS.md".into()] });
        let out = apply(&db, &dir, &pack, &ws).unwrap();
        assert_eq!((out.written.len(), out.skipped), (0, vec!["AGENTS.md".to_owned()]));
        assert_eq!(fs::read_to_string(t.0.join("AGENTS.md")).unwrap(), "mine");
    }

    #[test]
    fn paths_that_escape_or_touch_git_are_refused() {
        for bad in ["", "/etc/passwd", "../x", "a/../../x", ".git/config", "src/.git/hooks/pre-commit"] {
            assert!(safe_relative(bad).is_err(), "{bad:?}");
        }
        for ok in ["AGENTS.md", "docs/guide.md", ".opencode/skills/x/SKILL.md"] {
            assert!(safe_relative(ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn a_hostile_pack_writes_nothing_at_all() {
        let db = Db::open_in_memory().unwrap();
        let t = Tmp::new();
        let ws = workspace(&db, &t.0);
        let pack_dir = Tmp::new();
        fs::write(pack_dir.0.join("ok.txt"), "x").unwrap();
        let pack = Pack {
            id: "evil".into(),
            name: "e".into(),
            description: String::new(),
            team: TeamSpec { name: "t".into(), roles: vec![Role { id: "r".into(), name: "R".into(), system_prompt: String::new(), provider_id: None, model_id: None, tool_allowlist: vec![] }] },
            files: vec![
                PackFile { path: "fine.txt".into(), from: "ok.txt".into() },
                PackFile { path: ".git/hooks/post-checkout".into(), from: "ok.txt".into() },
            ],
        };
        assert!(matches!(apply(&db, &pack_dir.0, &pack, &ws), Err(PackError::UnsafePath(_))));
        assert!(!t.0.join("fine.txt").exists(), "validated before the first write");
        assert!(team::list(&db).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_cannot_carry_a_write_out_of_the_project() {
        let db = Db::open_in_memory().unwrap();
        let outside = Tmp::new();
        let t = Tmp::new();
        std::os::unix::fs::symlink(&outside.0, t.0.join("docs")).unwrap();
        let ws = workspace(&db, &t.0);
        let pack_dir = Tmp::new();
        fs::write(pack_dir.0.join("f"), "x").unwrap();
        let pack = Pack {
            id: "p".into(),
            name: "p".into(),
            description: String::new(),
            team: TeamSpec { name: "t".into(), roles: vec![Role { id: "r".into(), name: "R".into(), system_prompt: String::new(), provider_id: None, model_id: None, tool_allowlist: vec![] }] },
            files: vec![PackFile { path: "docs/guide.md".into(), from: "f".into() }],
        };
        assert!(apply(&db, &pack_dir.0, &pack, &ws).is_err());
        assert!(!outside.0.join("guide.md").exists());
    }

    #[test]
    fn an_invalid_pack_is_named_and_does_not_hide_the_others() {
        let t = Tmp::new();
        fs::create_dir_all(t.0.join("broken")).unwrap();
        fs::write(t.0.join("broken/pack.json"), "{ not json").unwrap();
        fs::create_dir_all(t.0.join("Bad_Id")).unwrap();
        fs::write(
            t.0.join("Bad_Id/pack.json"),
            r#"{"id":"Bad_Id","name":"x","description":"","team":{"name":"t","roles":[{"id":"r","name":"R"}]}}"#,
        )
        .unwrap();
        fs::create_dir_all(t.0.join("good")).unwrap();
        fs::write(
            t.0.join("good/pack.json"),
            r#"{"id":"good","name":"x","description":"","team":{"name":"t","roles":[{"id":"r","name":"R"}]}}"#,
        )
        .unwrap();
        let ids: Vec<_> = load_all(&t.0).into_iter().map(|(p, _)| p.id).collect();
        assert_eq!(ids, ["good"]);
    }
}
