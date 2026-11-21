use std::path::PathBuf;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::Utc;
use clipforge_cloud_client::{CloudClient, HttpCloudClient, ShareVisibility};
use clipforge_common::{ClipId, ClipMetadata, GameInfo};
use clipforge_server::build_app;
use clipforge_server::config::{ServerConfig, StorageConfig};
use tempfile::TempDir;
use tower::ServiceExt;

fn test_config(dir: &TempDir, max_upload_bytes: u64) -> ServerConfig {
    ServerConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        database_path: dir.path().join("test.db"),
        storage: StorageConfig::Local {
            root_dir: dir.path().join("objects"),
        },
        max_upload_bytes,
        public_base_url: None,
        web_dir: None,
        bootstrap_api_key: None,
        session_ttl_hours: 24,
    }
}

fn sample_clip_metadata(title: &str) -> ClipMetadata {
    ClipMetadata {
        id: ClipId::new(),
        title: title.to_string(),
        game: Some(GameInfo {
            id: "valorant".to_string(),
            name: "VALORANT".to_string(),
            executable: Some("VALORANT-Win64-Shipping.exe".to_string()),
            process_name: None,
        }),
        file_path: PathBuf::from("clips/highlight.mp4"),
        thumbnail_path: None,
        created_at: Utc::now(),
        duration_ms: 15_000,
        width: 1920,
        height: 1080,
        fps: 60.0,
        size_bytes: 0,
        codec: "h264".to_string(),
        audio_tracks: vec![],
        tags: vec!["ace".to_string(), "clutch".to_string()],
        cloud_share: None,
        notes: Some("Clean 1v4 clutch".to_string()),
    }
}

async fn read_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("valid json")
}

