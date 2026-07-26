//! GCS-based remote index sync for session search.
//!
//! On bootstrap completion, compresses `session_search.sqlite` with zstd
//! and uploads to GCS (async, fire-and-forget, debounced to at most once
//! per hour). On startup, if the local index is stale (missing, no
//! `last_bootstrap_at`, or `last_bootstrap_at` > 1 hour older than remote),
//! downloads and decompresses the remote index before running incremental
//! bootstrap.
//!
//! Gated behind `RemoteSyncConfig::enabled` (default false).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use super::search_fts::SessionSearchIndex;
use crate::auth::{AuthManager, GrokComConfig};
use crate::session::repo_changes::{TraceExportConfig, UploadMethod};
use xai_file_utils::storage_client::ExistsResult;

/// GCS object name for the compressed index.
const REMOTE_INDEX_OBJECT: &str = "session_search.sqlite.zst";

/// Zstd compression level (balance of speed vs ratio).
const ZSTD_COMPRESSION_LEVEL: i32 = 3;

/// Minimum interval between uploads (1 hour).
const UPLOAD_DEBOUNCE: Duration = Duration::from_secs(3600);

/// Staleness threshold: if local `last_bootstrap_at` is more than this
/// duration older than the remote object's timestamp, download the remote.
const STALENESS_THRESHOLD: Duration = Duration::from_secs(3600);

/// SQLite meta key for the last successful bootstrap timestamp (unix secs).
const META_KEY_LAST_BOOTSTRAP: &str = "last_bootstrap_at";

#[derive(Clone)]
pub struct RemoteSyncRuntime {
    pub config: RemoteSyncConfig,
    pub gcs_config: TraceExportConfig,
    pub auth_manager: Option<Arc<AuthManager>>,
}

// Configuration

/// Configuration for remote index sync.
///
/// Parsed from `[session_search.remote_sync]` in `~/.grok/config.toml`.
/// Default: disabled.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct RemoteSyncConfig {
    /// Whether remote sync is enabled.
    pub enabled: bool,
    /// GCS prefix for the remote index (directory structure in the bucket).
    /// Defaults to `"session_search_index"`.
    pub gcs_prefix: String,
}

impl Default for RemoteSyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            gcs_prefix: "session_search_index".to_string(),
        }
    }
}

// Debounce state (per target, per process)
static LAST_UPLOAD_AT_BY_TARGET: OnceLock<Mutex<std::collections::HashMap<String, i64>>> =
    OnceLock::new();

fn upload_debounce_map() -> &'static Mutex<std::collections::HashMap<String, i64>> {
    LAST_UPLOAD_AT_BY_TARGET.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn upload_target_key(config: &RemoteSyncConfig, gcs_config: &TraceExportConfig) -> String {
    let method = match &gcs_config.upload_method {
        UploadMethod::Proxy { proxy_base_url, .. } => format!("proxy:{proxy_base_url}"),
        UploadMethod::Direct { .. } => {
            format!("direct:{}", gcs_config.bucket_url.as_deref().unwrap_or(""))
        }
        UploadMethod::S3 {
            bucket,
            region,
            endpoint_url,
            ..
        } => format!(
            "s3:{bucket}:{region}:{}",
            endpoint_url.as_deref().unwrap_or_default()
        ),
    };
    format!("{method}|{}", config.gcs_prefix)
}

/// Returns true if enough time has passed since the last successful upload for this target.
fn upload_debounce_ok(target: &str) -> bool {
    let now = chrono::Utc::now().timestamp();
    let map = upload_debounce_map();
    let guard = map.lock().unwrap_or_else(|e| e.into_inner());
    let Some(last) = guard.get(target).copied() else {
        return true;
    };
    (now - last) >= UPLOAD_DEBOUNCE.as_secs() as i64
}

/// Record that an upload just completed for this target.
fn record_upload(target: &str) {
    let map = upload_debounce_map();
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    guard.insert(target.to_string(), chrono::Utc::now().timestamp());
}

// Compression / decompression

/// Compress `src` with zstd level 3, writing to `dst`.
fn compress_file(src: &Path, dst: &Path) -> io::Result<u64> {
    let input = std::fs::File::open(src)?;
    let output = std::fs::File::create(dst)?;
    let mut encoder = zstd::Encoder::new(output, ZSTD_COMPRESSION_LEVEL)?;
    let bytes = io::copy(&mut io::BufReader::new(input), &mut encoder)?;
    encoder.finish()?;
    Ok(bytes)
}

