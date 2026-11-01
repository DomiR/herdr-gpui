//! A checkout read from Git's own files, without starting Git: which checkout
//! holds a directory, the repository it belongs to, the branch it has out,
//! and where a repository has a branch checked out. A few small reads cost
//! far less than a Git process, which counts when every listed worktree is
//! looked at on a schedule. A layout these readers do not know, such as a
//! reftable repository's or a bare one's, is `None`, for the caller to ask
//! Git instead; nothing here decides an answer Git might give differently.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

/// The most a file read here holds: `HEAD`, `.git`, `commondir`, and a
/// registry entry's `gitdir` are each one line.
const FILE_LIMIT: u64 = 4096;
/// The most directories walked up from the one asked about.
const DEPTH_LIMIT: usize = 64;
/// The most linked worktrees a registry is read for.
const WORKTREE_LIMIT: usize = 4096;

/// A checkout as its Git files describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkout {
    /// The checkout's top level, canonical: the directory holding its `.git`.
    pub root: PathBuf,
    /// The repository's shared Git directory, canonical: the main checkout's
    /// `.git`, which a linked worktree names in its `commondir`.
    pub common: PathBuf,
    /// The branch `HEAD` names; `None` while detached.
    pub branch: Option<String>,
}

/// Where a repository has a branch checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    One(PathBuf),
    Missing,
    /// Checked out in several worktrees at once, as `--force` allows.
    Many,
}

enum Head {
    Branch(String),
    Detached,
}

/// The checkout holding `dir`: the nearest `.git` at or above it.
pub fn checkout_at(dir: impl AsRef<Path>) -> Option<Checkout> {
    let dir = dir.as_ref().canonicalize().ok()?;
    let mut root = dir.as_path();
    for _ in 0..DEPTH_LIMIT {
        let dot_git = root.join(".git");
        if fs::symlink_metadata(&dot_git).is_ok() {
            let git_dir = git_dir(&dot_git, root)?;
            let branch = match head(&git_dir)? {
                Head::Branch(branch) => Some(branch),
                Head::Detached => None,
            };
            return Some(Checkout {
                root: root.to_owned(),
                common: common_dir(&git_dir)?,
                branch,
            });
        }
        root = root.parent()?;
    }
    None
}

/// Where the repository whose shared Git directory is `common` has `branch`
/// checked out: its main checkout, when `common` is `<root>/.git`, and every
/// linked worktree its registry lists. Paths are as the registry keeps them,
/// as `git worktree list` prints them.
pub fn find_branch(common: impl AsRef<Path>, branch: &str) -> Option<Found> {
    let common = common.as_ref().canonicalize().ok()?;
    let mut found = Vec::new();
    // A Git directory not named `.git` (bare, or kept apart from its
    // checkout) does not say where its main checkout is.
    let root = common
        .parent()
        .filter(|_| common.file_name().is_some_and(|name| name == ".git"))?;
    if let Head::Branch(name) = head(&common)?
        && name == branch
    {
        found.push(root.to_owned());
    }
    match fs::read_dir(common.join("worktrees")) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                if index == WORKTREE_LIMIT {
                    return None;
                }
                let entry = entry.ok()?.path();
                if !entry.is_dir() {
                    continue;
                }
                let Head::Branch(name) = head(&entry)? else {
                    continue;
                };
                if name != branch {
                    continue;
                }
                // The checkout's `.git` file, absolute as Git writes it.
                let dot_git = PathBuf::from(line(&entry.join("gitdir"))?);
                if !dot_git.is_absolute() {
                    return None;
                }
                found.push(dot_git.parent()?.to_owned());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return None,
    }
    Some(match found.len() {
        0 => Found::Missing,
        1 => Found::One(found.remove(0)),
        _ => Found::Many,
    })
}

/// A one-line file without its line ending.
fn line(path: &Path) -> Option<String> {
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(FILE_LIMIT)
        .read_to_string(&mut text)
        .ok()?;
    Some(text.trim_end_matches(['\r', '\n']).to_owned())
}

/// The Git directory a checkout's `.git` names: the directory itself, or
/// the one a linked worktree's `gitdir:` line points at, which may be
/// relative to the checkout.
fn git_dir(dot_git: &Path, root: &Path) -> Option<PathBuf> {
    if fs::metadata(dot_git).ok()?.is_dir() {
        return Some(dot_git.to_owned());
    }
    let text = line(dot_git)?;
    Some(root.join(text.strip_prefix("gitdir: ")?.trim()))
}

/// The shared Git directory `git_dir` belongs to: the one its `commondir`
/// names, relative to it, or itself for a main checkout.
fn common_dir(git_dir: &Path) -> Option<PathBuf> {
    let commondir = git_dir.join("commondir");
    let common = if fs::symlink_metadata(&commondir).is_ok() {
        git_dir.join(line(&commondir)?.trim())
    } else {
        git_dir.to_owned()
    };
    common.canonicalize().ok()
}

/// What `HEAD` in `git_dir` says, or `None` for anything but a local branch
/// or a commit, including reftable's placeholder.
fn head(git_dir: &Path) -> Option<Head> {
    let text = line(&git_dir.join("HEAD"))?;
    match text.strip_prefix("ref: ") {
        Some(reference) => {
            let branch = reference.trim().strip_prefix("refs/heads/")?;
            let valid = !branch.is_empty()
                && branch != ".invalid"
                && branch.len() <= 1024
                && !branch.chars().any(char::is_control);
            valid.then(|| Head::Branch(branch.to_owned()))
        }
        None => (text.len() >= 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then_some(Head::Detached),
    }
}
