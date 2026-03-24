use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use chrono::Utc;
use sha2::Digest;

use crate::models::{ArtifactPayloadBody, ArtifactRecord};

const MANIFEST_EXT: &str = ".json";

pub fn artifacts_dir(home: &Path) -> PathBuf {
    home.join("artifacts")
}

fn manifests_dir(home: &Path) -> PathBuf {
    artifacts_dir(home).join("manifests")
}

fn blobs_dir(home: &Path) -> PathBuf {
    artifacts_dir(home).join("blobs")
}

pub fn ensure_dirs(home: &Path) -> Result<()> {
    fs::create_dir_all(manifests_dir(home))?;
    fs::create_dir_all(blobs_dir(home))?;
    Ok(())
}

pub fn list_artifacts(home: &Path) -> Result<Vec<ArtifactRecord>> {
    ensure_dirs(home)?;
    let mut items = Vec::new();
    for entry in fs::read_dir(manifests_dir(home))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let record = serde_json::from_str::<ArtifactRecord>(&raw)
            .with_context(|| format!("parse artifact manifest {}", path.display()))?;
        items.push(record);
    }
    items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(items)
}

pub fn store_artifact_from_path(
    home: &Path,
    source_path: &Path,
    name: Option<&str>,
    mime_type: Option<&str>,
    direction: &str,
    peer_id: Option<&str>,
    note: Option<&str>,
) -> Result<ArtifactRecord> {
    ensure_dirs(home)?;
    let bytes = fs::read(source_path)
        .with_context(|| format!("read artifact source {}", source_path.display()))?;
    let name = name
        .map(ToString::to_string)
        .or_else(|| {
            source_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(ToString::to_string)
        })
        .ok_or_else(|| anyhow!("artifact source has no file name"))?;
    let mime_type = mime_type.unwrap_or_else(|| infer_mime(&name));
    store_artifact_bytes(home, &bytes, &name, mime_type, direction, peer_id, note)
}

pub fn store_artifact_payload(
    home: &Path,
    payload: &ArtifactPayloadBody,
    direction: &str,
    peer_id: Option<&str>,
) -> Result<ArtifactRecord> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&payload.content_base64)
        .context("decode artifact payload")?;
    let sha256 = hex::encode(sha2::Sha256::digest(&bytes));
    if sha256 != payload.sha256 {
        bail!("artifact payload sha256 mismatch");
    }
    if payload.size_bytes != bytes.len() as u64 {
        bail!("artifact payload size mismatch");
    }
    store_artifact_bytes_with_id(
        home,
        &payload.artifact_id,
        &bytes,
        &payload.name,
        &payload.mime_type,
        direction,
        peer_id,
        payload.note.as_deref(),
    )
}

pub fn load_artifact_bytes(home: &Path, artifact_id: &str) -> Result<(ArtifactRecord, Vec<u8>)> {
    let record = load_artifact_record(home, artifact_id)?;
    let bytes = fs::read(&record.saved_path)
        .with_context(|| format!("read artifact blob {}", record.saved_path))?;
    Ok((record, bytes))
}

pub fn load_artifact_record(home: &Path, artifact_id: &str) -> Result<ArtifactRecord> {
    ensure_dirs(home)?;
    let path = manifests_dir(home).join(format!("{artifact_id}{MANIFEST_EXT}"));
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read artifact manifest {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("parse artifact manifest {}", path.display()))
}

fn store_artifact_bytes(
    home: &Path,
    bytes: &[u8],
    name: &str,
    mime_type: &str,
    direction: &str,
    peer_id: Option<&str>,
    note: Option<&str>,
) -> Result<ArtifactRecord> {
    let artifact_id = uuid::Uuid::new_v4().simple().to_string();
    store_artifact_bytes_with_id(
        home,
        &artifact_id,
        bytes,
        name,
        mime_type,
        direction,
        peer_id,
        note,
    )
}

fn store_artifact_bytes_with_id(
    home: &Path,
    artifact_id: &str,
    bytes: &[u8],
    name: &str,
    mime_type: &str,
    direction: &str,
    peer_id: Option<&str>,
    note: Option<&str>,
) -> Result<ArtifactRecord> {
    ensure_dirs(home)?;
    let safe_name = sanitize_filename(name);
    let blob_path = blobs_dir(home).join(format!("{artifact_id}-{safe_name}"));
    fs::write(&blob_path, bytes)
        .with_context(|| format!("write artifact blob {}", blob_path.display()))?;
    let record = ArtifactRecord {
        artifact_id: artifact_id.to_string(),
        name: name.to_string(),
        mime_type: mime_type.to_string(),
        size_bytes: bytes.len() as u64,
        sha256: hex::encode(sha2::Sha256::digest(bytes)),
        direction: direction.to_string(),
        peer_id: peer_id.map(ToString::to_string),
        note: note.map(ToString::to_string),
        saved_path: blob_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
    };
    persist_manifest(home, &record)?;
    Ok(record)
}

fn persist_manifest(home: &Path, record: &ArtifactRecord) -> Result<()> {
    ensure_dirs(home)?;
    let path = manifests_dir(home).join(format!("{}{}", record.artifact_id, MANIFEST_EXT));
    fs::write(&path, serde_json::to_string_pretty(record)?)
        .with_context(|| format!("write artifact manifest {}", path.display()))?;
    Ok(())
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

fn infer_mime(name: &str) -> &str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => "application/json",
        "md" => "text/markdown",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "pdf" => "application/pdf",
        "html" => "text/html",
        _ => "application/octet-stream",
    }
}
