#!/usr/bin/env python3
"""Unit and mock-server integration tests for scripts/ci/upload_rustfs.py."""

from __future__ import annotations

import hashlib
import hmac
import http.server
import io
import json
import logging
import os
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import upload_rustfs  # noqa: E402


TEST_ACCESS_KEY = "AKIA_TEST_KEY_123456"
TEST_SECRET_KEY = "super_secret_key_never_log_me_987654"
TEST_BUCKET = "clipforge-builds"
TEST_REGION = "us-east-1"


def verify_sigv4_request(
    *,
    method: str,
    path: str,
    headers: dict[str, str],
    body: bytes,
    access_key: str,
    secret_key: str,
    region: str,
) -> bool:
    """Independently verify an incoming AWS SigV4 request."""
    auth = headers.get("authorization", "")
    if not auth.startswith("AWS4-HMAC-SHA256 "):
        return False

    parts = {}
    for item in auth[len("AWS4-HMAC-SHA256 ") :].split(","):
        k, v = item.strip().split("=", 1)
        parts[k] = v

    cred_parts = parts["Credential"].split("/")
    if len(cred_parts) != 5:
        return False
    req_access_key, date_stamp, req_region, req_service, req_term = cred_parts
    if (
        req_access_key != access_key
        or req_region != region
        or req_service != "s3"
        or req_term != "aws4_request"
    ):
        return False

    signed_headers = parts["SignedHeaders"].split(";")
    canonical_headers = "".join(
        f"{h}:{headers[h].strip()}\n" for h in signed_headers
    )
    payload_hash = hashlib.sha256(body).hexdigest()
    if headers.get("x-amz-content-sha256") != payload_hash:
        return False

    canonical_request = "\n".join(
        [
            method.upper(),
            path,
            "",
            canonical_headers,
            parts["SignedHeaders"],
            payload_hash,
        ]
    )
    amz_date = headers["x-amz-date"]
    scope = f"{date_stamp}/{region}/s3/aws4_request"
    string_to_sign = "\n".join(
        [
            "AWS4-HMAC-SHA256",
            amz_date,
            scope,
            hashlib.sha256(canonical_request.encode("utf-8")).hexdigest(),
        ]
    )

    k_date = hmac.new(
        ("AWS4" + secret_key).encode("utf-8"), date_stamp.encode("utf-8"), hashlib.sha256
    ).digest()
    k_region = hmac.new(k_date, region.encode("utf-8"), hashlib.sha256).digest()
    k_service = hmac.new(k_region, b"s3", hashlib.sha256).digest()
    k_signing = hmac.new(k_service, b"aws4_request", hashlib.sha256).digest()
    expected_sig = hmac.new(
        k_signing, string_to_sign.encode("utf-8"), hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected_sig, parts["Signature"])


class MockS3State:
    """Thread-safe in-memory S3 bucket state for local http.server tests."""

    def __init__(self) -> None:
        self.objects: dict[str, bytes] = {}
        self.requests: list[tuple[str, str]] = []
        self.sigv4_failures: list[str] = []
        self.override_head_length: dict[str, int] = {}
        self.lock = threading.Lock()


def make_handler(state: MockS3State):
    class MockS3Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            return

        def _lower_headers(self) -> dict[str, str]:
            return {k.lower(): v for k, v in self.headers.items()}

        def do_HEAD(self) -> None:  # noqa: N802
            headers = self._lower_headers()
            with state.lock:
                state.requests.append(("HEAD", self.path))
                if not verify_sigv4_request(
                    method="HEAD",
                    path=self.path,
                    headers=headers,
                    body=b"",
                    access_key=TEST_ACCESS_KEY,
                    secret_key=TEST_SECRET_KEY,
                    region=TEST_REGION,
                ):
                    state.sigv4_failures.append(f"HEAD {self.path}")
                    self.send_response(403)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return

                data = state.objects.get(self.path)
                if data is None:
                    self.send_response(404)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return

                reported_len = state.override_head_length.get(self.path, len(data))
                self.send_response(200)
                self.send_header("Content-Length", str(reported_len))
                self.end_headers()

        def do_PUT(self) -> None:  # noqa: N802
            headers = self._lower_headers()
            length = int(headers.get("content-length", "0"))
            body = self.rfile.read(length) if length > 0 else b""
            with state.lock:
                state.requests.append(("PUT", self.path))
                if not verify_sigv4_request(
                    method="PUT",
                    path=self.path,
                    headers=headers,
                    body=body,
                    access_key=TEST_ACCESS_KEY,
                    secret_key=TEST_SECRET_KEY,
                    region=TEST_REGION,
                ):
                    state.sigv4_failures.append(f"PUT {self.path}")
                    self.send_response(403)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return

                state.objects[self.path] = body
                self.send_response(200)
                self.send_header("Content-Length", "0")
                self.end_headers()

        def do_GET(self) -> None:  # noqa: N802
            headers = self._lower_headers()
            with state.lock:
                state.requests.append(("GET", self.path))
                if not verify_sigv4_request(
                    method="GET",
                    path=self.path,
                    headers=headers,
                    body=b"",
                    access_key=TEST_ACCESS_KEY,
                    secret_key=TEST_SECRET_KEY,
                    region=TEST_REGION,
                ):
                    state.sigv4_failures.append(f"GET {self.path}")
                    self.send_response(403)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return

                data = state.objects.get(self.path)
                if data is None:
                    self.send_response(404)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return

                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.send_header("Content-Type", "application/octet-stream")
                self.end_headers()
                self.wfile.write(data)

    return MockS3Handler


