import { describe, expect, it } from "vitest";
import {
  assertAllowedReleaseKey,
  isAllowedReleaseKey,
  UnsafeObjectKeyError,
} from "@/lib/s3/key-guard";

describe("S3 Release Key Guard", () => {
  it("allows valid releases/ object keys", () => {
    expect(isAllowedReleaseKey("releases/index.json")).toBe(true);
    expect(
      isAllowedReleaseKey("releases/v0.1.0/linux-x64/ClipForge-Linux-x64.tar.gz")
    ).toBe(true);
    expect(
      isAllowedReleaseKey(
        "releases/v0.2.0-beta.1/windows-x64/ClipForge-Windows-x64-portable.zip"
      )
    ).toBe(true);
  });

  it("rejects ci/, nightly/, and non-releases prefixes", () => {
    expect(isAllowedReleaseKey("ci/windows-x64/abc/123/build.zip")).toBe(false);
    expect(isAllowedReleaseKey("nightly/linux-x64/2026-11-22/1/app.tar.gz")).toBe(false);
    expect(isAllowedReleaseKey("private/secret.json")).toBe(false);
    expect(isAllowedReleaseKey("releases")).toBe(false);
    expect(isAllowedReleaseKey("releases/")).toBe(false);
  });

  it("rejects traversal, encoded dots, backslashes, and control chars", () => {
    expect(isAllowedReleaseKey("releases/../ci/secret.zip")).toBe(false);
    expect(isAllowedReleaseKey("releases/%2e%2e/ci/secret.zip")).toBe(false);
    expect(isAllowedReleaseKey("releases/%2fetc/passwd")).toBe(false);
    expect(isAllowedReleaseKey("releases/v0.1.0\\file.zip")).toBe(false);
    expect(isAllowedReleaseKey("releases//index.json")).toBe(false);
    expect(isAllowedReleaseKey("/releases/index.json")).toBe(false);
    expect(isAllowedReleaseKey("releases/index.json?foo=bar")).toBe(false);
    expect(isAllowedReleaseKey("releases/index.json#frag")).toBe(false);
    expect(isAllowedReleaseKey("releases/bad\nkey")).toBe(false);
  });

  it("throws UnsafeObjectKeyError from assertAllowedReleaseKey", () => {
    expect(() => assertAllowedReleaseKey("ci/build.zip")).toThrow(
      UnsafeObjectKeyError
    );
  });
});