#[tokio::test]
async fn auth_register_login_api_key_and_logout_flow() {
    let dir = TempDir::new().unwrap();
    let app = build_app(test_config(&dir, 10 * 1024 * 1024))
        .await
        .unwrap();

    // Register
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "alice", "password": "super-secret-password" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let reg_json = read_json(resp).await;
    let reg_token = reg_json["token"].as_str().unwrap().to_string();

    // Wrong password login
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "alice", "password": "wrong-password" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Valid login
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "alice", "password": "super-secret-password" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Create API key
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/api-keys")
                .header(header::AUTHORIZATION, format!("Bearer {reg_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "name": "desktop-app" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let key_json = read_json(resp).await;
    let key_id = key_json["id"].as_str().unwrap().to_string();
    let raw_key = key_json["key"].as_str().unwrap().to_string();
    assert!(raw_key.starts_with("cf_key_"));

    // Authenticate with X-API-Key
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/clips")
                .header("x-api-key", &raw_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Revoke API key
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/auth/api-keys/{key_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {reg_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Revoked key is rejected
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/clips")
                .header("x-api-key", &raw_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Logout revokes bearer token
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/logout")
                .header(header::AUTHORIZATION, format!("Bearer {reg_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/clips")
                .header(header::AUTHORIZATION, format!("Bearer {reg_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cloud_client_upload_range_visibility_and_delete_e2e() {
    let dir = TempDir::new().unwrap();
    let app = build_app(test_config(&dir, 5 * 1024 * 1024)).await.unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let base_url = format!("http://{addr}");
    let client = HttpCloudClient::new(&base_url, None);

    // Health check
    client.health_check().await.expect("health check");

    // Register owner & second user
    let owner_auth = client
        .register("owner_user", "owner-password-123")
        .await
        .expect("register owner");
    let other_auth = client
        .register("other_user", "other-password-123")
        .await
        .expect("register other");

    let owner_client = client.clone().with_token(owner_auth.token.clone());
    let other_client = client.clone().with_token(other_auth.token.clone());

    // Create sample video file (64 bytes of deterministic data)
    let video_bytes: Vec<u8> = (0..64u8).collect();
    let video_path = dir.path().join("sample.mp4");
    tokio::fs::write(&video_path, &video_bytes).await.unwrap();

    let clip_meta = sample_clip_metadata("Insane Clutch");
    let share = owner_client
        .upload_clip(&clip_meta, &video_path)
        .await
        .expect("upload clip");
    assert!(!share.share_id.is_empty());
    assert!(share.url.contains(&share.share_id));

    // Public web player HTML page loads
    let http = reqwest::Client::new();
    let page_resp = http
        .get(format!("{base_url}/s/{}", share.share_id))
        .send()
        .await
        .unwrap();
    assert_eq!(page_resp.status(), reqwest::StatusCode::OK);
    let html = page_resp.text().await.unwrap();
    assert!(html.contains("ClipForge"));
    assert!(html.contains("<video"));

    // Public range request (bytes 0-9)
    let range_resp = http
        .get(format!("{base_url}/api/v1/shares/{}/video", share.share_id))
        .header("Range", "bytes=0-9")
        .send()
        .await
        .unwrap();
    assert_eq!(range_resp.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        range_resp.headers().get("content-range").unwrap(),
        "bytes 0-9/64"
    );
    let part = range_resp.bytes().await.unwrap();
    assert_eq!(part.as_ref(), &video_bytes[0..10]);

    // Public range request (bytes 20-35)
    let range_resp_2 = http
        .get(format!("{base_url}/api/v1/shares/{}/video", share.share_id))
        .header("Range", "bytes=20-35")
        .send()
        .await
        .unwrap();
    assert_eq!(range_resp_2.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        range_resp_2.headers().get("content-range").unwrap(),
        "bytes 20-35/64"
    );
    let part2 = range_resp_2.bytes().await.unwrap();
    assert_eq!(part2.as_ref(), &video_bytes[20..=35]);

    // Out-of-bounds range -> 416
    let bad_range = http
        .get(format!("{base_url}/api/v1/shares/{}/video", share.share_id))
        .header("Range", "bytes=999-1000")
        .send()
        .await
        .unwrap();
    assert_eq!(
        bad_range.status(),
        reqwest::StatusCode::RANGE_NOT_SATISFIABLE
    );

    // Switch visibility to private
    let updated = owner_client
        .set_visibility(&clip_meta.id.to_string(), ShareVisibility::Private)
        .await
        .expect("set private");
    assert_eq!(updated.visibility, ShareVisibility::Private);

    // Unauthenticated access to private share is rejected (401)
    let unauth_meta = http
        .get(format!("{base_url}/api/v1/shares/{}/meta", share.share_id))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_meta.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Another user cannot access private share (403)
    let other_err = other_client.get_share(&share.share_id).await.unwrap_err();
    assert!(other_err.to_string().contains("403"));

    // Owner can still access private share & range stream
    let owner_share = owner_client
        .get_share(&share.share_id)
        .await
        .expect("owner get private share");
    assert_eq!(owner_share.share_id, share.share_id);

    let owner_range = http
        .get(format!("{base_url}/api/v1/shares/{}/video", share.share_id))
        .bearer_auth(&owner_auth.token)
        .header("Range", "bytes=50-63")
        .send()
        .await
        .unwrap();
    assert_eq!(owner_range.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        owner_range.bytes().await.unwrap().as_ref(),
        &video_bytes[50..64]
    );

    // List clips
    let list = owner_client
        .list_clips(None, None)
        .await
        .expect("list clips");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].share_id, share.share_id);

    // Delete share
    owner_client
        .delete_share(&share.share_id)
        .await
        .expect("delete share");

    // Verify deleted
    let after_del = owner_client.get_share(&share.share_id).await;
    assert!(after_del.is_err());

    server_handle.abort();
}

#[tokio::test]
async fn upload_exceeding_max_bytes_returns_413() {
    let dir = TempDir::new().unwrap();
    // Set max upload size to 32 bytes
    let app = build_app(test_config(&dir, 32)).await.unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let base_url = format!("http://{addr}");
    let client = HttpCloudClient::new(&base_url, None);
    let auth = client
        .register("limit_user", "limit-password-123")
        .await
        .unwrap();
    let authed = client.with_token(auth.token);

    let big_file = dir.path().join("too_big.mp4");
    tokio::fs::write(&big_file, vec![7u8; 128]).await.unwrap();

    let clip_meta = sample_clip_metadata("Too Big");
    let err = authed.upload_clip(&clip_meta, &big_file).await.unwrap_err();
    assert!(err.to_string().contains("413"));

    server_handle.abort();
}
