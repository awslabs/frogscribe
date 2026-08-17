// SPDX-License-Identifier: Apache-2.0
//! Integrity verification for models downloaded from Hugging Face.
//!
//! Hugging Face publishes, over TLS to `huggingface.co`, an authoritative
//! content digest for every file in a repo via its metadata API:
//!
//!   * Git-LFS tracked files (all the large model weights) expose the SHA256 of
//!     the file content as the LFS object id (`lfs.oid`).
//!   * Small, non-LFS files (e.g. `tokenizer.json`) expose the Git blob id
//!     (`oid`), which is `SHA1("blob " + len + "\0" + content)`.
//!
//! We fetch the expected digest before downloading and verify the bytes we
//! actually wrote to disk against it. This is defense-in-depth against
//! tampering/MITM on the file CDN (or a poisoned cache/proxy): altering the
//! downloaded bytes without also forging the TLS-protected API response is
//! detected here, and the offending file is removed.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// The kind of digest Hugging Face publishes for a file.
#[derive(Debug, Clone)]
pub enum ExpectedDigest {
    /// SHA256 of the raw file content (Git-LFS object id), lowercase hex.
    Sha256(String),
    /// Git blob id: `SHA1("blob " + len + "\0" + content)`, lowercase hex.
    GitBlobSha1(String),
}

/// Expected integrity metadata for a single file, from the Hugging Face API.
#[derive(Debug, Clone)]
pub struct ExpectedIntegrity {
    pub digest: ExpectedDigest,
    /// Expected size in bytes.
    pub size: u64,
}

#[derive(Deserialize)]
struct PathInfo {
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    oid: Option<String>,
    #[serde(default)]
    lfs: Option<LfsInfo>,
}

#[derive(Deserialize)]
struct LfsInfo {
    oid: String,
    size: u64,
}

/// Parse a Hugging Face `resolve` URL into `(repo_id, revision, file_path)`.
///
/// e.g. `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin`
/// -> `("ggerganov/whisper.cpp", "main", "ggml-base.bin")`
pub fn parse_hf_url(url: &str) -> Option<(String, String, String)> {
    let rest = url
        .strip_prefix("https://huggingface.co/")
        .or_else(|| url.strip_prefix("https://hf.co/"))?;
    let (repo, tail) = rest.split_once("/resolve/")?;
    let (revision, path) = tail.split_once('/')?;
    if repo.is_empty() || revision.is_empty() || path.is_empty() {
        return None;
    }
    Some((repo.to_string(), revision.to_string(), path.to_string()))
}

/// Fetch the expected digest + size for a file from the Hugging Face
/// `paths-info` API, pinned to the given revision.
pub async fn fetch_expected_integrity(
    client: &reqwest::Client,
    repo: &str,
    revision: &str,
    file_path: &str,
) -> Result<ExpectedIntegrity> {
    let api_url = format!(
        "https://huggingface.co/api/models/{}/paths-info/{}",
        repo, revision
    );
    let resp = client
        .post(&api_url)
        .form(&[("paths", file_path)])
        .send()
        .await
        .context("Failed to query Hugging Face paths-info API for checksum")?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Hugging Face paths-info API returned HTTP {} while fetching checksum for '{}'",
            resp.status(),
            file_path
        );
    }

    let infos: Vec<PathInfo> = resp
        .json()
        .await
        .context("Failed to parse Hugging Face paths-info response")?;

    let info = infos
        .into_iter()
        .find(|i| i.path == file_path)
        .with_context(|| {
            format!("File '{}' not found in Hugging Face repo '{}'", file_path, repo)
        })?;

    if let Some(lfs) = info.lfs {
        Ok(ExpectedIntegrity {
            digest: ExpectedDigest::Sha256(lfs.oid.to_lowercase()),
            size: lfs.size,
        })
    } else if let Some(oid) = info.oid {
        // Non-LFS file: the git blob id is a verifiable content digest.
        Ok(ExpectedIntegrity {
            digest: ExpectedDigest::GitBlobSha1(oid.to_lowercase()),
            size: info.size,
        })
    } else {
        anyhow::bail!(
            "Hugging Face published no checksum for '{}' in '{}'; refusing to install an unverifiable model",
            file_path,
            repo
        )
    }
}

