import { describe, expect, it } from "vitest";
import { ReleaseIndexSchema } from "@/lib/releases/schemas";
import { compareVersions } from "@/lib/releases/service";

describe("Release Manifest Schemas & Semver", () => {
  it("validates a well-formed release index", () => {
    const valid = {
      generatedAt: "2026-11-22T00:00:00Z",
      releases: [
        {
          version: "v0.1.0",
          channel: "stable",
          publishedAt: "2026-11-22T00:00:00Z",
          changelog: "Initial release",
          artifacts: [
            {
              id: "clipforge-0.1.0-linux-x64",
              platform: "linux-x64",
              filename: "ClipForge-0.1.0-linux-x64.tar.gz",
              objectKey: "releases/v0.1.0/linux-x64/ClipForge-0.1.0-linux-x64.tar.gz",
              sizeBytes: 12345678,
              sha256: "a".repeat(64),
            },
          ],
        },
      ],
    };

    const parsed = ReleaseIndexSchema.safeParse(valid);
    expect(parsed.success).toBe(true);
  });

  it("rejects ci/ object keys inside manifests", () => {
    const invalid = {
      releases: [
        {
          version: "v0.1.0",
          channel: "stable",
          publishedAt: "2026-11-22T00:00:00Z",
          changelog: "Bad key",
          artifacts: [
            {
              id: "bad-id",
              platform: "windows-x64",
              filename: "bad.zip",
              objectKey: "ci/windows-x64/abc/1/bad.zip",
              sizeBytes: 100,
              sha256: "b".repeat(64),
            },
          ],
        },
      ],
    };

    const parsed = ReleaseIndexSchema.safeParse(invalid);
    expect(parsed.success).toBe(false);
  });

  it("compares versions accurately", () => {
    expect(compareVersions("v0.2.0", "0.1.0")).toBeGreaterThan(0);
    expect(compareVersions("0.1.0", "0.1.0")).toBe(0);
    expect(compareVersions("0.1.0-beta.1", "0.1.0")).toBeLessThan(0);
    expect(compareVersions("0.1.1", "0.1.0")).toBeGreaterThan(0);
  });
});
