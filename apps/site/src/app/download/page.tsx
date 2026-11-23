import type { Metadata } from "next";
import { fetchReleaseIndex } from "@/lib/releases/service";
import { DownloadClient } from "@/components/DownloadClient";

export const dynamic = "force-dynamic";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Download ClipForge for Linux x64 and Windows x64 across Stable, Beta, and Nightly release channels.",
};

export default async function DownloadPage() {
  const index = await fetchReleaseIndex();

  return (
    <div className="mx-auto max-w-4xl px-4 py-16 sm:px-6">
      <div className="max-w-2xl">
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          Downloads
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
          Download ClipForge
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
          Select your operating system and release channel. Every official
          artifact is verified with a SHA-256 checksum.
        </p>
      </div>

      <div className="mt-10">
        <DownloadClient
          initialReleases={index.releases}
          configured={index.configured}
        />
      </div>
    </div>
  );
}
