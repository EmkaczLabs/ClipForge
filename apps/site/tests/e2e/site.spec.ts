import { expect, test } from "@playwright/test";

const PAGES = [
  { path: "/", heading: /Capture the moment/i },
  { path: "/features", heading: /Features & Implementation Status/i },
  { path: "/download", heading: /Download ClipForge/i },
  { path: "/changelog", heading: /Changelog/i },
  { path: "/roadmap", heading: /Project Roadmap/i },
  { path: "/docs", heading: /ClipForge Docs/i },
  { path: "/docs/installation", heading: /Installation/i },
  { path: "/server", heading: /ClipForge-Server/i },
  { path: "/open-source", heading: /Open Source/i },
  { path: "/privacy", heading: /Privacy by Architecture/i },
  { path: "/status", heading: /Service Health/i },
];

test.describe("ClipForge Site E2E", () => {
  for (const pageDef of PAGES) {
    test(`renders ${pageDef.path} cleanly without console or page errors`, async ({
      page,
    }) => {
      const errors: string[] = [];
      page.on("pageerror", (err) => errors.push(err.message));
      page.on("console", (msg) => {
        if (msg.type() === "error") {
          errors.push(msg.text());
        }
      });

      const response = await page.goto(pageDef.path);
      expect(response?.status()).toBe(200);
      await expect(
        page.getByRole("heading", { level: 1, name: pageDef.heading })
      ).toBeVisible();
      expect(errors).toEqual([]);
    });
  }

  test("docs sidebar search filters documentation articles", async ({ page }) => {
    await page.goto("/docs");
    const searchInput = page.getByRole("searchbox", {
      name: /Search documentation/i,
    });
    await searchInput.fill("Wayland");
    const sidebarNav = page.getByRole("navigation", {
      name: "Documentation sections",
    });
    await expect(
      sidebarNav.getByRole("link", { name: "Troubleshooting", exact: true })
    ).toBeVisible();
    await expect(
      sidebarNav.getByRole("link", { name: "FAQ", exact: true })
    ).toHaveCount(0);
  });

  test("download page switches platforms and channels", async ({ page }) => {
    await page.goto("/download");
    await page.getByRole("button", { name: "Windows (x64)" }).click();
    await page.getByRole("button", { name: "beta" }).click();
    await expect(page.getByText(/windows-x64|unconfigured/i)).toBeVisible();
  });

  test("API routes respond cleanly and reject arbitrary download keys", async ({
    request,
  }) => {
    const health = await request.get("/api/health");
    expect(health.status()).toBe(200);
    const healthJson = await health.json();
    expect(healthJson.service).toBe("clipforge-site");

    const releases = await request.get("/api/releases?channel=stable");
    expect(releases.status()).toBe(200);

    const latest = await request.get("/api/releases/latest?channel=stable");
    expect(latest.status()).toBe(200);

    const updates = await request.get(
      "/api/updates/latest?version=0.1.0&platform=linux-x64&channel=stable"
    );
    expect(updates.status()).toBe(200);

    const badDownload = await request.get("/api/download/..%2Fci%2Fsecret.zip");
    expect([400, 404]).toContain(badDownload.status());

    const invalidChannel = await request.get("/api/releases?channel=hacked");
    expect(invalidChannel.status()).toBe(400);
  });
});
