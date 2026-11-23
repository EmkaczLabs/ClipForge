import { NextRequest, NextResponse } from "next/server";
import { SafeReleaseIdSchema } from "@/lib/releases/schemas";
import { findApprovedArtifactById } from "@/lib/releases/service";
import { RustFSClient } from "@/lib/s3/client";
import { isAllowedReleaseKey } from "@/lib/s3/key-guard";

export const dynamic = "force-dynamic";

export async function GET(
  request: NextRequest,
  context: { params: Promise<{ releaseId: string }> }
) {
  const { releaseId } = await context.params;
  const parsed = SafeReleaseIdSchema.safeParse(releaseId);

  if (!parsed.success) {
    return NextResponse.json(
      { error: "Invalid releaseId" },
      { status: 400, headers: { "Cache-Control": "no-store" } }
    );
  }

  const lookup = await findApprovedArtifactById(parsed.data);
  if (!lookup.configured) {
    return NextResponse.json(
      { error: "Release storage is not configured" },
      { status: 503, headers: { "Cache-Control": "no-store" } }
    );
  }

  if (!lookup.artifact) {
    return NextResponse.json(
      { error: "Release artifact not found" },
      { status: 404, headers: { "Cache-Control": "no-store" } }
    );
  }

  const { artifact } = lookup;
  if (!isAllowedReleaseKey(artifact.objectKey)) {
    return NextResponse.json(
      { error: "Forbidden artifact key" },
      { status: 403, headers: { "Cache-Control": "no-store" } }
    );
  }

  const client = new RustFSClient();
  const mode = request.nextUrl.searchParams.get("mode");

  if (mode === "presign") {
    const url = client.createPresignedDownloadUrl(artifact.objectKey, 120);
    return NextResponse.json(
      {
        url,
        expiresInSeconds: 120,
        filename: artifact.filename,
        sha256: artifact.sha256,
        sizeBytes: artifact.sizeBytes,
      },
      { status: 200, headers: { "Cache-Control": "no-store" } }
    );
  }

  try {
    const upstream = await client.getObjectStream(artifact.objectKey);
    if (!upstream || !upstream.body) {
      return NextResponse.json(
        { error: "Release artifact not found in storage" },
        { status: 404, headers: { "Cache-Control": "no-store" } }
      );
    }

    const headers = new Headers();
    headers.set("Content-Type", artifact.contentType || "application/octet-stream");
    headers.set(
      "Content-Disposition",
      `attachment; filename="${artifact.filename}"`
    );
    headers.set("X-Content-Type-Options", "nosniff");
    headers.set("Cache-Control", "private, no-store");
    if (artifact.sizeBytes > 0) {
      headers.set("Content-Length", String(artifact.sizeBytes));
    }

    return new NextResponse(upstream.body, {
      status: 200,
      headers,
    });
  } catch {
    return NextResponse.json(
      { error: "Unable to retrieve release artifact" },
      { status: 502, headers: { "Cache-Control": "no-store" } }
    );
  }
}
