use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use futures_util::{Stream, StreamExt, stream};
use ring::{digest, hmac};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::config::StorageConfig;
use crate::error::ServerError;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

/// Metadata for a stored object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    pub size_bytes: u64,
    pub content_type: Option<String>,
}

/// Object storage abstraction supporting local disk and S3-compatible backends.
#[async_trait]
pub trait ObjectStorage: Send + Sync {
    /// Streams a file from `source_path` into storage under `key`.
    async fn put_file(
        &self,
        key: &str,
        source_path: &Path,
        size_bytes: u64,
        content_type: &str,
    ) -> Result<(), ServerError>;

    /// Returns object size and optional content-type.
    async fn head(&self, key: &str) -> Result<ObjectMeta, ServerError>;

    /// Streams an inclusive byte range `[start, end_inclusive]` for `key`.
    async fn get_range(
        &self,
        key: &str,
        start: u64,
        end_inclusive: u64,
    ) -> Result<ByteStream, ServerError>;

    /// Deletes an object by key (succeeds if already absent).
    async fn delete(&self, key: &str) -> Result<(), ServerError>;
}

/// Validates an object key so it cannot escape the storage root.
pub fn validate_storage_key(key: &str) -> Result<&str, ServerError> {
    if key.is_empty()
        || key.starts_with('/')
        || key.contains('\\')
        || key.contains('\0')
        || key.contains("..")
    {
        return Err(ServerError::BadRequest("invalid storage key".to_string()));
    }
    for comp in Path::new(key).components() {
        if !matches!(comp, Component::Normal(_)) {
            return Err(ServerError::BadRequest("invalid storage key".to_string()));
        }
    }
    Ok(key)
}

/// Sanitizes an uploaded filename and returns a safe lowercase video extension.
pub fn sanitize_video_extension(filename: Option<&str>) -> &'static str {
    let ext = filename
        .and_then(|name| Path::new(name).extension())
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("mp4") => "mp4",
        Some("mkv") => "mkv",
        Some("webm") => "webm",
        Some("mov") => "mov",
        Some("flv") => "flv",
        _ => "mp4",
    }
}

/// Maps a safe extension to its MIME content type.
pub fn content_type_for_ext(ext: &str) -> &'static str {
    match ext {
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "flv" => "video/x-flv",
        _ => "application/octet-stream",
    }
}

/// Builds an [`ObjectStorage`] instance from [`StorageConfig`].
pub fn build_storage(
    config: &StorageConfig,
) -> Result<std::sync::Arc<dyn ObjectStorage>, ServerError> {
    match config {
        StorageConfig::Local { root_dir } => {
            std::fs::create_dir_all(root_dir)?;
            Ok(std::sync::Arc::new(LocalObjectStorage::new(
                root_dir.clone(),
            )))
        }
        StorageConfig::S3 {
            endpoint,
            bucket,
            region,
            access_key,
            secret_key,
        } => Ok(std::sync::Arc::new(S3ObjectStorage::new(
            endpoint.clone(),
            bucket.clone(),
            region.clone(),
            access_key.clone(),
            secret_key.clone(),
        ))),
    }
}

/// Local filesystem object storage backend.
#[derive(Debug, Clone)]
pub struct LocalObjectStorage {
    root_dir: PathBuf,
}

impl LocalObjectStorage {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    fn resolve_path(&self, key: &str) -> Result<PathBuf, ServerError> {
        let safe_key = validate_storage_key(key)?;
        Ok(self.root_dir.join(safe_key))
    }
}

#[async_trait]
impl ObjectStorage for LocalObjectStorage {
    async fn put_file(
        &self,
        key: &str,
        source_path: &Path,
        _size_bytes: u64,
        _content_type: &str,
    ) -> Result<(), ServerError> {
        let dest = self.resolve_path(key)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut src = tokio::fs::File::open(source_path).await?;
        let mut dst = tokio::fs::File::create(&dest).await?;
        tokio::io::copy(&mut src, &mut dst).await?;
        Ok(())
    }

    async fn head(&self, key: &str) -> Result<ObjectMeta, ServerError> {
        let path = self.resolve_path(key)?;
        let meta = tokio::fs::metadata(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ServerError::NotFound("clip video not found".to_string())
            } else {
                ServerError::Internal(format!("failed to stat object: {e}"))
            }
        })?;
        let content_type = mime_guess::from_path(&path)
            .first_raw()
            .map(ToString::to_string);
        Ok(ObjectMeta {
            size_bytes: meta.len(),
            content_type,
        })
    }

    async fn get_range(
        &self,
        key: &str,
        start: u64,
        end_inclusive: u64,
    ) -> Result<ByteStream, ServerError> {
        let path = self.resolve_path(key)?;
        let mut file = tokio::fs::File::open(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ServerError::NotFound("clip video not found".to_string())
            } else {
                ServerError::Internal(format!("failed to open object: {e}"))
            }
        })?;

        if start > end_inclusive {
            return Ok(Box::pin(stream::empty()));
        }

        file.seek(std::io::SeekFrom::Start(start)).await?;
        let len = end_inclusive - start + 1;
        let limited = file.take(len);
        let reader_stream = ReaderStream::with_capacity(limited, 64 * 1024);
        Ok(Box::pin(reader_stream))
    }

    async fn delete(&self, key: &str) -> Result<(), ServerError> {
        let path = self.resolve_path(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ServerError::Internal(format!(
                "failed to delete object: {e}"
            ))),
        }
    }
}

