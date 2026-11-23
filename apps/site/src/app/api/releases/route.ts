import { NextRequest, NextResponse } from "next/server";
import { ReleasesQuerySchema } from "@/lib/releases/schemas";
import { getFilteredReleases } from "@/lib/releases/service";

export const dynamic = "force-dynamic";

export async function GET(request: NextRequest) {
  const { searchParams } = request.nextUrl;
  const parsed = ReleasesQuerySchema.safeParse({
    channel: searchParams.get("channel") ?? undefined,
    platform: searchParams.get("platform") ?? undefined,
    version: searchParams.get("version") ?? undefined,
  });

  if (!parsed.success) {
    return NextResponse.json(
      { error: "Invalid query parameters" },
      { status: 400, headers: { "Cache-Control": "no-store" } }
    );
  }

  const result = await getFilteredReleases(parsed.data);

  return NextResponse.json(
    {
      configured: result.configured,
      total: result.releases.length,
      releases: result.releases,
    },
    {
      status: 200,
      headers: {
        "Cache-Control": "public, s-maxage=60, stale-while-revalidate=120",
      },
    }
  );
}
