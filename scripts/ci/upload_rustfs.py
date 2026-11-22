#!/usr/bin/env python3
"""Pure-stdlib AWS SigV4 artifact uploader for RustFS (S3-compatible)."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import hmac
import http.client
import json
import logging
import os
import sys
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping, Sequence

DEFAULT_ENDPOINT = "https://s3.emkacz.dev"
DEFAULT_BUCKET = "clipforge-builds"
DEFAULT_REGION = "us-east-1"
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()

LOGGER = logging.getLogger("upload_rustfs")


class UploadError(RuntimeError):
    """Raised when an S3 operation or immutability/verification check fails."""


@dataclass(frozen=True)
class S3Config:
    """Configuration for path-style S3 SigV4 requests."""

    endpoint: str
    bucket: str
    region: str
    access_key_id: str
    secret_access_key: str

    @classmethod
    def from_env(cls, env: Mapping[str, str] | None = None) -> "S3Config":
        source = os.environ if env is None else env
        endpoint = (source.get("RUSTFS_ENDPOINT") or DEFAULT_ENDPOINT).strip()
        bucket = (source.get("RUSTFS_BUCKET") or DEFAULT_BUCKET).strip()
        region = (source.get("RUSTFS_REGION") or DEFAULT_REGION).strip()
        access_key_id = (source.get("RUSTFS_ACCESS_KEY_ID") or "").strip()
        secret_access_key = (source.get("RUSTFS_SECRET_ACCESS_KEY") or "").strip()

        if not access_key_id or not secret_access_key:
            raise UploadError(
                "RUSTFS_ACCESS_KEY_ID and RUSTFS_SECRET_ACCESS_KEY must be set."
            )
        if not endpoint:
            raise UploadError("RUSTFS_ENDPOINT must not be empty.")
        if not bucket:
            raise UploadError("RUSTFS_BUCKET must not be empty.")
        if not region:
            raise UploadError("RUSTFS_REGION must not be empty.")

        return cls(
            endpoint=endpoint,
            bucket=bucket,
            region=region,
            access_key_id=access_key_id,
            secret_access_key=secret_access_key,
        )


def _hmac_sha256(key: bytes, msg: str) -> bytes:
    return hmac.new(key, msg.encode("utf-8"), hashlib.sha256).digest()


def _derive_signing_key(secret_key: str, date_stamp: str, region: str) -> bytes:
    k_date = _hmac_sha256(("AWS4" + secret_key).encode("utf-8"), date_stamp)
    k_region = _hmac_sha256(k_date, region)
    k_service = _hmac_sha256(k_region, "s3")
    return _hmac_sha256(k_service, "aws4_request")


def _encode_s3_path(base_path: str, bucket: str, key: str) -> str:
    clean_base = base_path.rstrip("/")
    encoded_bucket = urllib.parse.quote(bucket, safe="")
    segments = [urllib.parse.quote(seg, safe="") for seg in key.lstrip("/").split("/") if seg]
    if not segments:
        raise UploadError("Object key must not be empty.")
    return f"{clean_base}/{encoded_bucket}/" + "/".join(segments)


def build_sigv4_headers(
    *,
    method: str,
    endpoint: str,
    bucket: str,
    key: str,
    region: str,
    access_key_id: str,
    secret_access_key: str,
    payload_sha256: str,
    content_length: int | None = None,
    content_type: str | None = None,
    now: dt.datetime | None = None,
) -> tuple[str, dict[str, str]]:
    """Build path-style request path and signed AWS SigV4 headers."""
    parsed = urllib.parse.urlparse(endpoint)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise UploadError(f"Invalid S3 endpoint URL: {endpoint!r}")

    timestamp = (now or dt.datetime.now(dt.timezone.utc)).astimezone(dt.timezone.utc)
    amz_date = timestamp.strftime("%Y%m%dT%H%M%SZ")
    date_stamp = timestamp.strftime("%Y%m%d")

    canonical_uri = _encode_s3_path(parsed.path, bucket, key)

    headers_to_sign: dict[str, str] = {
        "host": parsed.netloc,
        "x-amz-content-sha256": payload_sha256,
        "x-amz-date": amz_date,
    }
    if content_type is not None:
        headers_to_sign["content-type"] = content_type
    if content_length is not None:
        headers_to_sign["content-length"] = str(content_length)

    sorted_keys = sorted(headers_to_sign.keys())
    canonical_headers = "".join(
        f"{k}:{headers_to_sign[k].strip()}\n" for k in sorted_keys
    )
    signed_headers = ";".join(sorted_keys)

    canonical_request = "\n".join(
        [
            method.upper(),
            canonical_uri,
            "",  # No query string for HeadObject / PutObject
            canonical_headers,
            signed_headers,
            payload_sha256,
        ]
    )

    credential_scope = f"{date_stamp}/{region}/s3/aws4_request"
    string_to_sign = "\n".join(
        [
            "AWS4-HMAC-SHA256",
            amz_date,
            credential_scope,
            hashlib.sha256(canonical_request.encode("utf-8")).hexdigest(),
        ]
    )

    signing_key = _derive_signing_key(secret_access_key, date_stamp, region)
    signature = hmac.new(
        signing_key, string_to_sign.encode("utf-8"), hashlib.sha256
    ).hexdigest()

    authorization = (
        f"AWS4-HMAC-SHA256 Credential={access_key_id}/{credential_scope}, "
        f"SignedHeaders={signed_headers}, Signature={signature}"
    )

    request_headers: dict[str, str] = {
        "Host": parsed.netloc,
        "x-amz-date": amz_date,
        "x-amz-content-sha256": payload_sha256,
        "Authorization": authorization,
    }
    if content_type is not None:
        request_headers["Content-Type"] = content_type
    if content_length is not None:
        request_headers["Content-Length"] = str(content_length)

    return canonical_uri, request_headers


class RustFSClient:
    """Minimal path-style S3 client using only Python 3 standard library."""

    def __init__(self, config: S3Config, timeout: float = 60.0) -> None:
        self.config = config
        self.timeout = timeout
        self._parsed = urllib.parse.urlparse(config.endpoint)

    def _open_connection(self) -> http.client.HTTPConnection:
        host = self._parsed.hostname
        if not host:
            raise UploadError(f"Invalid endpoint host: {self.config.endpoint!r}")
        if self._parsed.scheme == "https":
            return http.client.HTTPSConnection(
                host, self._parsed.port, timeout=self.timeout
            )
        return http.client.HTTPConnection(
            host, self._parsed.port, timeout=self.timeout
        )

    def head_object(self, key: str) -> tuple[int, dict[str, str]]:
        """Send HeadObject and return (status_code, lowercase_headers)."""
        path, headers = build_sigv4_headers(
            method="HEAD",
            endpoint=self.config.endpoint,
            bucket=self.config.bucket,
            key=key,
            region=self.config.region,
            access_key_id=self.config.access_key_id,
            secret_access_key=self.config.secret_access_key,
            payload_sha256=EMPTY_SHA256,
        )
        conn = self._open_connection()
        try:
            conn.request("HEAD", path, body=b"", headers=headers)
            resp = conn.getresponse()
            resp.read()
            resp_headers = {k.lower(): v for k, v in resp.getheaders()}
            return resp.status, resp_headers
        finally:
            conn.close()

    def put_bytes(
        self,
        key: str,
        data: bytes,
        content_type: str = "application/octet-stream",
    ) -> int:
        """Upload raw bytes via PutObject and return HTTP status code."""
        payload_sha256 = hashlib.sha256(data).hexdigest()
        path, headers = build_sigv4_headers(
            method="PUT",
            endpoint=self.config.endpoint,
            bucket=self.config.bucket,
            key=key,
            region=self.config.region,
            access_key_id=self.config.access_key_id,
            secret_access_key=self.config.secret_access_key,
            payload_sha256=payload_sha256,
            content_length=len(data),
            content_type=content_type,
        )
        conn = self._open_connection()
        try:
            conn.request("PUT", path, body=data, headers=headers)
            resp = conn.getresponse()
            resp.read()
            return resp.status
        finally:
            conn.close()

    def put_file(
        self,
        key: str,
        file_path: Path,
        payload_sha256: str,
        size_bytes: int,
        content_type: str = "application/octet-stream",
    ) -> int:
        """Stream a file via PutObject and return HTTP status code."""
        path, headers = build_sigv4_headers(
            method="PUT",
            endpoint=self.config.endpoint,
            bucket=self.config.bucket,
            key=key,
            region=self.config.region,
            access_key_id=self.config.access_key_id,
            secret_access_key=self.config.secret_access_key,
            payload_sha256=payload_sha256,
            content_length=size_bytes,
            content_type=content_type,
        )
        conn = self._open_connection()
        try:
            with file_path.open("rb") as handle:
                conn.request("PUT", path, body=handle, headers=headers)
                resp = conn.getresponse()
                resp.read()
                return resp.status
        finally:
            conn.close()

    def _ensure_absent(self, key: str) -> None:
        status, _ = self.head_object(key)
        if status == 404:
            return
        if status == 200:
            raise UploadError(
                f"Refusing to overwrite immutable object: {self.config.bucket}/{key}"
            )
        raise UploadError(
            f"Unexpected HeadObject status {status} before upload for {self.config.bucket}/{key}"
        )

    def _verify_uploaded(self, key: str, expected_size: int) -> None:
        status, headers = self.head_object(key)
        if status != 200:
            raise UploadError(
                f"Post-upload HeadObject failed for {self.config.bucket}/{key}: status {status}"
            )
        raw_len = headers.get("content-length")
        if raw_len is None:
            raise UploadError(
                f"Post-upload HeadObject missing Content-Length for {self.config.bucket}/{key}"
            )
        try:
            actual_len = int(raw_len)
        except ValueError as exc:
            raise UploadError(
                f"Invalid Content-Length {raw_len!r} for {self.config.bucket}/{key}"
            ) from exc
        if actual_len != expected_size:
            raise UploadError(
                f"Content-Length mismatch for {self.config.bucket}/{key}: "
                f"expected {expected_size}, got {actual_len}"
            )

    def upload_immutable_bytes(
        self,
        key: str,
        data: bytes,
        content_type: str = "application/octet-stream",
    ) -> None:
        """Check absence, upload bytes, and verify HeadObject 200 + Content-Length."""
        self._ensure_absent(key)
        status = self.put_bytes(key, data, content_type=content_type)
        if status not in (200, 201):
            raise UploadError(
                f"PutObject failed for {self.config.bucket}/{key}: status {status}"
            )
        self._verify_uploaded(key, len(data))
        LOGGER.info("Uploaded and verified %s/%s (%d bytes)", self.config.bucket, key, len(data))

    def upload_immutable_file(
        self,
        key: str,
        file_path: Path,
        payload_sha256: str,
        size_bytes: int,
        content_type: str = "application/octet-stream",
    ) -> None:
        """Check absence, stream file, and verify HeadObject 200 + Content-Length."""
        self._ensure_absent(key)
        status = self.put_file(
            key,
            file_path,
            payload_sha256=payload_sha256,
            size_bytes=size_bytes,
            content_type=content_type,
        )
        if status not in (200, 201):
            raise UploadError(
                f"PutObject failed for {self.config.bucket}/{key}: status {status}"
            )
        self._verify_uploaded(key, size_bytes)
        LOGGER.info("Uploaded and verified %s/%s (%d bytes)", self.config.bucket, key, size_bytes)


def compute_sha256_and_size(file_path: Path) -> tuple[str, int]:
    """Compute SHA-256 hex digest and byte size of a file."""
    hasher = hashlib.sha256()
    total = 0
    with file_path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            hasher.update(chunk)
            total += len(chunk)
    return hasher.hexdigest(), total


def _sanitize_segment(value: str, name: str) -> str:
    cleaned = value.strip().strip("/")
    if not cleaned or ".." in cleaned.split("/"):
        raise UploadError(f"Invalid {name} value: {value!r}")
    return cleaned


def build_key_prefix(
    *,
    mode: str,
    platform: str,
    commit: str,
    run_id: str,
    date_str: str | None = None,
    version: str | None = None,
    smoke_test: bool = False,
) -> str:
    """Build the immutable S3 key prefix for the requested upload mode."""
    safe_commit = _sanitize_segment(commit, "commit")
    safe_run_id = _sanitize_segment(run_id, "run-id")
    if smoke_test:
        return f"ci/smoke/{safe_commit}/{safe_run_id}/"

    safe_platform = _sanitize_segment(platform, "platform")
    if mode == "ci":
        return f"ci/{safe_platform}/{safe_commit}/{safe_run_id}/"
    if mode == "nightly":
        safe_date = _sanitize_segment(
            date_str or dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d"),
            "date",
        )
        return f"nightly/{safe_platform}/{safe_date}/"
    if mode == "releases":
        if not version:
            raise UploadError("--version is required when --mode=releases")
        safe_version = _sanitize_segment(version, "version")
        return f"releases/{safe_version}/{safe_platform}/"
    raise UploadError(f"Unsupported upload mode: {mode!r}")


def upload_artifacts(
    client: RustFSClient,
    *,
    prefix: str,
    artifact_paths: Sequence[Path],
    commit: str,
    run_id: str,
    platform: str,
    workflow: str,
    timestamp: str | None = None,
) -> list[str]:
    """Upload artifacts, their .sha256 files, and build-metadata.json."""
    if not artifact_paths:
        raise UploadError("At least one artifact path is required.")

    for path in artifact_paths:
        if not path.is_file():
            raise UploadError(f"Artifact file not found: {path}")

    provided_sha_files: dict[str, Path] = {}
    primary_files: list[Path] = []
    for path in artifact_paths:
        if path.name.endswith(".sha256"):
            provided_sha_files[path.name] = path
        else:
            primary_files.append(path)

    if not primary_files:
        primary_files = list(artifact_paths)
        provided_sha_files.clear()

    uploaded_keys: list[str] = []
    metadata_artifacts: list[dict[str, object]] = []

    for artifact in primary_files:
        sha256_hex, size_bytes = compute_sha256_and_size(artifact)
        artifact_key = f"{prefix}{artifact.name}"
        client.upload_immutable_file(
            artifact_key,
            artifact,
            payload_sha256=sha256_hex,
            size_bytes=size_bytes,
            content_type="application/octet-stream",
        )
        uploaded_keys.append(artifact_key)

        if not artifact.name.endswith(".sha256"):
            sha_filename = f"{artifact.name}.sha256"
            sha_key = f"{prefix}{sha_filename}"
            if sha_filename in provided_sha_files:
                sha_bytes = provided_sha_files[sha_filename].read_bytes()
            else:
                sha_bytes = f"{sha256_hex}  {artifact.name}\n".encode("utf-8")
            client.upload_immutable_bytes(
                sha_key,
                sha_bytes,
                content_type="text/plain; charset=utf-8",
            )
            uploaded_keys.append(sha_key)

        metadata_artifacts.append(
            {
                "filename": artifact.name,
                "sha256": sha256_hex,
                "size_bytes": size_bytes,
            }
        )

    ts = timestamp or dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    primary_entry = metadata_artifacts[0]
    metadata_doc = {
        "commit": commit,
        "run_id": run_id,
        "platform": platform,
        "workflow": workflow,
        "timestamp": ts,
        "sha256": primary_entry["sha256"],
        "size_bytes": primary_entry["size_bytes"],
        "artifacts": metadata_artifacts,
    }
    metadata_bytes = (json.dumps(metadata_doc, indent=2, sort_keys=True) + "\n").encode(
        "utf-8"
    )
    metadata_key = f"{prefix}build-metadata.json"
    client.upload_immutable_bytes(
        metadata_key,
        metadata_bytes,
        content_type="application/json",
    )
    uploaded_keys.append(metadata_key)
    return uploaded_keys


def run_smoke_test(
    client: RustFSClient,
    *,
    commit: str,
    run_id: str,
    workflow: str,
) -> list[str]:
    """Upload and verify a small non-sensitive smoke-test object under ci/smoke/."""
    prefix = build_key_prefix(
        mode="ci",
        platform="smoke",
        commit=commit,
        run_id=run_id,
        smoke_test=True,
    )
    ts = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    payload = (
        json.dumps(
            {
                "smoke_test": True,
                "commit": commit,
                "run_id": run_id,
                "workflow": workflow,
                "timestamp": ts,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode("utf-8")
    key = f"{prefix}smoke-test.json"
    client.upload_immutable_bytes(key, payload, content_type="application/json")
    return [key]


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Upload immutable CI/release artifacts to RustFS via AWS SigV4."
    )
    parser.add_argument(
        "artifacts",
        nargs="*",
        help="Artifact file paths to upload (ignored in --smoke-test mode).",
    )
    parser.add_argument(
        "--mode",
        choices=("ci", "nightly", "releases"),
        default="ci",
        help="Key prefix mode (default: ci).",
    )
    parser.add_argument(
        "--platform",
        default="windows-x64",
        help="Target platform segment (default: windows-x64).",
    )
    parser.add_argument(
        "--commit",
        default=os.environ.get("GITHUB_SHA", "local"),
        help="Git commit SHA (default: $GITHUB_SHA or 'local').",
    )
    parser.add_argument(
        "--run-id",
        default=os.environ.get(
            "GITHUB_RUN_ID",
            dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
        ),
        help="CI run ID (default: $GITHUB_RUN_ID or UTC timestamp).",
    )
    parser.add_argument(
        "--workflow",
        default=os.environ.get("GITHUB_WORKFLOW", "local"),
        help="Workflow name (default: $GITHUB_WORKFLOW or 'local').",
    )
    parser.add_argument(
        "--date",
        default=None,
        help="Date segment for nightly mode (YYYY-MM-DD, default: today UTC).",
    )
    parser.add_argument(
        "--version",
        default=os.environ.get("GITHUB_REF_NAME"),
        help="Release version tag for releases mode (default: $GITHUB_REF_NAME).",
    )
    parser.add_argument(
        "--smoke-test",
        action="store_true",
        help="Upload and verify a small non-sensitive object under ci/smoke/<commit>/<run-id>/.",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    args = parse_args(argv)
    try:
        config = S3Config.from_env()
        client = RustFSClient(config)
        if args.smoke_test:
            keys = run_smoke_test(
                client,
                commit=args.commit,
                run_id=args.run_id,
                workflow=args.workflow,
            )
        else:
            if not args.artifacts:
                raise UploadError("No artifact paths provided.")
            prefix = build_key_prefix(
                mode=args.mode,
                platform=args.platform,
                commit=args.commit,
                run_id=args.run_id,
                date_str=args.date,
                version=args.version,
            )
            keys = upload_artifacts(
                client,
                prefix=prefix,
                artifact_paths=[Path(p) for p in args.artifacts],
                commit=args.commit,
                run_id=args.run_id,
                platform=args.platform,
                workflow=args.workflow,
            )
        LOGGER.info("Completed %d object upload(s) to %s", len(keys), config.endpoint)
        return 0
    except UploadError as exc:
        LOGGER.error("%s", exc)
        return 1
    except Exception as exc:  # pragma: no cover - defensive boundary
        LOGGER.error("Unexpected upload error: %s", type(exc).__name__)
        return 1


if __name__ == "__main__":
    sys.exit(main())
