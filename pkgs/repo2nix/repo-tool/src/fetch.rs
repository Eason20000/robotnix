use serde::Deserialize;
use serde_json;
use std::io;
use std::path::PathBuf;
use thiserror::Error;
use tokio::process::Command;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct NixPrefetchGitOutput {
    #[allow(dead_code)]
    pub url: String,

    pub rev: String,

    pub date: u64,

    pub path: PathBuf,

    #[allow(dead_code)]
    pub sha256: String,

    pub hash: String,

    #[allow(dead_code)]
    #[serde(rename = "fetchLFS")]
    pub fetch_lfs: bool,

    #[allow(dead_code)]
    #[serde(rename = "fetchSubmodules")]
    pub fetch_submodules: bool,

    #[allow(dead_code)]
    #[serde(rename = "deepClone")]
    pub deep_clone: bool,

    #[allow(dead_code)]
    #[serde(rename = "leaveDotGit")]
    pub leave_dot_git: bool,
}

#[derive(Debug, Error)]
pub enum NixPrefetchGitError {
    #[error("couldn't spawn `nix-prefetch-git` process")]
    ProcessSpawn(#[from] io::Error),
    #[error("`nix-prefetch-git` did not return successfully ({0:?}), stderr:\n{1}")]
    NonzeroExitStatus(Option<i32>, String),
    #[error("couldn't parse `nix-prefetch-git` output")]
    Parse(#[from] serde_json::Error),
}

/// Extra `nix-prefetch-git` flags for a fetch.
/// `shallow` maps to `--no-deepClone` ("shallow clone of just the required
/// ref"). The resulting tree (and therefore hash) is identical to a full
/// clone since `.git` is stripped either way; only transfer and temp peak
/// shrink.
fn extra_args(fetch_lfs: bool, fetch_submodules: bool, shallow: bool) -> Vec<&'static str> {
    let mut flag_args = vec![];
    if fetch_lfs {
        flag_args.push("--fetch-lfs")
    }
    if fetch_submodules {
        flag_args.push("--fetch-submodules")
    }
    if shallow {
        flag_args.push("--no-deepClone")
    }
    flag_args
}

pub async fn nix_prefetch_git(
    repo_url: &Url,
    revision: &str,
    fetch_lfs: bool,
    fetch_submodules: bool,
    shallow: bool,
) -> Result<NixPrefetchGitOutput, NixPrefetchGitError> {
    eprintln!("Prefetching `{}`, revision {}...", repo_url, revision);
    let output = Command::new("nix-prefetch-git")
        .arg("--url")
        .arg(repo_url.as_str())
        .arg("--rev")
        .arg(&revision)
        .args(&extra_args(fetch_lfs, fetch_submodules, shallow))
        .output()
        .await?;

    if !output.status.success() {
        return Err(NixPrefetchGitError::NonzeroExitStatus(
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

#[derive(Debug, Error)]
pub enum GitLsRemoteError {
    #[error("couldn't spawn `git ls-remote` process")]
    ProcessSpawn(#[from] io::Error),
    #[error("`git ls-remote` did not return successfully ({0:?}), stderr:\n{1}")]
    NonzeroExitStatus(Option<i32>, String),
    #[error("`git ls-remote` returned invalid UTF-8")]
    Utf8(std::str::Utf8Error),
    #[error("rev not found at remote")]
    RevNotFound,
    #[error("error parsing output")]
    Parse,
}

pub async fn git_ls_remote(url: &str, git_ref: &str) -> Result<String, GitLsRemoteError> {
    let output = Command::new("git")
        .arg("ls-remote")
        .arg(url)
        .output()
        .await?;

    if !output.status.success() {
        return Err(GitLsRemoteError::NonzeroExitStatus(
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let output_str = std::str::from_utf8(&output.stdout).map_err(GitLsRemoteError::Utf8)?;
    for line in output_str.split("\n") {
        if line.ends_with(git_ref) {
            let commit = line.split("\t").next().unwrap();
            return Ok(commit.to_string());
        }
    }

    Err(GitLsRemoteError::RevNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shallow_adds_no_deep_clone_flag() {
        assert_eq!(extra_args(false, false, false), Vec::<&str>::new());
        assert_eq!(extra_args(true, false, false), vec!["--fetch-lfs"]);
        assert_eq!(
            extra_args(false, true, false),
            vec!["--fetch-submodules"]
        );
        assert_eq!(extra_args(false, false, true), vec!["--no-deepClone"]);
        assert_eq!(
            extra_args(true, true, true),
            vec!["--fetch-lfs", "--fetch-submodules", "--no-deepClone"]
        );
    }
}