/// Convenience wrapper: derive repo/revision/path from a `resolve` URL and
/// fetch the expected integrity metadata.
pub async fn fetch_expected_for_url(
    client: &reqwest::Client,
    url: &str,
) -> Result<ExpectedIntegrity> {
    let (repo, revision, file_path) = parse_hf_url(url)
        .with_context(|| format!("Cannot parse Hugging Face URL for verification: {}", url))?;
    fetch_expected_integrity(client, &repo, &revision, &file_path).await
}

/// Stream a file from disk to compute its SHA256 (constant memory).
pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Cannot open {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Stream a file from disk to compute its Git blob id:
/// `SHA1("blob " + size + "\0" + content)` (constant memory).
pub fn git_blob_sha1_file(path: &Path, size: u64) -> Result<String> {
    use sha1::{Digest, Sha1};
    use std::io::Read;
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Cannot open {} for hashing", path.display()))?;
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", size).as_bytes());
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Fail the verification: delete the offending file and return an error.
fn fail(path: &Path, msg: String) -> Result<()> {
    let _ = std::fs::remove_file(path);
    Err(anyhow::anyhow!(msg))
}

/// Verify a file on disk against expected integrity metadata, hashing it with
/// whichever algorithm Hugging Face published. On any mismatch the file is
/// deleted and an error is returned.
pub fn verify_file(path: &Path, expected: &ExpectedIntegrity) -> Result<()> {
    let actual_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if actual_size != expected.size {
        return fail(
            path,
            format!(
                "Size mismatch for {}: expected {} bytes, got {}. The download may have been \
                 tampered with or truncated; the file has been removed.",
                path.display(),
                expected.size,
                actual_size
            ),
        );
    }
    match &expected.digest {
        ExpectedDigest::Sha256(want) => {
            let actual = sha256_file(path)?;
            check_digest(path, "SHA256", &actual, want)
        }
        ExpectedDigest::GitBlobSha1(want) => {
            let actual = git_blob_sha1_file(path, expected.size)?;
            check_digest(path, "git blob SHA1", &actual, want)
        }
    }
}

/// Verify a file when its SHA256 was already computed during download (avoids a
/// second pass over large files). If Hugging Face published a non-SHA256 digest
/// for this file, the correct hash is recomputed from disk instead.
pub fn verify_downloaded_sha256(
    path: &Path,
    precomputed_sha256: &str,
    expected: &ExpectedIntegrity,
) -> Result<()> {
    let actual_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if actual_size != expected.size {
        return fail(
            path,
            format!(
                "Size mismatch for {}: expected {} bytes, got {}. The download may have been \
                 tampered with or truncated; the file has been removed.",
                path.display(),
                expected.size,
                actual_size
            ),
        );
    }
    match &expected.digest {
        ExpectedDigest::Sha256(want) => check_digest(path, "SHA256", precomputed_sha256, want),
        // Different algorithm than we streamed; recompute the right one.
        ExpectedDigest::GitBlobSha1(want) => {
            let actual = git_blob_sha1_file(path, expected.size)?;
            check_digest(path, "git blob SHA1", &actual, want)
        }
    }
}

fn check_digest(path: &Path, algo: &str, actual: &str, want: &str) -> Result<()> {
    if actual.eq_ignore_ascii_case(want) {
        Ok(())
    } else {
        fail(
            path,
            format!(
                "Integrity check FAILED for {}: expected {} {}, got {}. The download may have \
                 been tampered with (possible MITM) or corrupted; the file has been removed.",
                path.display(),
                algo,
                want,
                actual
            ),
        )
    }
}
