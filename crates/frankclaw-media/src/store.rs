use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::debug;

use frankclaw_core::error::{FrankClawError, Result};
use frankclaw_core::media::{MediaFile, mime_for_safe_extension, safe_extension_for_mime};
use frankclaw_core::types::MediaId;

/// File-based media store with TTL cleanup.
///
/// Files are stored with owner-only permissions (0o600).
/// Each file gets a UUID to prevent enumeration attacks.
pub struct MediaStore {
    base_dir: PathBuf,
    max_file_size: u64,
    ttl_hours: u64,
}

pub struct StoredMediaContent {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MediaMetadata {
    original_name: String,
    mime_type: String,
}

impl MediaStore {
    pub fn new(base_dir: PathBuf, max_file_size: u64, ttl_hours: u64) -> Result<Self> {
        std::fs::create_dir_all(&base_dir).map_err(|e| FrankClawError::Internal {
            msg: format!("failed to create media directory: {e}"),
        })?;

        // Set directory permissions to owner-only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            let _ = std::fs::set_permissions(&base_dir, perms);
        }

        Ok(Self {
            base_dir,
            max_file_size,
            ttl_hours,
        })
    }

    /// Store bytes as a media file. Returns metadata.
    pub fn store(
        &self,
        original_name: &str,
        mime_type: &str,
        data: &[u8],
    ) -> Result<MediaFile> {
        if data.len() as u64 > self.max_file_size {
            return Err(FrankClawError::MediaTooLarge {
                max_bytes: self.max_file_size,
            });
        }

        let id = MediaId::new();
        let ext = safe_extension_for_mime(mime_type);
        let filename = format!("{id}.{ext}");
        let path = self.base_dir.join(&filename);
        let metadata_path = metadata_path_for(&path);
        let sanitized_name = sanitize_filename(original_name);

        std::fs::write(&path, data).map_err(|e| FrankClawError::Internal {
            msg: format!("failed to write media file: {e}"),
        })?;
        write_metadata(
            &metadata_path,
            &MediaMetadata {
                original_name: sanitized_name.clone(),
                mime_type: mime_type.to_string(),
            },
        )?;

        // Set file permissions to owner-only (NOT 0o644 like OpenClaw).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms.clone());
            let _ = std::fs::set_permissions(&metadata_path, perms);
        }

        let now = Utc::now();
        Ok(MediaFile {
            id,
            original_name: sanitized_name,
            mime_type: mime_type.to_string(),
            size_bytes: data.len() as u64,
            path,
            created_at: now,
            expires_at: now + chrono::Duration::hours(self.ttl_hours as i64),
        })
    }

    /// Delete expired media files.
    pub fn cleanup(&self) -> Result<u64> {
        let mut deleted = 0u64;
        let _now = Utc::now();

        let entries = std::fs::read_dir(&self.base_dir).map_err(|e| FrankClawError::Internal {
            msg: format!("failed to read media directory: {e}"),
        })?;

        for entry in entries.flatten() {
            if is_metadata_path(&entry.path()) {
                continue;
            }
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let age = std::time::SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or_default();
                    if age > std::time::Duration::from_secs(self.ttl_hours * 3600) {
                        if std::fs::remove_file(entry.path()).is_ok() {
                            let _ = std::fs::remove_file(metadata_path_for(&entry.path()));
                            deleted += 1;
                        }
                    }
                }
            }
        }

        if deleted > 0 {
            debug!(deleted, "cleaned up expired media files");
        }

        Ok(deleted)
    }

    pub fn read(
        &self,
        id: &MediaId,
    ) -> Result<Option<StoredMediaContent>> {
        let Some(path) = self.resolve_path(id)? else {
            return Ok(None);
        };
        let bytes = std::fs::read(&path).map_err(|e| FrankClawError::Internal {
            msg: format!("failed to read media file: {e}"),
        })?;
        let metadata = read_metadata(&metadata_path_for(&path))?;
        let filename = metadata
            .as_ref()
            .map(|value| value.original_name.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("media.bin")
                    .to_string()
            });
        let mime_type = metadata
            .as_ref()
            .map(|value| value.mime_type.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(mime_for_safe_extension)
                    .unwrap_or("application/octet-stream")
                    .to_string()
            });

        Ok(Some(StoredMediaContent {
            bytes,
            mime_type,
            filename,
        }))
    }

    fn resolve_path(&self, id: &MediaId) -> Result<Option<PathBuf>> {
        let prefix = id.to_string();
        let entries = std::fs::read_dir(&self.base_dir).map_err(|e| FrankClawError::Internal {
            msg: format!("failed to read media directory: {e}"),
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if is_metadata_path(&path) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if stem == prefix {
                return Ok(Some(path));
            }
        }

        Ok(None)
    }
}

fn metadata_path_for(path: &std::path::Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("media.bin");
    path.with_file_name(format!("{filename}.meta.json"))
}

fn is_metadata_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.ends_with(".meta.json"))
}

