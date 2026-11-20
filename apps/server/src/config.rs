use std::path::PathBuf;

/// Object storage backend selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageConfig {
    Local {
        root_dir: PathBuf,
    },
    S3 {
        endpoint: String,
        bucket: String,
        region: String,
        access_key: String,
        secret_key: String,
    },
}

/// Runtime configuration for `clipforge-server`.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub database_path: PathBuf,
    pub storage: StorageConfig,
    pub max_upload_bytes: u64,
    pub public_base_url: Option<String>,
    pub web_dir: Option<PathBuf>,
    pub bootstrap_api_key: Option<String>,
    pub session_ttl_hours: i64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:3000".to_string(),
            database_path: PathBuf::from("clipforge-server.db"),
            storage: StorageConfig::Local {
                root_dir: PathBuf::from("data/clips"),
            },
            max_upload_bytes: 500 * 1024 * 1024, // 500 MiB
            public_base_url: None,
            web_dir: None,
            bootstrap_api_key: None,
            session_ttl_hours: 24 * 30,
        }
    }
}

impl ServerConfig {
    /// Loads server configuration from `CLIPFORGE_*` environment variables.
    pub fn from_env() -> Result<Self, String> {
        let mut cfg = Self::default();

        if let Ok(v) = std::env::var("CLIPFORGE_BIND_ADDR") {
            cfg.bind_addr = v;
        }
        if let Ok(v) = std::env::var("CLIPFORGE_DATABASE_PATH") {
            cfg.database_path = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("CLIPFORGE_MAX_UPLOAD_BYTES") {
            cfg.max_upload_bytes = v
                .parse::<u64>()
                .map_err(|e| format!("invalid CLIPFORGE_MAX_UPLOAD_BYTES: {e}"))?;
        }
        if let Ok(v) = std::env::var("CLIPFORGE_PUBLIC_BASE_URL")
            && !v.trim().is_empty()
        {
            cfg.public_base_url = Some(v.trim_end_matches('/').to_string());
        }
        if let Ok(v) = std::env::var("CLIPFORGE_WEB_DIR")
            && !v.trim().is_empty()
        {
            cfg.web_dir = Some(PathBuf::from(v));
        }
        if let Ok(v) = std::env::var("CLIPFORGE_BOOTSTRAP_API_KEY")
            && !v.trim().is_empty()
        {
            cfg.bootstrap_api_key = Some(v);
        }
        if let Ok(v) = std::env::var("CLIPFORGE_SESSION_TTL_HOURS") {
            cfg.session_ttl_hours = v
                .parse::<i64>()
                .map_err(|e| format!("invalid CLIPFORGE_SESSION_TTL_HOURS: {e}"))?;
        }

        let backend = std::env::var("CLIPFORGE_STORAGE_BACKEND")
            .unwrap_or_else(|_| "local".to_string())
            .to_lowercase();

        cfg.storage = match backend.as_str() {
            "local" => {
                let root_dir = std::env::var("CLIPFORGE_STORAGE_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from("data/clips"));
                StorageConfig::Local { root_dir }
            }
            "s3" => {
                let endpoint = std::env::var("CLIPFORGE_S3_ENDPOINT")
                    .map_err(|_| "CLIPFORGE_S3_ENDPOINT is required for s3 backend".to_string())?;
                let bucket = std::env::var("CLIPFORGE_S3_BUCKET")
                    .map_err(|_| "CLIPFORGE_S3_BUCKET is required for s3 backend".to_string())?;
                let region = std::env::var("CLIPFORGE_S3_REGION")
                    .unwrap_or_else(|_| "us-east-1".to_string());
                let access_key = std::env::var("CLIPFORGE_S3_ACCESS_KEY").map_err(|_| {
                    "CLIPFORGE_S3_ACCESS_KEY is required for s3 backend".to_string()
                })?;
                let secret_key = std::env::var("CLIPFORGE_S3_SECRET_KEY").map_err(|_| {
                    "CLIPFORGE_S3_SECRET_KEY is required for s3 backend".to_string()
                })?;
                StorageConfig::S3 {
                    endpoint: endpoint.trim_end_matches('/').to_string(),
                    bucket,
                    region,
                    access_key,
                    secret_key,
                }
            }
            other => {
                return Err(format!("unsupported CLIPFORGE_STORAGE_BACKEND: {other}"));
            }
        };

        Ok(cfg)
    }
}