/// Decompress a zstd-compressed file from `src` to `dst`.
fn decompress_file(src: &Path, dst: &Path) -> io::Result<u64> {
    let input = std::fs::File::open(src)?;
    let mut decoder = zstd::Decoder::new(input)?;
    let output = std::fs::File::create(dst)?;
    let bytes = io::copy(&mut decoder, &mut io::BufWriter::new(output))?;
    Ok(bytes)
}

// Staleness check

/// Read `last_bootstrap_at` from the sqlite meta table.
///
/// Returns `None` if the DB doesn't exist, can't be opened, or the key
/// is missing.
pub fn read_last_bootstrap_at(db_path: &Path) -> Option<i64> {
    if !db_path.exists() {
        return None;
    }
    let index = SessionSearchIndex::open_or_create(db_path).ok()?;
    index
        .get_meta(META_KEY_LAST_BOOTSTRAP)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
}

/// Like [`read_last_bootstrap_at`] but preserves read failures, so callers
/// can tell "marker genuinely absent" apart from "could not read the DB"
/// (transient busy/locked/I/O). A missing DB file is a true absence, not an
/// error.
pub fn try_read_last_bootstrap_at(db_path: &Path) -> Result<Option<i64>, String> {
    if !db_path.exists() {
        return Ok(None);
    }
    let index = SessionSearchIndex::open_or_create(db_path).map_err(|e| e.to_string())?;
    let value = index
        .get_meta(META_KEY_LAST_BOOTSTRAP)
        .map_err(|e| e.to_string())?;
    Ok(value.and_then(|v| v.parse::<i64>().ok()))
}

/// Write `last_bootstrap_at` into the sqlite meta table.
pub fn write_last_bootstrap_at(db_path: &Path) -> io::Result<()> {
    let index =
        SessionSearchIndex::open_or_create(db_path).map_err(|e| io::Error::other(e.to_string()))?;
    let now = chrono::Utc::now().timestamp();
    index
        .set_meta(META_KEY_LAST_BOOTSTRAP, &now.to_string())
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Determine whether the local index is stale enough to warrant downloading
/// the remote copy.
///
/// Returns `true` if:
/// - The local DB file doesn't exist, or
/// - There is no `last_bootstrap_at` in the meta table, or
/// - `last_bootstrap_at` is more than [`STALENESS_THRESHOLD`] old compared
///   to `remote_timestamp_unix` (0 if unknown — always stale).
pub fn is_local_stale(db_path: &Path, remote_timestamp_unix: i64) -> bool {
    let Some(local_ts) = read_last_bootstrap_at(db_path) else {
        return true; // no local timestamp → stale
    };
    if remote_timestamp_unix == 0 {
        // Remote timestamp unknown; if we have a local bootstrap, trust it.
        return false;
    }
    (remote_timestamp_unix - local_ts) > STALENESS_THRESHOLD.as_secs() as i64
}

// GCS object path helpers

fn gcs_object_path(config: &RemoteSyncConfig) -> String {
    format!("{}/{}", config.gcs_prefix, REMOTE_INDEX_OBJECT)
}

pub fn resolve_runtime() -> Option<RemoteSyncRuntime> {
    let root = match crate::config::load_effective_config() {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(error = %e, "REMOTE_SYNC_DISABLED: unable to load effective config");
            return None;
        }
    };

    let remote_sync = root
        .get("session_search")
        .and_then(|v| v.get("remote_sync"))
        .and_then(|v| v.clone().try_into::<RemoteSyncConfig>().ok())
        .unwrap_or_default();
    if !remote_sync.enabled {
        tracing::debug!("REMOTE_SYNC_DISABLED: session_search.remote_sync.enabled=false");
        return None;
    }

    let endpoints = root
        .get("endpoints")
        .and_then(|v| {
            v.clone()
                .try_into::<crate::agent::config::EndpointsConfig>()
                .ok()
        })
        .unwrap_or_default();
    let auth_cfg = root
        .get("auth")
        .and_then(|v| v.clone().try_into::<GrokComConfig>().ok())
        .unwrap_or_default();

    let grok_home = crate::util::grok_home::grok_home();
    let auth_manager = Arc::new(AuthManager::new(&grok_home, auth_cfg));

    let auth_token = if endpoints.deployment_key.is_none() {
        auth_manager
            .current_or_expired()
            .filter(|auth| auth.is_xai_auth())
            .map(|auth| auth.key)
    } else {
        None
    };
    let Some(upload_method) = endpoints.resolve_upload_method(auth_token) else {
        tracing::warn!("REMOTE_SYNC_DISABLED: no upload method available for remote sync");
        return None;
    };

    let bucket_url = match &upload_method {
        UploadMethod::Direct { .. } => match endpoints.resolve_trace_bucket_url() {
            Some(resolved) => Some(resolved.value),
            None => {
                tracing::warn!(
                    "REMOTE_SYNC_DISABLED: direct upload selected but no trace bucket configured"
                );
                return None;
            }
        },
        UploadMethod::S3 { bucket, .. } => Some(format!("s3://{bucket}")),
        UploadMethod::Proxy { .. } => None,
    };

    let gcs_prefix = remote_sync.gcs_prefix.clone();
    Some(RemoteSyncRuntime {
        config: remote_sync,
        gcs_config: TraceExportConfig {
            bucket_url,
            service_account_key: None,
            prefix_dir: None,
            gcs_prefix: Some(gcs_prefix),
            absolute_paths: false,
            archive_name_override: None,
            upload_method,
        },
        auth_manager: Some(auth_manager),
    })
}

