import { NextResponse } from "next/server";
import { checkStorageHealth } from "@/lib/releases/service";

export const dynamic = "force-dynamic";

export async function GET() {
  const storage = await checkStorageHealth();
  const status =
    storage.configured && storage.reachable === false ? "degraded" : "ok";

  return NextResponse.json(
    {
      status,
      service: "clipforge-site",
      timestamp: new Date().toISOString(),
      storage: {
        configured: storage.configured,
        reachable: storage.reachable,
      },
    },
    {
      status: status === "ok" ? 200 : 503,
      headers: {
        "Cache-Control": "no-store",
      },
    }
  );
}
