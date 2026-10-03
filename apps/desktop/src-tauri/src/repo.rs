//! The repo-native surface: a run works in its own git worktree.
//!
//! Workmate creates the branch and the worktree; an agent never does. The
//! worktree lives under workmate's own data directory, outside the user's tree,
//! so a run cannot litter the checkout and `.git/` stays unreachable (the
//! permission ruleset enforces that; this module only decides where things go).
//! Merging into the user's checkout is an explicit call that refuses rather than
//! guesses — it never runs unasked (ticket 007).
//!
//! `git2` in-process, because status and diff run on every change and a
//! subprocess per query does not survive that.

use std::path::{Path, PathBuf};

use git2::{
    BranchType, DiffFormat, DiffOptions, MergeAnalysis, Repository, Signature, StatusOptions,
    WorktreeAddOptions, WorktreePruneOptions,
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("{0} is not inside a git repository")]
    NotARepo(PathBuf),
    #[error("the repository has no commits yet, so there is nothing to branch from")]
    NoCommits,
    #[error("a worktree for run {0} already exists")]
    AlreadyExists(String),
    #[error("no worktree for run {0}")]
    NoWorktree(String),
    #[error("the run's worktree has uncommitted changes; discarding them needs an explicit force")]
    DirtyWorktree,
    #[error("your checkout has uncommitted changes to tracked files; commit or stash them first")]
    DirtyCheckout,
    #[error("your checkout is on a detached HEAD, so there is no branch to merge into")]
    DetachedHead,
    #[error("merging {0} would conflict; nothing was changed")]
    Conflicts(String),
    #[error("{0} is not a usable path: workmate needs UTF-8")]
    NotUtf8(PathBuf),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("filesystem: {0}")]
    Io(#[from] std::io::Error),
}

/// A run's worktree and the commit it branched from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    /// The commit the branch started at; the per-run diff is taken against it.
    pub base: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub kind: ChangeKind,
}

/// Everything a run changed since it branched: committed or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDiff {
    pub files: Vec<FileDiff>,
    pub additions: usize,
    pub deletions: usize,
    pub patch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeOutcome {
    UpToDate,
    FastForward,
    Merged,
}

/// The branch a run works on. The `run_` prefix of the id is dropped so the
/// branch reads `workmate/run-3fa9…` rather than `workmate/run-run_3fa9…`.
#[must_use]
pub fn branch_name(run_id: &str) -> String {
    format!("workmate/run-{}", run_id.strip_prefix("run_").unwrap_or(run_id))
}

/// Where a run's worktree lives: `<base>/<workspace id>/<run id>`.
///
/// Derived, never stored, so there is no second copy to go stale.
#[must_use]
pub fn worktree_path(base: &Path, workspace_id: &str, run_id: &str) -> PathBuf {
    base.join(workspace_id).join(run_id)
}

/// Whether `dir` is inside a git repository.
#[must_use]
pub fn is_repo(dir: &Path) -> bool {
    Repository::discover(dir).is_ok()
}

fn open(dir: &Path) -> Result<Repository, RepoError> {
    Repository::discover(dir).map_err(|_| RepoError::NotARepo(dir.to_owned()))
}

/// Worktree names are single path components; the run id already is one.
fn wt_name(run_id: &str) -> &str {
    run_id
}