class UploadRustFSTests(unittest.TestCase):
    def setUp(self) -> None:
        self.state = MockS3State()
        self.server = http.server.ThreadingHTTPServer(
            ("127.0.0.1", 0), make_handler(self.state)
        )
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.endpoint = f"http://127.0.0.1:{self.port}"
        self.config = upload_rustfs.S3Config(
            endpoint=self.endpoint,
            bucket=TEST_BUCKET,
            region=TEST_REGION,
            access_key_id=TEST_ACCESS_KEY,
            secret_access_key=TEST_SECRET_KEY,
        )
        self.client = upload_rustfs.RustFSClient(self.config)

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def test_upload_artifacts_sigv4_metadata_and_verification(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            artifact = Path(tmpdir) / "ClipForge-Windows-x64-portable.zip"
            payload = b"PK\x03\x04dummy-portable-zip-bytes" * 64
            artifact.write_bytes(payload)
            expected_sha = hashlib.sha256(payload).hexdigest()

            prefix = upload_rustfs.build_key_prefix(
                mode="ci",
                platform="windows-x64",
                commit="abc1234",
                run_id="99001",
            )
            keys = upload_rustfs.upload_artifacts(
                self.client,
                prefix=prefix,
                artifact_paths=[artifact],
                commit="abc1234",
                run_id="99001",
                platform="windows-x64",
                workflow="Windows Portable Release",
                timestamp="2026-11-22T23:00:00Z",
            )

            self.assertEqual(self.state.sigv4_failures, [])
            self.assertEqual(
                keys,
                [
                    "ci/windows-x64/abc1234/99001/ClipForge-Windows-x64-portable.zip",
                    "ci/windows-x64/abc1234/99001/ClipForge-Windows-x64-portable.zip.sha256",
                    "ci/windows-x64/abc1234/99001/build-metadata.json",
                ],
            )

            obj_base = f"/{TEST_BUCKET}/ci/windows-x64/abc1234/99001"
            self.assertEqual(
                self.state.objects[f"{obj_base}/ClipForge-Windows-x64-portable.zip"],
                payload,
            )
            self.assertEqual(
                self.state.objects[f"{obj_base}/ClipForge-Windows-x64-portable.zip.sha256"].decode(
                    "utf-8"
                ),
                f"{expected_sha}  ClipForge-Windows-x64-portable.zip\n",
            )

            metadata = json.loads(
                self.state.objects[f"{obj_base}/build-metadata.json"].decode("utf-8")
            )
            self.assertEqual(metadata["commit"], "abc1234")
            self.assertEqual(metadata["run_id"], "99001")
            self.assertEqual(metadata["platform"], "windows-x64")
            self.assertEqual(metadata["workflow"], "Windows Portable Release")
            self.assertEqual(metadata["timestamp"], "2026-11-22T23:00:00Z")
            self.assertEqual(metadata["sha256"], expected_sha)
            self.assertEqual(metadata["size_bytes"], len(payload))

            # Verify 404 HEAD -> PUT -> 200 HEAD sequence for each object
            zip_path = f"{obj_base}/ClipForge-Windows-x64-portable.zip"
            zip_ops = [op for op, p in self.state.requests if p == zip_path]
            self.assertEqual(zip_ops, ["HEAD", "PUT", "HEAD"])

    def test_immutability_refuses_overwrite_when_head_returns_200(self) -> None:
        key = "ci/windows-x64/abc1234/99001/existing.bin"
        self.state.objects[f"/{TEST_BUCKET}/{key}"] = b"original"

        with self.assertRaises(upload_rustfs.UploadError) as ctx:
            self.client.upload_immutable_bytes(key, b"overwrite-attempt")

        self.assertIn("Refusing to overwrite immutable object", str(ctx.exception))
        # Ensure PUT was never sent
        put_ops = [op for op, p in self.state.requests if op == "PUT" and p == f"/{TEST_BUCKET}/{key}"]
        self.assertEqual(put_ops, [])
        self.assertEqual(self.state.objects[f"/{TEST_BUCKET}/{key}"], b"original")

    def test_post_upload_head_content_length_mismatch_fails(self) -> None:
        key = "ci/windows-x64/abc1234/99002/badlen.bin"
        self.state.override_head_length[f"/{TEST_BUCKET}/{key}"] = 999999

        with self.assertRaises(upload_rustfs.UploadError) as ctx:
            self.client.upload_immutable_bytes(key, b"actual-bytes")

        self.assertIn("Content-Length mismatch", str(ctx.exception))

    def test_prefixes_and_smoke_test(self) -> None:
        self.assertEqual(
            upload_rustfs.build_key_prefix(
                mode="nightly",
                platform="linux-x64",
                commit="deadbeef",
                run_id="42",
                date_str="2026-11-22",
            ),
            "nightly/linux-x64/2026-11-22/42/",
        )
        self.assertEqual(
            upload_rustfs.build_key_prefix(
                mode="releases",
                platform="windows-x64",
                commit="deadbeef",
                run_id="42",
                version="v0.2.0",
            ),
            "releases/v0.2.0/windows-x64/",
        )
        self.assertEqual(
            upload_rustfs.build_key_prefix(
                mode="ci",
                platform="windows-x64",
                commit="deadbeef",
                run_id="42",
                smoke_test=True,
            ),
            "ci/smoke-tests/deadbeef/42/",
        )

        smoke_keys = upload_rustfs.run_smoke_test(
            self.client,
            commit="deadbeef",
            run_id="42",
            workflow="smoke",
        )
        self.assertEqual(smoke_keys, ["ci/smoke-tests/deadbeef/42/smoke-test.json"])
        self.assertEqual(self.state.sigv4_failures, [])

    def test_verify_download_and_cli_download(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            artifact = Path(tmpdir) / "bundle.zip"
            payload = b"PK\x03\x04rustfs-download-test" * 32
            artifact.write_bytes(payload)
            expected_sha = hashlib.sha256(payload).hexdigest()

            prefix = upload_rustfs.build_key_prefix(
                mode="ci",
                platform="windows-x64",
                commit="def4567",
                run_id="555",
            )
            upload_rustfs.upload_artifacts(
                self.client,
                prefix=prefix,
                artifact_paths=[artifact],
                commit="def4567",
                run_id="555",
                platform="windows-x64",
                workflow="CI",
                verify_download=True,
            )

            out_file = Path(tmpdir) / "downloaded" / "bundle.zip"
            env = {
                "RUSTFS_ENDPOINT": self.endpoint,
                "RUSTFS_BUCKET": TEST_BUCKET,
                "RUSTFS_REGION": TEST_REGION,
                "RUSTFS_ACCESS_KEY_ID": TEST_ACCESS_KEY,
                "RUSTFS_SECRET_ACCESS_KEY": TEST_SECRET_KEY,
            }
            with mock.patch.dict(os.environ, env, clear=False):
                rc = upload_rustfs.main(
                    [
                        "--download",
                        f"{prefix}bundle.zip",
                        "--output",
                        str(out_file),
                        "--expected-sha256",
                        expected_sha,
                    ]
                )
                self.assertEqual(rc, 0)
            self.assertEqual(out_file.read_bytes(), payload)

    def test_zero_secret_leakage_in_logs_and_errors(self) -> None:
        log_stream = io.StringIO()
        handler = logging.StreamHandler(log_stream)
        logger = logging.getLogger("upload_rustfs")
        logger.addHandler(handler)
        logger.setLevel(logging.DEBUG)
        try:
            env = {
                "RUSTFS_ENDPOINT": self.endpoint,
                "RUSTFS_BUCKET": TEST_BUCKET,
                "RUSTFS_REGION": TEST_REGION,
                "RUSTFS_ACCESS_KEY_ID": TEST_ACCESS_KEY,
                "RUSTFS_SECRET_ACCESS_KEY": TEST_SECRET_KEY,
            }
            with mock.patch.dict(os.environ, env, clear=False):
                rc = upload_rustfs.main(
                    ["--smoke-test", "--commit", "sec123", "--run-id", "77"]
                )
                self.assertEqual(rc, 0)
                # Second run hits immutability error; ensure secrets still don't leak
                rc_dup = upload_rustfs.main(
                    ["--smoke-test", "--commit", "sec123", "--run-id", "77"]
                )
                self.assertEqual(rc_dup, 1)
        finally:
            logger.removeHandler(handler)

        output = log_stream.getvalue()
        self.assertNotIn(TEST_ACCESS_KEY, output)
        self.assertNotIn(TEST_SECRET_KEY, output)
        self.assertNotIn("Authorization", output)
        self.assertNotIn("Signature=", output)


if __name__ == "__main__":
    unittest.main()
