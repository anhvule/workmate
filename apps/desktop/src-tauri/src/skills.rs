//! The skills catalog.
//!
//! Skill *discovery* is the engine's: it reads `.opencode/skills/<name>/SKILL.md`
//! from the project. What workmate adds is everything around getting a skill
//! there safely — git sync from sources the user chose, frontmatter validation,
//! a checksum so a skill the user edited is never silently replaced, and
//! drift-aware install and removal (ticket 020).
//!
//! Installing writes into the user's checkout, so like a starter pack it is
//! only ever the user's explicit action and never overwrites.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::params;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::{now_ms, Db, DbError};
use crate::ids::new_id;
use crate::workspace::Workspace;

/// Where the engine looks.
pub const INSTALL_DIR: &str = ".opencode/skills";
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("not a valid skill: {0}")]
    Invalid(String),
    #[error("`{0}` is already installed here")]
    AlreadyInstalled(String),
    #[error("`{0}` has local changes; keep them or pass force")]
    Modified(String),
    #[error("`{0}` is not installed")]
    NotInstalled(String),
    #[error("{0} is not a source workmate can sync: use an https:// or ssh URL, or a local path")]
    BadSource(String),
    #[error("git failed: {0}")]
    Git(String),
    #[error("no such source")]
    NoSource,
    #[error("filesystem: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: String,
}

/// Parse a `SKILL.md`: frontmatter between `---` lines with `name` and
/// `description`. A hand-rolled `key: value` reader — the format needed is that
/// small, and a YAML dependency would be a parser for untrusted input.
///
/// # Errors
/// [`SkillError::Invalid`] naming what is missing or malformed.
pub fn parse(text: &str) -> Result<Skill, SkillError> {
    let bad = |m: &str| SkillError::Invalid(m.to_owned());
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(bad("SKILL.md must start with a `---` frontmatter block"));
    }
    let (mut name, mut description) = (None, None);
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'').to_owned();
            match k.trim() {
                "name" => name = Some(v),
                "description" => description = Some(v),
                _ => {}
            }
        }
    }
    if !closed {
        return Err(bad("the frontmatter block is never closed"));
    }
    let name = name.filter(|n| valid_name(n)).ok_or_else(|| bad("`name` must be 1-64 lowercase letters, digits or `-`"))?;
    let description = description
        .filter(|d| !d.is_empty() && d.chars().count() <= 1024)
        .ok_or_else(|| bad("`description` is required and at most 1024 characters"))?;
    Ok(Skill { name, description })
}

fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 64
        && !n.starts_with('-')
        && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The files of a skill, relative, sorted; regular files only and never `.git`.
fn files_of(dir: &Path) -> Result<Vec<PathBuf>, SkillError> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for e in fs::read_dir(at)? {
            let e = e?;
            let ft = e.file_type()?;
            if e.file_name() == ".git" || ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                walk(root, &e.path(), out)?;
            } else if ft.is_file() {
                out.push(e.path().strip_prefix(root).unwrap_or(&e.path()).to_owned());
            }
        }
        Ok(())
    }
    let mut out = vec![];
    walk(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

/// A digest of a skill directory: every file's path and bytes, in order.
///
/// # Errors
/// An I/O failure.
pub fn checksum(dir: &Path) -> Result<String, SkillError> {
    let mut h = Sha256::new();
    for rel in files_of(dir)? {
        h.update(rel.to_string_lossy().as_bytes());
        h.update([0]);
        h.update(fs::read(dir.join(&rel))?);
        h.update([0]);
    }
    let mut hex = String::with_capacity(64);
    for b in h.finalize() {
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    pub checksum: String,
    /// `"workmate"` for the bundled skills, otherwise the source's URL.
    pub origin: String,
}

/// Find the skills under `root`: any directory holding a valid `SKILL.md` whose
/// `name` matches the directory it lives in. Invalid ones are left out.
#[must_use]
pub fn scan(root: &Path, origin: &str) -> Vec<Entry> {
    fn go(at: &Path, depth: usize, origin: &str, out: &mut Vec<Entry>) {
        let Ok(rd) = fs::read_dir(at) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() || e.file_name() == ".git" || depth > MAX_DEPTH {
                continue;
            }
            if let Ok(text) = fs::read_to_string(p.join("SKILL.md")) {
                match parse(&text) {
                    Ok(s) if p.file_name().is_some_and(|n| n.to_string_lossy() == s.name) => {
                        if let Ok(sum) = checksum(&p) {
                            out.push(Entry { name: s.name, description: s.description, dir: p.clone(), checksum: sum, origin: origin.to_owned() });
                        }
                        continue;
                    }
                    _ => eprintln!("skills: skipping invalid skill {}", p.display()),
                }
            }
            go(&p, depth + 1, origin, out);
        }
    }
    let mut out = vec![];
    go(root, 0, origin, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.origin.cmp(&b.origin)));
    out
}