#[derive(Debug, Clone, Copy)]
struct RemoteMetadata {
    /// Epoch seconds.
    timestamp_unix: i64,
}

#[derive(Debug, Clone, Copy)]
enum RemoteProbe {
    Found(RemoteMetadata),
    NotFound,
}

fn generation_to_unix_secs(generation: i64) -> Option<i64> {
    if generation <= 0 {
        return None;
    }
    // GCS generation is typically microseconds since epoch.
    if generation > 1_000_000_000_000 {
        return Some(generation / 1_000_000);
    }
    Some(generation)
}

async fn probe_remote_metadata(
    object_path: &str,
    gcs_config: &TraceExportConfig,
    auth_manager: Option<Arc<AuthManager>>,
) -> io::Result<RemoteProbe> {
    match &gcs_config.upload_method {
        UploadMethod::Proxy {
            proxy_base_url,
            user_token,
            deployment_key,
            alpha_test_key,
        } => {
            let client = crate::auth::credential_provider::build_storage_client_for_proxy(
                proxy_base_url,
                deployment_key.clone(),
                alpha_test_key.clone(),
                auth_manager,
                Some(user_token.clone()),
                None,
                "grok-shell",
            );
            match client.check_exists(object_path).await {
                ExistsResult::Found(resp) => {
                    let Some(remote_ts) = generation_to_unix_secs(resp.generation) else {
                        return Err(io::Error::other(
                            "REMOTE_METADATA_FAILED: invalid/unknown object generation",
                        ));
                    };
                    Ok(RemoteProbe::Found(RemoteMetadata {
                        timestamp_unix: remote_ts,
                    }))
                }
                ExistsResult::NotFound => Ok(RemoteProbe::NotFound),
                ExistsResult::Unauthorized => Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "REMOTE_METADATA_FAILED: unauthorized",
                )),
                ExistsResult::ProbeFailed => Err(io::Error::other(
                    "REMOTE_METADATA_FAILED: metadata probe failed",
                )),
            }
        }
        _ => Err(io::Error::other(
            "REMOTE_METADATA_FAILED: metadata probe unsupported for this upload method",
        )),
    }
}

fn temp_sibling(path: &Path, tag: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{tag}.{}.tmp", uuid::Uuid::now_v7()));
    PathBuf::from(name)
}

async fn download_compressed_to_path(
    object_path: &str,
    gcs_config: &TraceExportConfig,
    auth_manager: Option<Arc<AuthManager>>,
    compressed_path: &Path,
) -> io::Result<()> {
    match &gcs_config.upload_method {
        UploadMethod::Proxy {
            proxy_base_url,
            user_token,
            deployment_key,
            alpha_test_key,
        } => {
            let client = crate::auth::credential_provider::build_storage_client_for_proxy(
                proxy_base_url,
                deployment_key.clone(),
                alpha_test_key.clone(),
                auth_manager,
                Some(user_token.clone()),
                None,
                "grok-shell",
            );
            client
                .download_blob(object_path, compressed_path)
                .await
                .map_err(io::Error::other)
        }
        _ => Err(io::Error::other(
            "REMOTE_DOWNLOAD_FAILED: download unsupported for this upload method",
        )),
    }
}