/// Create the branch and worktree for `run_id`, branching from the checkout's
/// current commit.
///
/// # Errors
/// [`RepoError::NotARepo`], [`RepoError::NoCommits`] on an unborn branch,
/// [`RepoError::AlreadyExists`] if the branch or directory is taken, or a git /
/// filesystem failure.
pub fn create_worktree(checkout: &Path, path: &Path, run_id: &str) -> Result<Worktree, RepoError> {
    let repo = open(checkout)?;
    let head = repo.head().map_err(|_| RepoError::NoCommits)?.peel_to_commit()?;
    let branch = branch_name(run_id);
    if path.exists() || repo.find_branch(&branch, BranchType::Local).is_ok() {
        return Err(RepoError::AlreadyExists(run_id.to_owned()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let created = repo.branch(&branch, &head, false)?;
    let mut opts = WorktreeAddOptions::new();
    opts.reference(Some(created.get()));
    if let Err(e) = repo.worktree(wt_name(run_id), path, Some(&opts)) {
        // Leave nothing half-made: a stray branch would block a retry.
        let _ = repo.find_branch(&branch, BranchType::Local).and_then(|mut b| b.delete());
        return Err(e.into());
    }
    Ok(Worktree {
        path: path.to_owned(),
        branch,
        base: head.id().to_string(),
    })
}

fn kind_of(delta: git2::Delta) -> Option<ChangeKind> {
    match delta {
        git2::Delta::Added | git2::Delta::Untracked => Some(ChangeKind::Added),
        git2::Delta::Modified | git2::Delta::Typechange => Some(ChangeKind::Modified),
        git2::Delta::Deleted => Some(ChangeKind::Deleted),
        git2::Delta::Renamed | git2::Delta::Copied => Some(ChangeKind::Renamed),
        _ => None,
    }
}

/// Uncommitted changes in `dir`, untracked files included.
///
/// # Errors
/// [`RepoError::NotARepo`] or a git failure.
pub fn status(dir: &Path) -> Result<Vec<Change>, RepoError> {
    let repo = open(dir)?;
    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    let mut out = Vec::new();
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let s = entry.status();
        let kind = if s.is_wt_new() || s.is_index_new() {
            ChangeKind::Added
        } else if s.is_wt_deleted() || s.is_index_deleted() {
            ChangeKind::Deleted
        } else if s.is_wt_renamed() || s.is_index_renamed() {
            ChangeKind::Renamed
        } else {
            ChangeKind::Modified
        };
        out.push(Change { path: path.to_owned(), kind });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The commit a run branched from: the merge-base of the user's checkout and
/// the run's branch. Derived rather than stored, so it cannot go stale — if the
/// user's branch moves on, the base is still where the run diverged.
///
/// # Errors
/// [`RepoError::NoWorktree`] if the run's branch does not exist.
pub fn run_base(checkout: &Path, run_id: &str) -> Result<String, RepoError> {
    let repo = open(checkout)?;
    let theirs = repo
        .find_branch(&branch_name(run_id), BranchType::Local)
        .map_err(|_| RepoError::NoWorktree(run_id.to_owned()))?
        .get()
        .peel_to_commit()?
        .id();
    let ours = repo.head()?.peel_to_commit()?.id();
    Ok(repo.merge_base(ours, theirs)?.to_string())
}

/// What the run changed in `worktree` since `base`, committed or not.
///
/// # Errors
/// [`RepoError::NotARepo`], or a git failure (including an unknown `base`).
pub fn diff(worktree: &Path, base: &str) -> Result<RunDiff, RepoError> {
    let repo = open(worktree)?;
    let tree = repo.find_commit(git2::Oid::from_str(base)?)?.tree()?;
    let mut opts = DiffOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true).show_untracked_content(true);
    let mut d = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))?;
    d.find_similar(None)?;
    let mut files = Vec::new();
    for delta in d.deltas() {
        let (Some(kind), Some(path)) = (kind_of(delta.status()), delta.new_file().path()) else {
            continue;
        };
        let path = if delta.status() == git2::Delta::Deleted {
            delta.old_file().path().unwrap_or(path)
        } else {
            path
        };
        files.push(FileDiff { path: path.to_string_lossy().into_owned(), kind });
    }
    let stats = d.stats()?;
    let mut patch = String::new();
    d.print(DiffFormat::Patch, |_, _, line| {
        if matches!(line.origin(), '+' | '-' | ' ') {
            patch.push(line.origin());
        }
        patch.push_str(&String::from_utf8_lossy(line.content()));
        true
    })?;
    Ok(RunDiff {
        files,
        additions: stats.insertions(),
        deletions: stats.deletions(),
        patch,
    })
}

fn tracked_changes(repo: &Repository) -> Result<bool, RepoError> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(false).include_ignored(false);
    Ok(!repo.statuses(Some(&mut opts))?.is_empty())
}

fn signature(repo: &Repository) -> Result<Signature<'static>, RepoError> {
    Ok(repo
        .signature()
        .or_else(|_| Signature::now("workmate", "workmate@localhost"))?)
}