fn write_metadata(path: &std::path::Path, metadata: &MediaMetadata) -> Result<()> {
    let bytes = serde_json::to_vec(metadata).map_err(|e| FrankClawError::Internal {
        msg: format!("failed to serialize media metadata: {e}"),
    })?;
    std::fs::write(path, bytes).map_err(|e| FrankClawError::Internal {
        msg: format!("failed to write media metadata: {e}"),
    })
}

fn read_metadata(path: &std::path::Path) -> Result<Option<MediaMetadata>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| FrankClawError::Internal {
        msg: format!("failed to read media metadata: {e}"),
    })?;
    let metadata = serde_json::from_slice(&bytes).map_err(|e| FrankClawError::Internal {
        msg: format!("failed to parse media metadata: {e}"),
    })?;
    Ok(Some(metadata))
}

/// Sanitize filename to prevent path traversal.
/// Strips directory separators, leading dots, and limits length.
/// Maximum length for sanitized filenames.
/// On-disk files use UUID-based names; this limits the original_name metadata.
const MAX_FILENAME_LEN: usize = 60;

fn sanitize_filename(name: &str) -> String {
    // Take only the filename component (strip any directory path).
    let basename = name.rsplit(&['/', '\\']).next().unwrap_or(name);
    // Allow only safe characters.
    let cleaned: String = basename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .take(MAX_FILENAME_LEN)
        .collect();
    // Strip leading dots to prevent hidden files / traversal.
    let result = cleaned.trim_start_matches('.').to_string();
    // If nothing remains after sanitization, use a safe default.
    if result.is_empty() {
        "unnamed".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_traversal() {
        assert_eq!(sanitize_filename("../../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("normal-file.txt"), "normal-file.txt");
        assert_eq!(sanitize_filename("file with spaces.png"), "filewithspaces.png");
    }

    #[test]
    fn sanitize_limits_filename_length() {
        let long_name = "a".repeat(200) + ".txt";
        let result = sanitize_filename(&long_name);
        assert!(result.len() <= MAX_FILENAME_LEN);
    }

    #[test]
    fn sanitize_handles_dots_only_filename() {
        assert_eq!(sanitize_filename("..."), "unnamed");
        assert_eq!(sanitize_filename("."), "unnamed");
        assert_eq!(sanitize_filename("..hidden"), "hidden");
    }

    #[test]
    fn sanitize_handles_empty_and_special_chars() {
        assert_eq!(sanitize_filename(""), "unnamed");
        assert_eq!(sanitize_filename("   "), "unnamed");
        assert_eq!(sanitize_filename("!@#$%^&*()"), "unnamed");
    }

    #[test]
    fn safe_extensions() {
        assert_eq!(safe_extension_for_mime("application/x-executable"), "bin");
        assert_eq!(safe_extension_for_mime("application/x-sh"), "bin");
        assert_eq!(safe_extension_for_mime("image/png"), "png");
        assert_eq!(safe_extension_for_mime("audio/mp4"), "m4a");
    }

    #[test]
    fn read_returns_bytes_and_inferred_mime() {
        let temp_dir = std::env::temp_dir().join(format!(
            "frankclaw-media-read-{}",
            uuid::Uuid::new_v4()
        ));
        let store = MediaStore::new(temp_dir.clone(), 1024, 1).expect("store should create");
        let media = store
            .store("note.txt", "text/plain", b"hello")
            .expect("media should store");

        let loaded = store
            .read(&media.id)
            .expect("media read should succeed")
            .expect("media should exist");
        assert_eq!(loaded.bytes, b"hello");
        assert_eq!(loaded.mime_type, "text/plain");
        assert_eq!(loaded.filename, "note.txt");

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn read_falls_back_when_metadata_sidecar_is_missing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "frankclaw-media-fallback-{}",
            uuid::Uuid::new_v4()
        ));
        let store = MediaStore::new(temp_dir.clone(), 1024, 1).expect("store should create");
        let media = store
            .store("note.txt", "text/plain", b"hello")
            .expect("media should store");
        let metadata_path = metadata_path_for(&media.path);
        std::fs::remove_file(&metadata_path).expect("metadata should delete");

        let loaded = store
            .read(&media.id)
            .expect("media read should succeed")
            .expect("media should exist");
        assert_eq!(loaded.bytes, b"hello");
        assert_eq!(loaded.mime_type, "text/plain; charset=utf-8");
        assert!(loaded.filename.ends_with(".txt"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn cleanup_removes_sidecar_metadata_with_media_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "frankclaw-media-cleanup-{}",
            uuid::Uuid::new_v4()
        ));
        let store = MediaStore::new(temp_dir.clone(), 1024, 0).expect("store should create");
        let media = store
            .store("note.txt", "text/plain", b"hello")
            .expect("media should store");
        let metadata_path = metadata_path_for(&media.path);

        assert!(media.path.exists());
        assert!(metadata_path.exists());

        let deleted = store.cleanup().expect("cleanup should succeed");
        assert_eq!(deleted, 1);
        assert!(!media.path.exists());
        assert!(!metadata_path.exists());

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