// Upload (fire-and-forget, debounced)

/// Compress and upload the local search index to GCS.
///
/// This is a fire-and-forget operation: errors are logged but not
/// propagated. The upload is debounced to at most once per hour.
///
/// Called after bootstrap completion when remote sync is enabled.
pub async fn maybe_upload_index(db_path: PathBuf, runtime: &RemoteSyncRuntime) -> bool {
    let config = &runtime.config;
    let gcs_config = &runtime.gcs_config;
    let auth_manager = runtime.auth_manager.clone();

    if !config.enabled {
        tracing::debug!("REMOTE_SYNC_DISABLED: upload skipped");
        return false;
    }
    let target = upload_target_key(config, gcs_config);
    if !upload_debounce_ok(&target) {
        tracing::debug!(
            target,
            "LOCAL_FALLBACK: skipping search index upload (debounce)"
        );
        return false;
    }
    if !db_path.exists() {
        tracing::debug!("LOCAL_FALLBACK: skipping search index upload (no local DB)");
        return false;
    }

    match upload_index_inner(&db_path, config, gcs_config, auth_manager).await {
        Ok(()) => {
            record_upload(&target);
            tracing::info!(target, "REMOTE_UPLOAD_SUCCESS");
            true
        }
        Err(e) => {
            tracing::warn!(error = %e, "REMOTE_UPLOAD_FAILED");
            false
        }
    }
}

async fn upload_index_inner(
    db_path: &Path,
    config: &RemoteSyncConfig,
    gcs_config: &TraceExportConfig,
    auth_manager: Option<Arc<AuthManager>>,
) -> io::Result<()> {
    let db_path = db_path.to_path_buf();
    let compressed_path = db_path.with_extension("sqlite.zst.tmp");

    // Compress on blocking thread
    let src = db_path.clone();
    let dst = compressed_path.clone();
    let original_size = tokio::task::spawn_blocking(move || -> io::Result<u64> {
        let size = compress_file(&src, &dst)?;
        Ok(size)
    })
    .await
    .map_err(io::Error::other)??;

    // Read compressed bytes
    let compressed_bytes = tokio::fs::read(&compressed_path).await?;
    let compressed_size = compressed_bytes.len() as u64;

    // Upload to GCS
    let object_path = gcs_object_path(config);
    let upload_config = crate::upload::gcs::WithAuth::with_auth(gcs_config, auth_manager);
    match xai_file_utils::gcs::upload_bytes(
        &upload_config,
        &object_path,
        &compressed_bytes,
        "application/zstd",
    )
    .await
    {
        Ok(_url) => {
            tracing::info!(
                original_bytes = original_size,
                compressed_bytes = compressed_size,
                object_path = %object_path,
                "remote search index uploaded"
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, "GCS upload_bytes failed for search index");
        }
    }

    // Clean up temp file
    let _ = tokio::fs::remove_file(&compressed_path).await;

    Ok(())
}

// Download (on startup, if stale)

/// Check the remote index and download it if the local copy is stale.
///
/// Called before bootstrap when remote sync is enabled. If the remote
/// index is newer, it replaces the local `session_search.sqlite`.
///
/// Returns `true` if a remote index was downloaded and installed.
pub async fn maybe_download_index(db_path: &Path, runtime: &RemoteSyncRuntime) -> bool {
    let config = &runtime.config;
    let gcs_config = &runtime.gcs_config;
    let auth_manager = runtime.auth_manager.clone();

    if !config.enabled {
        tracing::debug!("REMOTE_SYNC_DISABLED: download skipped");
        return false;
    }

    match download_index_inner(db_path, config, gcs_config, auth_manager).await {
        Ok(downloaded) => {
            if downloaded {
                tracing::info!("REMOTE_DOWNLOAD_SUCCESS");
            }
            downloaded
        }
        Err(e) => {
            tracing::warn!(error = %e, "REMOTE_DOWNLOAD_FAILED");
            tracing::info!("LOCAL_FALLBACK");
            false
        }
    }
}