/// Merge a run's branch into the branch the user has checked out.
///
/// Explicit and conservative: it refuses on a dirty checkout, a detached HEAD
/// or any conflict, and in each case leaves the checkout exactly as it was.
/// Workmate never resolves a conflict on the user's behalf.
///
/// # Errors
/// [`RepoError::DirtyCheckout`], [`RepoError::DetachedHead`],
/// [`RepoError::Conflicts`], or a git failure.
pub fn merge(checkout: &Path, run_id: &str) -> Result<MergeOutcome, RepoError> {
    let repo = open(checkout)?;
    let branch = branch_name(run_id);
    if tracked_changes(&repo)? {
        return Err(RepoError::DirtyCheckout);
    }
    let head_ref = repo.head()?;
    if !head_ref.is_branch() {
        return Err(RepoError::DetachedHead);
    }
    let head_name = head_ref.name().map_err(|_| RepoError::DetachedHead)?.to_owned();
    let ours = head_ref.peel_to_commit()?;
    let theirs_ref = repo
        .find_branch(&branch, BranchType::Local)
        .map_err(|_| RepoError::NoWorktree(run_id.to_owned()))?;
    let theirs = theirs_ref.get().peel_to_commit()?;
    let annotated = repo.find_annotated_commit(theirs.id())?;
    let (analysis, _) = repo.merge_analysis(&[&annotated])?;

    if analysis.contains(MergeAnalysis::ANALYSIS_UP_TO_DATE) {
        return Ok(MergeOutcome::UpToDate);
    }
    let mut checkout_opts = git2::build::CheckoutBuilder::new();
    checkout_opts.safe();
    if analysis.contains(MergeAnalysis::ANALYSIS_FASTFORWARD) {
        repo.checkout_tree(theirs.as_object(), Some(&mut checkout_opts))?;
        repo.reference(&head_name, theirs.id(), true, &format!("workmate: fast-forward {branch}"))?;
        return Ok(MergeOutcome::FastForward);
    }
    // The merge is computed in memory first, so a conflict touches nothing.
    let mut index = repo.merge_commits(&ours, &theirs, None)?;
    if index.has_conflicts() {
        return Err(RepoError::Conflicts(branch));
    }
    let tree = repo.find_tree(index.write_tree_to(&repo)?)?;
    repo.checkout_tree(tree.as_object(), Some(&mut checkout_opts))?;
    let sig = signature(&repo)?;
    repo.commit(
        Some(&head_name),
        &sig,
        &sig,
        &format!("Merge {branch}"),
        &tree,
        &[&ours, &theirs],
    )?;
    Ok(MergeOutcome::Merged)
}

/// Remove a run's worktree directory.
///
/// Archiving keeps the branch, so the work stays recoverable and mergeable.
/// Abandoning (`delete_branch`) throws it away. Either way a worktree with
/// uncommitted changes is refused unless `force`, because that is the one thing
/// removing a directory would lose for good.
///
/// # Errors
/// [`RepoError::NoWorktree`], [`RepoError::DirtyWorktree`], or a git /
/// filesystem failure.
pub fn remove_worktree(
    checkout: &Path,
    run_id: &str,
    delete_branch: bool,
    force: bool,
) -> Result<(), RepoError> {
    let repo = open(checkout)?;
    let wt = repo
        .find_worktree(wt_name(run_id))
        .map_err(|_| RepoError::NoWorktree(run_id.to_owned()))?;
    if !force && wt.path().exists() && tracked_changes_or_new(wt.path())? {
        return Err(RepoError::DirtyWorktree);
    }
    let mut opts = WorktreePruneOptions::new();
    opts.valid(true).locked(true).working_tree(true);
    wt.prune(Some(&mut opts))?;
    if delete_branch {
        if let Ok(mut b) = repo.find_branch(&branch_name(run_id), BranchType::Local) {
            b.delete()?;
        }
    }
    Ok(())
}

