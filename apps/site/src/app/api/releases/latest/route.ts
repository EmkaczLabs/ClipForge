import { NextRequest, NextResponse } from "next/server";
import { LatestReleaseQuerySchema } from "@/lib/releases/schemas";
import { getLatestRelease } from "@/lib/releases/service";

export const dynamic = "force-dynamic";

export async function GET(request: NextRequest) {
  const { searchParams } = request.nextUrl;
  const parsed = LatestReleaseQuerySchema.safeParse({
    channel: searchParams.get("channel") ?? undefined,
    platform: searchParams.get("platform") ?? undefined,
  });

  if (!parsed.success) {
    return NextResponse.json(
      { error: "Invalid query parameters" },
      { status: 400, headers: { "Cache-Control": "no-store" } }
    );
  }

  const result = await getLatestRelease(parsed.data);

  return NextResponse.json(
    {
      configured: result.configured,
      release: result.release,
    },
    {
      status: 200,
      headers: {
        "Cache-Control": "public, s-maxage=60, stale-while-revalidate=120",
      },
    }
  );
}