/// A URL or path workmate will hand to `git clone`.
fn valid_source(url: &str) -> bool {
    let remote = url.starts_with("https://") || url.starts_with("ssh://") || url.starts_with("git@");
    // A leading `-` would be read by git as an option.
    (remote || Path::new(url).is_absolute()) && !url.starts_with('-') && !url.contains(['\n', '\0'])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub url: String,
    pub last_synced_at: Option<i64>,
}

/// Register a source repository.
///
/// # Errors
/// [`SkillError::BadSource`] for a URL workmate will not hand to git.
pub fn add_source(db: &Db, url: &str) -> Result<Source, SkillError> {
    let url = url.trim();
    if !valid_source(url) {
        return Err(SkillError::BadSource(url.to_owned()));
    }
    let id = new_id("src");
    db.with(|c| {
        c.execute(
            "INSERT OR IGNORE INTO skill_source (id,url,created_at) VALUES (?1,?2,?3)",
            params![id, url, now_ms()],
        )
    })?;
    let got = sources(db)?.into_iter().find(|s| s.url == url).ok_or(SkillError::NoSource)?;
    Ok(got)
}

/// # Errors
/// [`SkillError::Db`] on a read failure.
pub fn sources(db: &Db) -> Result<Vec<Source>, SkillError> {
    Ok(db.with(|c| {
        let mut s = c.prepare("SELECT id,url,last_synced_at FROM skill_source ORDER BY created_at, id")?;
        let rows = s.query_map([], |r| Ok(Source { id: r.get(0)?, url: r.get(1)?, last_synced_at: r.get(2)? }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

/// Forget a source and its cached clone.
///
/// # Errors
/// [`SkillError::NoSource`] for an unknown id.
pub fn remove_source(db: &Db, cache: &Path, id: &str) -> Result<(), SkillError> {
    let n = db.with(|c| c.execute("DELETE FROM skill_source WHERE id=?1", [id]))?;
    if n == 0 {
        return Err(SkillError::NoSource);
    }
    let _ = fs::remove_dir_all(cache.join(id));
    Ok(())
}

fn git(args: &[&str], cwd: Option<&Path>) -> Result<(), SkillError> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        // A credential prompt would hang a background sync forever.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "true");
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let out = cmd.output().map_err(|e| SkillError::Git(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(SkillError::Git(String::from_utf8_lossy(&out.stderr).trim().to_owned()))
    }
}

/// Clone or fast-forward a source into the cache. Shelling out is the one
/// place workmate does: it is the only way to inherit the user's git
/// credentials and ssh config (ticket 007).
///
/// # Errors
/// [`SkillError::Git`] with git's own message.
pub fn sync(db: &Db, cache: &Path, id: &str) -> Result<Vec<Entry>, SkillError> {
    let src = sources(db)?.into_iter().find(|s| s.id == id).ok_or(SkillError::NoSource)?;
    fs::create_dir_all(cache)?;
    let dest = cache.join(&src.id);
    if dest.join(".git").exists() {
        git(&["pull", "--ff-only", "--quiet"], Some(&dest))?;
    } else {
        let d = dest.to_string_lossy();
        // `--` so a URL can never be read as an option.
        git(&["clone", "--depth", "1", "--quiet", "--", &src.url, &d], None)?;
    }
    db.with(|c| c.execute("UPDATE skill_source SET last_synced_at=?2 WHERE id=?1", params![id, now_ms()]))?;
    Ok(scan(&dest, &src.url))
}

/// The bundled skills plus every synced source, bundled first.
///
/// # Errors
/// [`SkillError::Db`] on a read failure.
pub fn catalog(db: &Db, bundled: &Path, cache: &Path) -> Result<Vec<Entry>, SkillError> {
    let mut all = scan(bundled, "workmate");
    for s in sources(db)? {
        all.extend(scan(&cache.join(&s.id), &s.url));
    }
    Ok(all)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub name: String,
    pub origin: String,
    /// The user (or an agent) has changed it since it was installed.
    pub modified: bool,
}

fn target(ws: &Workspace, name: &str) -> PathBuf {
    ws.directory.join(INSTALL_DIR).join(name)
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), SkillError> {
    let mut total = 0u64;
    for rel in files_of(from)? {
        let bytes = fs::read(from.join(&rel))?;
        total += bytes.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(SkillError::Invalid("a skill over 2 MiB is not a skill".into()));
        }
        let dest = to.join(&rel);
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(dest, bytes)?;
    }
    Ok(())
}

/// Copy a catalog skill into a workspace. Never overwrites.
///
/// # Errors
/// [`SkillError::AlreadyInstalled`] if the name is taken; an I/O failure.
pub fn install(db: &Db, ws: &Workspace, entry: &Entry) -> Result<(), SkillError> {
    let dest = target(ws, &entry.name);
    if fs::symlink_metadata(&dest).is_ok() {
        return Err(SkillError::AlreadyInstalled(entry.name.clone()));
    }
    if let Err(e) = copy_dir(&entry.dir, &dest) {
        let _ = fs::remove_dir_all(&dest); // no half-installed skill
        return Err(e);
    }
    db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO skill_install (workspace_id,name,origin,checksum,installed_at) VALUES (?1,?2,?3,?4,?5)",
            params![ws.id, entry.name, entry.origin, checksum(&dest).unwrap_or_default(), now_ms()],
        )
    })?;
    Ok(())
}

/// What is installed in a workspace and whether it has drifted.
///
/// # Errors
/// [`SkillError::Db`] on a read failure.
pub fn installed(db: &Db, ws: &Workspace) -> Result<Vec<Installed>, SkillError> {
    let rows: Vec<(String, String, String)> = db.with(|c| {
        let mut s = c.prepare("SELECT name,origin,checksum FROM skill_install WHERE workspace_id=?1 ORDER BY name")?;
        let r = s.query_map([&ws.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        r.collect::<rusqlite::Result<_>>()
    })?;
    Ok(rows
        .into_iter()
        .map(|(name, origin, sum)| {
            let now = checksum(&target(ws, &name)).ok();
            Installed { modified: now.as_deref() != Some(sum.as_str()), name, origin }
        })
        .collect())
}

/// Remove an installed skill. A modified one is kept unless `force`.
///
/// # Errors
/// [`SkillError::NotInstalled`], [`SkillError::Modified`], or an I/O failure.
pub fn uninstall(db: &Db, ws: &Workspace, name: &str, force: bool) -> Result<(), SkillError> {
    if !valid_name(name) {
        return Err(SkillError::NotInstalled(name.to_owned()));
    }
    let item = installed(db, ws)?.into_iter().find(|i| i.name == name).ok_or_else(|| SkillError::NotInstalled(name.to_owned()))?;
    if item.modified && !force {
        return Err(SkillError::Modified(name.to_owned()));
    }
    let _ = fs::remove_dir_all(target(ws, name));
    db.with(|c| c.execute("DELETE FROM skill_install WHERE workspace_id=?1 AND name=?2", params![ws.id, name]))?;
    Ok(())
}

/// Update an installed skill from the catalog, unless the user changed it.
///
/// # Errors
/// [`SkillError::Modified`] if it has local changes (and not `force`).
pub fn update(db: &Db, ws: &Workspace, entry: &Entry, force: bool) -> Result<(), SkillError> {
    uninstall(db, ws, &entry.name, force)?;
    install(db, ws, entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let p = fs::canonicalize(std::env::temp_dir()).unwrap().join(new_id("skl"));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_skill(root: &Path, name: &str, desc: &str, extra: &str) {
        let d = root.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("SKILL.md"), format!("---\nname: {name}\ndescription: {desc}\n---\n\n{extra}\n")).unwrap();
    }

    fn bundled() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("skills")
    }

    fn ws(db: &Db, dir: &Path) -> Workspace {
        crate::workspace::create(db, "p", dir).unwrap()
    }

    #[test]
    fn frontmatter_is_parsed_and_malformed_skills_are_named() {
        let s = parse("---\nname: my-skill\ndescription: \"Does a thing.\"\n---\nbody").unwrap();
        assert_eq!((s.name.as_str(), s.description.as_str()), ("my-skill", "Does a thing."));
        for bad in [
            "no frontmatter",
            "---\nname: x\n---\n",
            "---\ndescription: d\n---\n",
            "---\nname: Bad_Name\ndescription: d\n---\n",
            "---\nname: ok\ndescription: d\n",
            "---\nname: -lead\ndescription: d\n---\n",
        ] {
            assert!(matches!(parse(bad), Err(SkillError::Invalid(_))), "{bad:?}");
        }
    }

    #[test]
    fn the_checksum_follows_content_and_not_the_clock() {
        let t = Tmp::new();
        write_skill(&t.0, "a", "d", "one");
        let first = checksum(&t.0.join("a")).unwrap();
        assert_eq!(first, checksum(&t.0.join("a")).unwrap());
        fs::write(t.0.join("a/extra.md"), "x").unwrap();
        assert_ne!(first, checksum(&t.0.join("a")).unwrap(), "a new file changes it");
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn the_bundled_skills_are_valid() {
        let names: Vec<_> = scan(&bundled(), "workmate").into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["conventional-commits", "review-checklist"]);
    }

    #[test]
    fn scanning_skips_a_skill_whose_name_does_not_match_its_folder() {
        let t = Tmp::new();
        write_skill(&t.0, "good", "d", "");
        fs::create_dir_all(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/SKILL.md"), "---\nname: other\ndescription: d\n---\n").unwrap();
        assert_eq!(scan(&t.0, "o").iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["good"]);
    }

    #[test]
    fn install_never_overwrites_and_places_the_skill_where_the_engine_looks() {
        let db = Db::open_in_memory().unwrap();
        let t = Tmp::new();
        let w = ws(&db, &t.0);
        let entry = scan(&bundled(), "workmate").remove(0);
        install(&db, &w, &entry).unwrap();
        assert!(t.0.join(".opencode/skills/conventional-commits/SKILL.md").exists());
        assert!(matches!(install(&db, &w, &entry), Err(SkillError::AlreadyInstalled(_))));
        assert_eq!(installed(&db, &w).unwrap(), [Installed { name: entry.name.clone(), origin: "workmate".into(), modified: false }]);
    }

    #[test]
    fn a_skill_the_user_edited_is_flagged_and_protected() {
        let db = Db::open_in_memory().unwrap();
        let t = Tmp::new();
        let w = ws(&db, &t.0);
        let entry = scan(&bundled(), "workmate").remove(0);
        install(&db, &w, &entry).unwrap();
        fs::write(t.0.join(".opencode/skills/conventional-commits/NOTES.md"), "mine").unwrap();
        assert!(installed(&db, &w).unwrap()[0].modified);
        assert!(matches!(update(&db, &w, &entry, false), Err(SkillError::Modified(_))));
        assert!(matches!(uninstall(&db, &w, &entry.name, false), Err(SkillError::Modified(_))));
        assert!(t.0.join(".opencode/skills/conventional-commits/NOTES.md").exists(), "refusal changed nothing");
        update(&db, &w, &entry, true).unwrap();
        assert!(!installed(&db, &w).unwrap()[0].modified);
    }

    #[test]
    fn uninstalling_removes_the_files_and_the_record() {
        let db = Db::open_in_memory().unwrap();
        let t = Tmp::new();
        let w = ws(&db, &t.0);
        let entry = scan(&bundled(), "workmate").remove(0);
        install(&db, &w, &entry).unwrap();
        uninstall(&db, &w, &entry.name, false).unwrap();
        assert!(!t.0.join(".opencode/skills/conventional-commits").exists());
        assert!(installed(&db, &w).unwrap().is_empty());
        assert!(matches!(uninstall(&db, &w, "../../etc", false), Err(SkillError::NotInstalled(_))));
    }

    #[test]
    fn only_sources_git_could_safely_be_given_are_accepted() {
        let db = Db::open_in_memory().unwrap();
        for bad in ["", "--upload-pack=evil", "ext::sh -c evil", "relative/path", "http://insecure", "file:relative", "-x"] {
            assert!(matches!(add_source(&db, bad), Err(SkillError::BadSource(_))), "{bad:?}");
        }
        assert!(add_source(&db, "https://example.com/skills.git").is_ok());
        assert!(add_source(&db, "git@github.com:me/skills.git").is_ok());
        let again = add_source(&db, "https://example.com/skills.git").unwrap();
        assert_eq!(sources(&db).unwrap().len(), 2, "the same URL is one source");
        assert_eq!(again.url, "https://example.com/skills.git");
    }

    /// Needs a `git` binary, like the feature itself.
    #[test]
    fn a_source_syncs_from_git_and_a_later_commit_is_picked_up() {
        if Command::new("git").arg("--version").output().is_err() {
            eprintln!("skipping: no git on PATH");
            return;
        }
        let db = Db::open_in_memory().unwrap();
        let remote = Tmp::new();
        let cache = Tmp::new();
        let run = |args: &[&str]| git(args, Some(&remote.0)).unwrap();
        run(&["init", "--quiet"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        write_skill(&remote.0.join("skills"), "alpha", "first skill", "");
        run(&["add", "-A"]);
        run(&["commit", "--quiet", "-m", "one"]);

        let src = add_source(&db, remote.0.to_str().unwrap()).unwrap();
        let first = sync(&db, &cache.0, &src.id).unwrap();
        assert_eq!(first.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["alpha"]);
        assert!(sources(&db).unwrap()[0].last_synced_at.is_some());

        write_skill(&remote.0.join("skills"), "beta", "second skill", "");
        run(&["add", "-A"]);
        run(&["commit", "--quiet", "-m", "two"]);
        let second = sync(&db, &cache.0, &src.id).unwrap();
        assert_eq!(second.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["alpha", "beta"]);

        let all = catalog(&db, &bundled(), &cache.0).unwrap();
        assert_eq!(all.len(), 4, "two bundled plus two synced");
        assert_eq!(all[0].origin, "workmate");
        remove_source(&db, &cache.0, &src.id).unwrap();
        assert!(!cache.0.join(&src.id).exists());
    }

    #[test]
    fn a_failed_sync_surfaces_gits_own_message() {
        let db = Db::open_in_memory().unwrap();
        let cache = Tmp::new();
        let src = add_source(&db, "/nonexistent/definitely/not/a/repo").unwrap();
        assert!(matches!(sync(&db, &cache.0, &src.id), Err(SkillError::Git(m)) if !m.is_empty()));
    }
}