async fn download_index_inner(
    db_path: &Path,
    config: &RemoteSyncConfig,
    gcs_config: &TraceExportConfig,
    auth_manager: Option<Arc<AuthManager>>,
) -> io::Result<bool> {
    let object_path = gcs_object_path(config);

    let remote = match probe_remote_metadata(&object_path, gcs_config, auth_manager.clone()).await {
        Ok(RemoteProbe::NotFound) => {
            tracing::info!(object_path = %object_path, "REMOTE_NOT_FOUND");
            tracing::info!("LOCAL_FALLBACK");
            return Ok(false);
        }
        Ok(RemoteProbe::Found(meta)) => meta,
        Err(e) => {
            tracing::warn!(error = %e, object_path = %object_path, "REMOTE_METADATA_FAILED");
            tracing::info!("LOCAL_FALLBACK");
            return Ok(false);
        }
    };

    if !is_local_stale(db_path, remote.timestamp_unix) {
        tracing::info!(
            remote_timestamp_unix = remote.timestamp_unix,
            "LOCAL_CURRENT"
        );
        return Ok(false);
    }

    tracing::info!(
        remote_timestamp_unix = remote.timestamp_unix,
        "REMOTE_NEWER"
    );

    let compressed_path = temp_sibling(db_path, "remote-zst");
    let dst_tmp = temp_sibling(db_path, "remote-db");

    let install_result = async {
        download_compressed_to_path(
            &object_path,
            gcs_config,
            auth_manager.clone(),
            &compressed_path,
        )
        .await?;
        let src = compressed_path.clone();
        let dst_tmp_owned = dst_tmp.clone();
        let dst_final = db_path.to_path_buf();
        tokio::task::spawn_blocking(move || -> io::Result<()> {
            decompress_file(&src, &dst_tmp_owned)?;
            std::fs::rename(&dst_tmp_owned, &dst_final)?;
            Ok(())
        })
        .await
        .map_err(io::Error::other)??;
        Ok::<(), io::Error>(())
    }
    .await;

    let _ = tokio::fs::remove_file(&compressed_path).await;
    let _ = tokio::fs::remove_file(&dst_tmp).await;

    install_result.map(|_| true)
}

fn reset_upload_debounce_for_tests() {
    let map = upload_debounce_map();
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    guard.clear();
}

fn note_upload_for_tests(target: &str, ts: i64) {
    let map = upload_debounce_map();
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    guard.insert(target.to_string(), ts);
}

fn last_upload_for_tests(target: &str) -> Option<i64> {
    let map = upload_debounce_map();
    let guard = map.lock().unwrap_or_else(|e| e.into_inner());
    guard.get(target).copied()
}

