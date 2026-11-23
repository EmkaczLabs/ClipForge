import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NextRequest } from "next/server";
import { GET as getHealth } from "@/app/api/health/route";
import { GET as getReleases } from "@/app/api/releases/route";
import { GET as getLatestRelease } from "@/app/api/releases/latest/route";
import { GET as getReleaseByVersion } from "@/app/api/releases/[version]/route";
import { GET as getDownload } from "@/app/api/download/[releaseId]/route";
import { GET as getUpdatesLatest } from "@/app/api/updates/latest/route";
import { clearReleaseCache } from "@/lib/releases/service";

const MOCK_INDEX = {
  generatedAt: "2026-11-22T12:00:00Z",
  releases: [
    {
      version: "0.2.0",
      channel: "stable",
      publishedAt: "2026-11-22T12:00:00Z",
      changelog: "Added Windows portable ZIP and Fast Trim.",
      artifacts: [
        {
          id: "clipforge-0.2.0-linux-x64",
          platform: "linux-x64",
          filename: "ClipForge-0.2.0-linux-x64.tar.gz",
          objectKey: "releases/0.2.0/linux-x64/ClipForge-0.2.0-linux-x64.tar.gz",
          sizeBytes: 11,
          sha256: "a".repeat(64),
          contentType: "application/gzip",
        },
        {
          id: "clipforge-0.2.0-windows-x64",
          platform: "windows-x64",
          filename: "ClipForge-0.2.0-windows-x64.zip",
          objectKey: "releases/0.2.0/windows-x64/ClipForge-0.2.0-windows-x64.zip",
          sizeBytes: 11,
          sha256: "b".repeat(64),
          contentType: "application/zip",
        },
      ],
    },
    {
      version: "0.3.0-beta.1",
      channel: "beta",
      publishedAt: "2026-11-22T14:00:00Z",
      changelog: "Beta preview.",
      artifacts: [
        {
          id: "clipforge-0.3.0-beta.1-linux-x64",
          platform: "linux-x64",
          filename: "ClipForge-0.3.0-beta.1-linux-x64.tar.gz",
          objectKey:
            "releases/0.3.0-beta.1/linux-x64/ClipForge-0.3.0-beta.1-linux-x64.tar.gz",
          sizeBytes: 11,
          sha256: "c".repeat(64),
          contentType: "application/gzip",
        },
      ],
    },
  ],
};

describe("API Route Handlers", () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    clearReleaseCache();
    delete process.env.RUSTFS_RELEASE_ACCESS_KEY_ID;
    delete process.env.RUSTFS_RELEASE_SECRET_ACCESS_KEY;
  });

  afterEach(() => {
    process.env = { ...originalEnv };
    vi.unstubAllGlobals();
    clearReleaseCache();
  });

  it("handles unconfigured RustFS gracefully across all endpoints", async () => {
    const healthRes = await getHealth();
    expect(healthRes.status).toBe(200);
    const healthBody = await healthRes.json();
    expect(healthBody.status).toBe("ok");
    expect(healthBody.storage.configured).toBe(false);
    expect(JSON.stringify(healthBody)).not.toContain("192.168");

    const relRes = await getReleases(
      new NextRequest("http://localhost/api/releases")
    );
    expect(relRes.status).toBe(200);
    const relBody = await relRes.json();
    expect(relBody.configured).toBe(false);
    expect(relBody.releases).toEqual([]);

    const dlRes = await getDownload(
      new NextRequest("http://localhost/api/download/clipforge-0.2.0-linux-x64"),
      { params: Promise.resolve({ releaseId: "clipforge-0.2.0-linux-x64" }) }
    );
    expect(dlRes.status).toBe(503);
  });

  it("filters releases, returns latest, version detail, updates, and streams approved download", async () => {
    process.env.RUSTFS_RELEASE_ACCESS_KEY_ID = "TEST_KEY";
    process.env.RUSTFS_RELEASE_SECRET_ACCESS_KEY = "TEST_SECRET";

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/releases/index.json")) {
        if (init?.method === "HEAD") {
          return new Response(null, { status: 200, headers: { "content-length": "512" } });
        }
        return new Response(JSON.stringify(MOCK_INDEX), { status: 200 });
      }
      if (url.endsWith("/releases/0.2.0/linux-x64/ClipForge-0.2.0-linux-x64.tar.gz")) {
        return new Response("hello-world", { status: 200 });
      }
      return new Response("Not Found", { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);

    // /api/releases?channel=stable&platform=windows-x64
    const relRes = await getReleases(
      new NextRequest(
        "http://localhost/api/releases?channel=stable&platform=windows-x64"
      )
    );
    expect(relRes.status).toBe(200);
    const relBody = await relRes.json();
    expect(relBody.total).toBe(1);
    expect(relBody.releases[0].artifacts).toHaveLength(1);
    expect(relBody.releases[0].artifacts[0].platform).toBe("windows-x64");

    // /api/releases/latest?channel=beta
    const latestRes = await getLatestRelease(
      new NextRequest("http://localhost/api/releases/latest?channel=beta")
    );
    expect(latestRes.status).toBe(200);
    const latestBody = await latestRes.json();
    expect(latestBody.release.version).toBe("0.3.0-beta.1");

    // /api/releases/0.2.0
    const verRes = await getReleaseByVersion(
      new NextRequest("http://localhost/api/releases/0.2.0"),
      { params: Promise.resolve({ version: "0.2.0" }) }
    );
    expect(verRes.status).toBe(200);

    // /api/updates/latest?version=0.1.0&platform=linux-x64&channel=stable
    const updRes = await getUpdatesLatest(
      new NextRequest(
        "http://localhost/api/updates/latest?version=0.1.0&platform=linux-x64&channel=stable"
      )
    );
    expect(updRes.status).toBe(200);
    const updBody = await updRes.json();
    expect(updBody.updateAvailable).toBe(true);
    expect(updBody.latestVersion).toBe("0.2.0");
    expect(updBody.downloadUrl).toBe("/api/download/clipforge-0.2.0-linux-x64");

    // /api/download/clipforge-0.2.0-linux-x64
    const dlRes = await getDownload(
      new NextRequest("http://localhost/api/download/clipforge-0.2.0-linux-x64"),
      { params: Promise.resolve({ releaseId: "clipforge-0.2.0-linux-x64" }) }
    );
    expect(dlRes.status).toBe(200);
    expect(dlRes.headers.get("content-disposition")).toContain(
      "ClipForge-0.2.0-linux-x64.tar.gz"
    );
    expect(await dlRes.text()).toBe("hello-world");
  });

  it("rejects arbitrary keys, traversal, and unknown releaseIds", async () => {
    process.env.RUSTFS_RELEASE_ACCESS_KEY_ID = "TEST_KEY";
    process.env.RUSTFS_RELEASE_SECRET_ACCESS_KEY = "TEST_SECRET";

    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(MOCK_INDEX), { status: 200 }))
    );

    const badId = await getDownload(
      new NextRequest("http://localhost/api/download/ci%2Fsecret"),
      { params: Promise.resolve({ releaseId: "ci/secret" }) }
    );
    expect(badId.status).toBe(400);

    const missingId = await getDownload(
      new NextRequest("http://localhost/api/download/nonexistent-id"),
      { params: Promise.resolve({ releaseId: "nonexistent-id" }) }
    );
    expect(missingId.status).toBe(404);

    const badQuery = await getReleases(
      new NextRequest("http://localhost/api/releases?channel=invalid")
    );
    expect(badQuery.status).toBe(400);
  });
});
