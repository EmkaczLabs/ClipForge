import { NextRequest, NextResponse } from "next/server";
import { UpdateCheckQuerySchema } from "@/lib/releases/schemas";
import { compareVersions, getLatestRelease } from "@/lib/releases/service";

export const dynamic = "force-dynamic";

export async function GET(request: NextRequest) {
  const { searchParams } = request.nextUrl;
  const parsed = UpdateCheckQuerySchema.safeParse({
    version: searchParams.get("version") ?? undefined,
    platform: searchParams.get("platform") ?? undefined,
    channel: searchParams.get("channel") ?? "stable",
  });

  if (!parsed.success) {
    return NextResponse.json(
      { error: "Invalid update check parameters" },
      { status: 400, headers: { "Cache-Control": "no-store" } }
    );
  }

  const { version: currentVersion, platform, channel } = parsed.data;
  const latest = await getLatestRelease({ channel, platform });

  if (!latest.configured || !latest.release) {
    return NextResponse.json(
      {
        configured: latest.configured,
        updateAvailable: false,
        currentVersion,
        latestVersion: null,
        releaseDate: null,
        notes: null,
        downloadUrl: null,
        sha256: null,
        sizeBytes: null,
      },
      { status: 200, headers: { "Cache-Control": "no-store" } }
    );
  }

  const artifact = latest.release.artifacts.find((a) => a.platform === platform);
  const updateAvailable =
    Boolean(artifact) &&
    compareVersions(latest.release.version, currentVersion) > 0;

  return NextResponse.json(
    {
      configured: true,
      updateAvailable,
      currentVersion,
      latestVersion: latest.release.version,
      releaseDate: latest.release.publishedAt,
      notes: latest.release.changelog,
      downloadUrl: artifact ? `/api/download/${encodeURIComponent(artifact.id)}` : null,
      sha256: artifact?.sha256 ?? null,
      sizeBytes: artifact?.sizeBytes ?? null,
    },
    {
      status: 200,
      headers: {
        "Cache-Control": "public, s-maxage=60, stale-while-revalidate=120",
      },
    }
  );
}