#[cfg(test)]
pub(crate) fn runtime_for_tests(
    config: RemoteSyncConfig,
    gcs_config: TraceExportConfig,
    auth_manager: Option<Arc<AuthManager>>,
) -> RemoteSyncRuntime {
    RemoteSyncRuntime {
        config,
        gcs_config,
        auth_manager,
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_sync_config_default() {
        let config = RemoteSyncConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.gcs_prefix, "session_search_index");
    }

    #[test]
    fn test_remote_sync_config_deserialize() {
        let toml_str = r#"
            enabled = true
            gcs_prefix = "custom/prefix"
        "#;
        let config: RemoteSyncConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.gcs_prefix, "custom/prefix");
    }

    #[test]
    fn test_gcs_object_path() {
        let config = RemoteSyncConfig {
            enabled: true,
            gcs_prefix: "my_prefix".to_string(),
        };
        assert_eq!(
            gcs_object_path(&config),
            "my_prefix/session_search.sqlite.zst"
        );
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let original = tmp.path().join("original.db");
        let compressed = tmp.path().join("compressed.zst");
        let decompressed = tmp.path().join("decompressed.db");

        // Write test data
        let test_data = b"Hello, this is test data for zstd compression roundtrip!";
        std::fs::write(&original, test_data).unwrap();

        // Compress
        let bytes_read = compress_file(&original, &compressed).unwrap();
        assert!(bytes_read > 0);
        assert!(compressed.exists());

        // Verify compressed file is different from original
        let compressed_bytes = std::fs::read(&compressed).unwrap();
        assert_ne!(&compressed_bytes[..], &test_data[..]);

        // Decompress
        let bytes_written = decompress_file(&compressed, &decompressed).unwrap();
        assert_eq!(bytes_written, test_data.len() as u64);

        // Verify roundtrip
        let result = std::fs::read(&decompressed).unwrap();
        assert_eq!(&result[..], &test_data[..]);
    }

    #[test]
    fn test_compress_large_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let original = tmp.path().join("large.db");
        let compressed = tmp.path().join("large.zst");

        // 1 MB of repeated data (should compress well)
        let test_data: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();
        std::fs::write(&original, &test_data).unwrap();

        compress_file(&original, &compressed).unwrap();

        let original_size = std::fs::metadata(&original).unwrap().len();
        let compressed_size = std::fs::metadata(&compressed).unwrap().len();

        // Repeated data should compress significantly
        assert!(
            compressed_size < original_size / 2,
            "compressed ({compressed_size}) should be much smaller than original ({original_size})"
        );
    }

    #[test]
    fn test_is_local_stale_no_db() {
        // No DB file → stale
        assert!(is_local_stale(Path::new("/nonexistent/db.sqlite"), 0));
    }

    #[test]
    fn test_is_local_stale_no_meta() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("session_search.sqlite");

        // Create DB without last_bootstrap_at
        let _index = SessionSearchIndex::open_or_create(&db_path).unwrap();

        // No bootstrap timestamp → stale
        assert!(is_local_stale(&db_path, 100));
    }

    #[test]
    fn test_is_local_stale_fresh() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("session_search.sqlite");

        let index = SessionSearchIndex::open_or_create(&db_path).unwrap();
        let now = chrono::Utc::now().timestamp();
        index
            .set_meta(META_KEY_LAST_BOOTSTRAP, &now.to_string())
            .unwrap();

        // Remote timestamp is only 10 seconds ahead → not stale
        assert!(!is_local_stale(&db_path, now + 10));
    }

    #[test]
    fn test_is_local_stale_old() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("session_search.sqlite");

        let index = SessionSearchIndex::open_or_create(&db_path).unwrap();
        let old_ts = chrono::Utc::now().timestamp() - 7200; // 2 hours ago
        index
            .set_meta(META_KEY_LAST_BOOTSTRAP, &old_ts.to_string())
            .unwrap();

        // Remote is 2 hours newer → stale
        let remote_ts = chrono::Utc::now().timestamp();
        assert!(is_local_stale(&db_path, remote_ts));
    }

    #[test]
    fn test_is_local_stale_remote_unknown() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("session_search.sqlite");

        let index = SessionSearchIndex::open_or_create(&db_path).unwrap();
        let now = chrono::Utc::now().timestamp();
        index
            .set_meta(META_KEY_LAST_BOOTSTRAP, &now.to_string())
            .unwrap();

        // Remote timestamp 0 (unknown) with local bootstrap → not stale
        assert!(!is_local_stale(&db_path, 0));
    }

    #[test]
    fn test_upload_debounce_initial() {
        reset_upload_debounce_for_tests();
        assert!(upload_debounce_ok("t1"));
        let now = chrono::Utc::now().timestamp();
        note_upload_for_tests("t1", now);
        assert!(!upload_debounce_ok("t1"));
    }

    #[test]
    fn test_upload_debounce_isolated_by_target() {
        reset_upload_debounce_for_tests();
        let now = chrono::Utc::now().timestamp();
        note_upload_for_tests("target-a", now);
        assert!(!upload_debounce_ok("target-a"));
        assert!(upload_debounce_ok("target-b"));
    }

    #[test]
    fn test_upload_debounce_record_updates_timestamp() {
        reset_upload_debounce_for_tests();
        record_upload("target-z");
        assert!(last_upload_for_tests("target-z").is_some());
    }

    #[test]
    fn test_read_write_last_bootstrap_at() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("session_search.sqlite");

        // Before writing, should be None
        assert_eq!(read_last_bootstrap_at(&db_path), None);

        // Create DB and write timestamp
        write_last_bootstrap_at(&db_path).unwrap();

        // Should now have a reasonable timestamp
        let ts = read_last_bootstrap_at(&db_path).unwrap();
        let now = chrono::Utc::now().timestamp();
        assert!(
            (now - ts).abs() < 5,
            "timestamp should be within 5 seconds of now"
        );
    }
}