fn tracked_changes_or_new(dir: &Path) -> Result<bool, RepoError> {
    Ok(!status(dir)?.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::new_id;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(new_id("repo"));
            std::fs::create_dir_all(&p).unwrap();
            // macOS /var -> /private/var; compare like with like.
            Self(std::fs::canonicalize(p).unwrap())
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn commit_all(repo: &Repository, msg: &str) {
        let mut idx = repo.index().unwrap();
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        idx.write().unwrap();
        let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
        let sig = Signature::now("t", "t@t").unwrap();
        let parents: Vec<_> = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
        let refs: Vec<_> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &refs).unwrap();
    }

    /// A checkout with one committed file on `main`, plus a spot for worktrees.
    fn fixture() -> (Tmp, PathBuf, PathBuf) {
        let tmp = Tmp::new();
        let checkout = tmp.0.join("project");
        std::fs::create_dir_all(&checkout).unwrap();
        let repo = Repository::init_opts(
            &checkout,
            git2::RepositoryInitOptions::new().initial_head("main"),
        )
        .unwrap();
        std::fs::write(checkout.join("a.txt"), "one\n").unwrap();
        commit_all(&repo, "init");
        let wt = tmp.0.join("wt").join("run_abc");
        (tmp, checkout, wt)
    }

    #[test]
    fn a_run_branches_into_its_own_worktree_and_leaves_the_checkout_alone() {
        let (_t, checkout, wt) = fixture();
        let made = create_worktree(&checkout, &wt, "run_abc").unwrap();
        assert_eq!(made.branch, "workmate/run-abc");
        assert!(wt.join("a.txt").exists());
        let wt_repo = Repository::open(&wt).unwrap();
        let head = wt_repo.head().unwrap();
        assert_eq!(head.shorthand().ok(), Some("workmate/run-abc"));
        let user = Repository::open(&checkout).unwrap();
        assert_eq!(user.head().unwrap().shorthand().ok(), Some("main"));
        assert!(status(&checkout).unwrap().is_empty());
    }

    #[test]
    fn a_second_worktree_for_the_same_run_is_refused() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        let again = checkout.parent().unwrap().join("wt2");
        assert!(matches!(
            create_worktree(&checkout, &again, "run_abc"),
            Err(RepoError::AlreadyExists(_))
        ));
    }

    #[test]
    fn a_folder_that_is_not_a_repo_is_a_named_error() {
        let t = Tmp::new();
        assert!(matches!(
            create_worktree(&t.0, &t.0.join("wt"), "run_x"),
            Err(RepoError::NotARepo(_))
        ));
        assert!(!is_repo(&t.0));
    }

    #[test]
    fn a_repo_with_no_commits_cannot_be_branched() {
        let t = Tmp::new();
        Repository::init(&t.0).unwrap();
        assert!(matches!(
            create_worktree(&t.0, &t.0.join("wt"), "run_x"),
            Err(RepoError::NoCommits)
        ));
    }

    #[test]
    fn status_reports_edits_additions_and_deletions_in_the_worktree() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(wt.join("a.txt"), "changed\n").unwrap();
        std::fs::write(wt.join("new.txt"), "n\n").unwrap();
        let s = status(&wt).unwrap();
        assert_eq!(
            s,
            vec![
                Change { path: "a.txt".into(), kind: ChangeKind::Modified },
                Change { path: "new.txt".into(), kind: ChangeKind::Added },
            ]
        );
        std::fs::remove_file(wt.join("a.txt")).unwrap();
        assert!(status(&wt).unwrap().contains(&Change {
            path: "a.txt".into(),
            kind: ChangeKind::Deleted
        }));
    }

    #[test]
    fn the_run_diff_covers_committed_and_uncommitted_work_against_the_base() {
        let (_t, checkout, wt) = fixture();
        let made = create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(wt.join("a.txt"), "one\ntwo\n").unwrap();
        commit_all(&Repository::open(&wt).unwrap(), "agent commit");
        std::fs::write(wt.join("b.txt"), "fresh\n").unwrap();

        let d = diff(&wt, &made.base).unwrap();
        let paths: Vec<_> = d.files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(paths, [("a.txt", ChangeKind::Modified), ("b.txt", ChangeKind::Added)]);
        assert_eq!((d.additions, d.deletions), (2, 0));
        assert!(d.patch.contains("+two"));
        assert!(d.patch.contains("+fresh"));
    }

    #[test]
    fn merging_a_run_that_only_moved_forward_fast_forwards() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(wt.join("a.txt"), "one\ntwo\n").unwrap();
        commit_all(&Repository::open(&wt).unwrap(), "agent commit");

        assert_eq!(merge(&checkout, "run_abc").unwrap(), MergeOutcome::FastForward);
        assert_eq!(std::fs::read_to_string(checkout.join("a.txt")).unwrap(), "one\ntwo\n");
        assert_eq!(merge(&checkout, "run_abc").unwrap(), MergeOutcome::UpToDate);
    }

    #[test]
    fn merging_after_the_user_moved_on_makes_a_merge_commit() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(wt.join("agent.txt"), "a\n").unwrap();
        commit_all(&Repository::open(&wt).unwrap(), "agent commit");
        std::fs::write(checkout.join("user.txt"), "u\n").unwrap();
        commit_all(&Repository::open(&checkout).unwrap(), "user commit");

        assert_eq!(merge(&checkout, "run_abc").unwrap(), MergeOutcome::Merged);
        assert!(checkout.join("agent.txt").exists() && checkout.join("user.txt").exists());
        let r = Repository::open(&checkout).unwrap();
        let head = r.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.parent_count(), 2);
    }

    #[test]
    fn a_conflicting_merge_is_refused_and_changes_nothing() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(wt.join("a.txt"), "agent\n").unwrap();
        commit_all(&Repository::open(&wt).unwrap(), "agent commit");
        std::fs::write(checkout.join("a.txt"), "user\n").unwrap();
        commit_all(&Repository::open(&checkout).unwrap(), "user commit");

        assert!(matches!(merge(&checkout, "run_abc"), Err(RepoError::Conflicts(_))));
        assert_eq!(std::fs::read_to_string(checkout.join("a.txt")).unwrap(), "user\n");
        assert!(status(&checkout).unwrap().is_empty());
    }

    #[test]
    fn a_dirty_checkout_blocks_the_merge() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(wt.join("b.txt"), "b\n").unwrap();
        commit_all(&Repository::open(&wt).unwrap(), "agent commit");
        std::fs::write(checkout.join("a.txt"), "dirty\n").unwrap();

        assert!(matches!(merge(&checkout, "run_abc"), Err(RepoError::DirtyCheckout)));
        assert_eq!(std::fs::read_to_string(checkout.join("a.txt")).unwrap(), "dirty\n");
    }

    #[test]
    fn archiving_removes_the_directory_but_keeps_the_branch() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        remove_worktree(&checkout, "run_abc", false, false).unwrap();
        assert!(!wt.exists());
        let repo = Repository::open(&checkout).unwrap();
        assert!(repo.find_branch("workmate/run-abc", BranchType::Local).is_ok());
    }

    #[test]
    fn abandoning_deletes_the_branch_too() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        remove_worktree(&checkout, "run_abc", true, false).unwrap();
        let repo = Repository::open(&checkout).unwrap();
        assert!(repo.find_branch("workmate/run-abc", BranchType::Local).is_err());
    }

    #[test]
    fn uncommitted_work_is_not_thrown_away_without_force() {
        let (_t, checkout, wt) = fixture();
        create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(wt.join("scratch.txt"), "precious\n").unwrap();

        assert!(matches!(
            remove_worktree(&checkout, "run_abc", true, false),
            Err(RepoError::DirtyWorktree)
        ));
        assert!(wt.join("scratch.txt").exists());
        remove_worktree(&checkout, "run_abc", true, true).unwrap();
        assert!(!wt.exists());
    }

    #[test]
    fn removing_an_unknown_worktree_is_a_named_error() {
        let (_t, checkout, _wt) = fixture();
        assert!(matches!(
            remove_worktree(&checkout, "run_nope", false, false),
            Err(RepoError::NoWorktree(_))
        ));
    }

    #[test]
    fn the_worktree_lives_outside_the_users_checkout() {
        let p = worktree_path(Path::new("/data/worktrees"), "ws_1", "run_2");
        assert_eq!(p, Path::new("/data/worktrees/ws_1/run_2"));
    }

    #[test]
    fn the_base_is_where_the_run_diverged_even_after_the_user_moves_on() {
        let (_t, checkout, wt) = fixture();
        let made = create_worktree(&checkout, &wt, "run_abc").unwrap();
        std::fs::write(checkout.join("later.txt"), "u\n").unwrap();
        commit_all(&Repository::open(&checkout).unwrap(), "user moves on");
        assert_eq!(run_base(&checkout, "run_abc").unwrap(), made.base);
        assert!(matches!(run_base(&checkout, "run_nope"), Err(RepoError::NoWorktree(_))));
    }
}
