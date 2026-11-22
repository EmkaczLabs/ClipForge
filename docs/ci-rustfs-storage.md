# ClipForge CI/CD & RustFS Artifact Storage

ClipForge uses **GitHub Actions** strictly for workflow orchestration, job logs, and status checks, while **RustFS** (`https://s3.emkacz.dev`) serves as the exclusive persistent storage backend for build artifacts. Zero build or release binaries are stored on GitHub Actions (`actions/upload-artifact` and `actions/download-artifact` are not used).

## 1. Infrastructure Topology

- **Linux Runner (`clipforge-linux-builder`)**: Proxmox LXC 116 (Debian 13, `192.168.1.75`, labels `[self-hosted, Linux, X64, clipforge-linux-build]`). Runs formatting, Clippy, Linux workspace tests, and RustFS smoke tests.
- **Windows Runner (`clipforge-win-builder`)**: Proxmox VM 300 (Windows Server 2025, `192.168.1.140`, labels `[self-hosted, Windows, X64, clipforge-windows-build]`). Runs Windows workspace tests and single-job Windows Portable Release builds + packaging + RustFS upload.
- **Fork PRs (`linux-fork-pr`)**: Untrusted fork pull requests run only on GitHub-hosted `ubuntu-24.04` runners with no access to self-hosted runners or GitHub Actions Secrets.
- **RustFS Object Storage**:
  - **Canonical HTTPS Endpoint**: `https://s3.emkacz.dev`
  - **Private Bucket**: `clipforge-builds`
  - **Internal S3 API**: `http://192.168.1.198:9000` (Management console on port `9001`)
  - **Local Reverse Proxy**: `192.168.1.99`

## 2. NAT Loopback & Hosts Mapping

The local network does not support NAT loopback for internal runners reaching `https://s3.emkacz.dev`. Both self-hosted runners resolve `s3.emkacz.dev` directly to the local reverse proxy via their system hosts file:

```text
192.168.1.99 s3.emkacz.dev
```

- Linux: `/etc/hosts`
- Windows: `C:\Windows\System32\drivers\etc\hosts`

This preserves standard TLS certificate verification against `https://s3.emkacz.dev` without disabling SSL/TLS checks.

## 3. Required GitHub Actions Secrets

Configure the following repository secrets in GitHub Settings (`Settings -> Secrets and variables -> Actions`):

- `RUSTFS_ACCESS_KEY_ID`
- `RUSTFS_SECRET_ACCESS_KEY`

**Security rules:**
- Pass secrets only via job/step `env:` blocks; never reference `secrets.*` inside a step `if:` expression.
- Never print, echo, or log credentials or SigV4 `Authorization` headers.
- Never expose secrets to `pull_request` workflows from forks.

## 4. Artifact Directory Layout

All keys in `clipforge-builds` are immutable (uploads fail if `HeadObject` returns `200`):

- **Smoke tests**:
  `ci/smoke-tests/<commit-sha>/<run-id>/smoke-test.json`
- **CI artifacts**:
  `ci/<platform>/<commit-sha>/<run-id>/`
- **Nightly builds**:
  `nightly/<platform>/<YYYY-MM-DD>/<run-id>/`
- **Official tagged releases (`v*`)**:
  `releases/<version>/<platform>/`

Each build upload writes:
1. The artifact archive (e.g. `ClipForge-Windows-x64-portable.zip`)
2. Its checksum sidecar (`ClipForge-Windows-x64-portable.zip.sha256`)
3. `build-metadata.json` (`commit`, `run_id`, `platform`, `workflow`, `timestamp`, `sha256`, `size_bytes`, `artifacts`)

## 5. Manual Retrieval & Checksum Verification

Use `scripts/ci/upload_rustfs.py` (pure Python 3 standard library, AWS SigV4) with `RUSTFS_ACCESS_KEY_ID` and `RUSTFS_SECRET_ACCESS_KEY` exported in your environment:

```bash
export RUSTFS_ENDPOINT="https://s3.emkacz.dev"
export RUSTFS_BUCKET="clipforge-builds"
export RUSTFS_ACCESS_KEY_ID="..."
export RUSTFS_SECRET_ACCESS_KEY="..."

# Download and verify SHA-256 against the expected digest
python3 scripts/ci/upload_rustfs.py \
  --download "ci/windows-x64/<commit-sha>/<run-id>/ClipForge-Windows-x64-portable.zip" \
  --output "./ClipForge-Windows-x64-portable.zip" \
  --expected-sha256 "<sha256-hex>"
```

On Windows PowerShell or Linux, you can also verify the downloaded archive independently against its `.sha256` sidecar:

```bash
sha256sum -c ClipForge-Windows-x64-portable.zip.sha256
```

## 6. Verification & Failure Recovery

- **Smoke test**: Trigger `.github/workflows/rustfs-smoke.yml` (`workflow_dispatch`) to validate `HeadObject` 404, `PutObject`, `HeadObject` 200, `GetObject` SHA-256 integrity, and immutable key overwrite refusal against live RustFS.
- **Upload / verification failure**: `scripts/ci/upload_rustfs.py --verify-download` verifies `HeadObject` 200 + `Content-Length` and streams `GetObject` to verify SHA-256 and byte size. Any mismatch fails the workflow immediately.
- **Re-running a failed job**: Because `ci/` and `nightly/` keys include `<run-id>`, re-running or dispatching a new workflow run writes to a fresh `<run-id>` prefix without colliding with prior objects. For `releases/<version>/<platform>/`, immutable keys prevent silent overwrites of an already-published tag artifact.

## 7. Retention Policy Recommendations

Do **not** enable destructive S3 lifecycle deletion rules without explicit authorization. Recommended retention policy for `clipforge-builds`:

- `ci/` (including `ci/smoke-tests/`): **7 days**
- `nightly/`: **30 days**
- `releases/`: **Indefinite** (never expire)
