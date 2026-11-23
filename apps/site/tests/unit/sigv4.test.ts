import { describe, expect, it } from "vitest";
import { createPresignedGetUrl, signS3Request } from "@/lib/s3/sigv4";
import { UnsafeObjectKeyError } from "@/lib/s3/key-guard";

const TEST_CREDS = {
  accessKeyId: "AKIAIOSFODNN7EXAMPLE",
  secretAccessKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  region: "us-east-1",
};

describe("Pure Node.js AWS SigV4 Client", () => {
  it("generates valid SigV4 Authorization headers for releases/ keys", () => {
    const fixedDate = new Date("2026-11-22T12:00:00.000Z");
    const signed = signS3Request({
      method: "GET",
      endpoint: "https://s3.emkacz.dev",
      bucket: "clipforge-builds",
      key: "releases/index.json",
      credentials: TEST_CREDS,
      now: fixedDate,
    });

    expect(signed.url).toBe("https://s3.emkacz.dev/clipforge-builds/releases/index.json");
    expect(signed.headers.host).toBe("s3.emkacz.dev");
    expect(signed.headers["x-amz-date"]).toBe("20261122T120000Z");
    expect(signed.headers.authorization).toMatch(
      /^AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE\/20261122\/us-east-1\/s3\/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=[a-f0-9]{64}$/
    );
  });

  it("produces deterministic signatures for identical inputs", () => {
    const fixedDate = new Date("2026-11-22T12:00:00.000Z");
    const a = signS3Request({
      method: "HEAD",
      endpoint: "https://s3.emkacz.dev",
      bucket: "clipforge-builds",
      key: "releases/v0.1.0/manifest.json",
      credentials: TEST_CREDS,
      now: fixedDate,
    });
    const b = signS3Request({
      method: "HEAD",
      endpoint: "https://s3.emkacz.dev",
      bucket: "clipforge-builds",
      key: "releases/v0.1.0/manifest.json",
      credentials: TEST_CREDS,
      now: fixedDate,
    });

    expect(a.headers.authorization).toBe(b.headers.authorization);
  });

  it("refuses to sign non-HTTPS endpoints or non-release keys", () => {
    expect(() =>
      signS3Request({
        method: "GET",
        endpoint: "http://192.168.1.198:9000",
        bucket: "clipforge-builds",
        key: "releases/index.json",
        credentials: TEST_CREDS,
      })
    ).toThrow("S3 endpoint must use HTTPS");

    expect(() =>
      signS3Request({
        method: "GET",
        endpoint: "https://s3.emkacz.dev",
        bucket: "clipforge-builds",
        key: "ci/windows-x64/1/2/artifact.zip",
        credentials: TEST_CREDS,
      })
    ).toThrow(UnsafeObjectKeyError);
  });

  it("creates bounded short-lived presigned URLs", () => {
    const fixedDate = new Date("2026-11-22T12:00:00.000Z");
    const url = createPresignedGetUrl({
      endpoint: "https://s3.emkacz.dev",
      bucket: "clipforge-builds",
      key: "releases/v0.1.0/linux-x64/ClipForge.tar.gz",
      credentials: TEST_CREDS,
      expiresInSeconds: 99999,
      now: fixedDate,
    });

    const parsed = new URL(url);
    expect(parsed.origin).toBe("https://s3.emkacz.dev");
    expect(parsed.searchParams.get("X-Amz-Expires")).toBe("300");
    expect(parsed.searchParams.get("X-Amz-Algorithm")).toBe("AWS4-HMAC-SHA256");
    expect(parsed.searchParams.get("X-Amz-Signature")).toMatch(/^[a-f0-9]{64}$/);
  });
});