/// S3-compatible object storage backend using AWS Signature Version 4.
#[derive(Debug, Clone)]
pub struct S3ObjectStorage {
    client: reqwest::Client,
    endpoint: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
}

impl S3ObjectStorage {
    pub fn new(
        endpoint: String,
        bucket: String,
        region: String,
        access_key: String,
        secret_key: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            bucket,
            region,
            access_key,
            secret_key,
        }
    }

    fn object_url(&self, key: &str) -> Result<String, ServerError> {
        let safe_key = validate_storage_key(key)?;
        Ok(format!("{}/{}/{}", self.endpoint, self.bucket, safe_key))
    }

    fn host_header(&self) -> String {
        self.endpoint
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("localhost")
            .to_string()
    }

    fn sign_headers(
        &self,
        method: &str,
        key: &str,
        payload_hash: &str,
    ) -> Result<Vec<(String, String)>, ServerError> {
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let host = self.host_header();
        let canonical_uri = format!("/{}/{}", self.bucket, validate_storage_key(key)?);

        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "{method}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );

        let req_hash = hex::encode(digest::digest(
            &digest::SHA256,
            canonical_request.as_bytes(),
        ));
        let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", self.region);
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{req_hash}");

        let k_secret = format!("AWS4{}", self.secret_key);
        let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"s3");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");
        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key
        );

        Ok(vec![
            ("x-amz-date".to_string(), amz_date),
            ("x-amz-content-sha256".to_string(), payload_hash.to_string()),
            ("Authorization".to_string(), authorization),
        ])
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let k = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::sign(&k, data).as_ref().to_vec()
}

#[async_trait]
impl ObjectStorage for S3ObjectStorage {
    async fn put_file(
        &self,
        key: &str,
        source_path: &Path,
        size_bytes: u64,
        content_type: &str,
    ) -> Result<(), ServerError> {
        let url = self.object_url(key)?;
        let file = tokio::fs::File::open(source_path).await?;
        let stream = ReaderStream::with_capacity(file, 64 * 1024);
        let body = reqwest::Body::wrap_stream(stream);
        let headers = self.sign_headers("PUT", key, "UNSIGNED-PAYLOAD")?;

        let mut req = self
            .client
            .put(url)
            .header("Content-Type", content_type)
            .header("Content-Length", size_bytes.to_string())
            .body(body);
        for (k, v) in headers {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ServerError::Internal(format!("s3 put failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ServerError::Internal(format!(
                "s3 put returned status {}",
                resp.status()
            )));
        }
        Ok(())
    }

    async fn head(&self, key: &str) -> Result<ObjectMeta, ServerError> {
        let url = self.object_url(key)?;
        let empty_hash = hex::encode(digest::digest(&digest::SHA256, b""));
        let headers = self.sign_headers("HEAD", key, &empty_hash)?;

        let mut req = self.client.head(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ServerError::Internal(format!("s3 head failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ServerError::NotFound("clip video not found".to_string()));
        }
        if !resp.status().is_success() {
            return Err(ServerError::Internal(format!(
                "s3 head returned status {}",
                resp.status()
            )));
        }
        let size_bytes = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);
        Ok(ObjectMeta {
            size_bytes,
            content_type,
        })
    }

    async fn get_range(
        &self,
        key: &str,
        start: u64,
        end_inclusive: u64,
    ) -> Result<ByteStream, ServerError> {
        let url = self.object_url(key)?;
        let empty_hash = hex::encode(digest::digest(&digest::SHA256, b""));
        let headers = self.sign_headers("GET", key, &empty_hash)?;

        let mut req = self
            .client
            .get(url)
            .header("Range", format!("bytes={start}-{end_inclusive}"));
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ServerError::Internal(format!("s3 get failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ServerError::NotFound("clip video not found".to_string()));
        }
        if !resp.status().is_success() {
            return Err(ServerError::Internal(format!(
                "s3 get returned status {}",
                resp.status()
            )));
        }

        let byte_stream = resp
            .bytes_stream()
            .map(|res| res.map_err(std::io::Error::other));
        Ok(Box::pin(byte_stream))
    }

    async fn delete(&self, key: &str) -> Result<(), ServerError> {
        let url = self.object_url(key)?;
        let empty_hash = hex::encode(digest::digest(&digest::SHA256, b""));
        let headers = self.sign_headers("DELETE", key, &empty_hash)?;

        let mut req = self.client.delete(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ServerError::Internal(format!("s3 delete failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND || resp.status().is_success() {
            Ok(())
        } else {
            Err(ServerError::Internal(format!(
                "s3 delete returned status {}",
                resp.status()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_keys() {
        assert!(validate_storage_key("../secret").is_err());
        assert!(validate_storage_key("/etc/passwd").is_err());
        assert!(validate_storage_key("a\\b.mp4").is_err());
        assert!(validate_storage_key("").is_err());
        assert!(validate_storage_key("user1/clip1.mp4").is_ok());
    }

    #[test]
    fn sanitizes_extensions() {
        assert_eq!(sanitize_video_extension(Some("clip.MP4")), "mp4");
        assert_eq!(sanitize_video_extension(Some("../../evil.exe")), "mp4");
        assert_eq!(sanitize_video_extension(Some("highlight.webm")), "webm");
    }
}
