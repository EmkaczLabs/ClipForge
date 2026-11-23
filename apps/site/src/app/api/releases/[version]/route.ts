import { NextRequest, NextResponse } from "next/server";
import { SafeVersionSchema } from "@/lib/releases/schemas";
import { getReleaseByVersion } from "@/lib/releases/service";

export const dynamic = "force-dynamic";

export async function GET(
  _request: NextRequest,
  context: { params: Promise<{ version: string }> }
) {
  const { version } = await context.params;
  const parsed = SafeVersionSchema.safeParse(version);

  if (!parsed.success) {
    return NextResponse.json(
      { error: "Invalid version parameter" },
      { status: 400, headers: { "Cache-Control": "no-store" } }
    );
  }

  const result = await getReleaseByVersion(parsed.data);

  if (!result.release) {
    return NextResponse.json(
      { error: "Release not found", configured: result.configured },
      { status: 404, headers: { "Cache-Control": "no-store" } }
    );
  }

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
