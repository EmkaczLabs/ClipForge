"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import { Download, Check, Copy, Terminal, ShieldCheck, AlertCircle } from "lucide-react";
import type {
  ReleaseChannel,
  ReleaseEntry,
  ReleasePlatform,
} from "@/lib/releases/schemas";

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let idx = 0;
  let val = bytes;
  while (val >= 1024 && idx < units.length - 1) {
    val /= 1024;
    idx++;
  }
  return `${val.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

export function DownloadClient({
  initialReleases,
  configured,
}: {
  initialReleases: ReleaseEntry[];
  configured: boolean;
}) {
  const [platform, setPlatform] = useState<ReleasePlatform>("linux-x64");
  const [channel, setChannel] = useState<ReleaseChannel>("stable");
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const matchingReleases = useMemo(() => {
    return initialReleases
      .filter((rel) => rel.channel === channel)
      .map((rel) => ({
        ...rel,
        artifact: rel.artifacts.find((a) => a.platform === platform) ?? null,
      }))
      .filter((rel) => rel.artifact !== null);
  }, [initialReleases, channel, platform]);

  const latest = matchingReleases[0] ?? null;

  async function copySha(id: string, sha256: string) {
    try {
      await navigator.clipboard.writeText(sha256);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch {
      // Clipboard API may be unavailable in restricted contexts
    }
  }

  return (
    <div className="space-y-8">
      {/* Platform & Channel Selectors */}
      <div className="flex flex-col justify-between gap-4 rounded-2xl border border-white/[0.08] bg-[#111114] p-4 sm:flex-row sm:items-center sm:p-5">
        <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Select platform">
          {(
            [
              { id: "linux-x64", label: "Linux (x64)" },
              { id: "windows-x64", label: "Windows (x64)" },
            ] as const
          ).map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => setPlatform(item.id)}
              className={`rounded-xl px-4 py-2 text-sm font-medium transition-colors ${
                platform === item.id
                  ? "bg-violet-600 text-white"
                  : "border border-white/[0.08] bg-[#0b0b0e] text-zinc-400 hover:text-zinc-200"
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>

        <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Select release channel">
          {(["stable", "beta", "nightly"] as const).map((ch) => (
            <button
              key={ch}
              type="button"
              onClick={() => setChannel(ch)}
              className={`rounded-lg px-3 py-1.5 text-xs font-medium capitalize transition-colors ${
                channel === ch
                  ? "border border-violet-500/40 bg-violet-500/15 text-violet-300"
                  : "border border-white/[0.07] bg-[#0b0b0e] text-zinc-400 hover:text-zinc-200"
              }`}
            >
              {ch}
            </button>
          ))}
        </div>
      </div>

      {/* Latest Release Card or Clean Fallback */}
      {latest && latest.artifact ? (
        <div className="rounded-2xl border border-violet-500/30 bg-[#111114] p-6 sm:p-8">
          <div className="flex flex-col justify-between gap-6 lg:flex-row lg:items-center">
            <div className="space-y-2">
              <div className="flex flex-wrap items-center gap-2">
                <span className="rounded-full border border-violet-500/30 bg-violet-500/15 px-2.5 py-0.5 font-mono text-xs text-violet-300">
                  {latest.version}
                </span>
                <span className="rounded-full border border-white/10 bg-white/5 px-2.5 py-0.5 text-xs capitalize text-zinc-300">
                  {latest.channel}
                </span>
                <span className="text-xs text-zinc-500">
                  Released {new Date(latest.publishedAt).toLocaleDateString("en-US", {
                    year: "numeric",
                    month: "short",
                    day: "numeric",
                  })}
                </span>
              </div>
              <h2 className="text-xl font-semibold text-white">
                {latest.artifact.filename}
              </h2>
              <p className="text-sm text-zinc-400">
                Size: <span className="font-mono text-zinc-200">{formatBytes(latest.artifact.sizeBytes)}</span>
              </p>
            </div>

            <a
              href={`/api/download/${encodeURIComponent(latest.artifact.id)}`}
              className="inline-flex shrink-0 items-center justify-center gap-2 rounded-xl bg-violet-600 px-6 py-3 text-sm font-medium text-white shadow-[0_0_24px_rgba(139,92,246,0.3)] transition-colors hover:bg-violet-500"
            >
              <Download className="h-4 w-4" aria-hidden="true" />
              <span>Download for {platform === "linux-x64" ? "Linux" : "Windows"}</span>
            </a>
          </div>

          <div className="mt-6 rounded-xl border border-white/[0.07] bg-[#0b0b0e] p-4">
            <div className="flex items-center justify-between gap-2">
              <span className="inline-flex items-center gap-1.5 text-xs font-medium text-zinc-400">
                <ShieldCheck className="h-3.5 w-3.5 text-emerald-400" aria-hidden="true" />
                SHA-256 Checksum
              </span>
              <button
                type="button"
                onClick={() => copySha(latest.artifact!.id, latest.artifact!.sha256)}
                className="inline-flex items-center gap-1 rounded-md border border-white/10 px-2 py-1 text-xs text-zinc-300 hover:bg-white/5"
              >
                {copiedId === latest.artifact.id ? (
                  <>
                    <Check className="h-3 w-3 text-emerald-400" aria-hidden="true" />
                    <span>Copied</span>
                  </>
                ) : (
                  <>
                    <Copy className="h-3 w-3" aria-hidden="true" />
                    <span>Copy</span>
                  </>
                )}
              </button>
            </div>
            <p className="mt-2 break-all font-mono text-xs text-zinc-300">
              {latest.artifact.sha256}
            </p>
          </div>

          {latest.changelog && (
            <div className="mt-6 border-t border-white/[0.07] pt-5">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
                Release Notes
              </h3>
              <p className="mt-2 whitespace-pre-line text-sm leading-relaxed text-zinc-300">
                {latest.changelog}
              </p>
            </div>
          )}
        </div>
      ) : (
        <div className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6 sm:p-8">
          <div className="flex items-start gap-3">
            <AlertCircle className="mt-0.5 h-5 w-5 shrink-0 text-violet-400" aria-hidden="true" />
            <div className="space-y-3">
              <h2 className="text-lg font-semibold text-white">
                {configured
                  ? `No ${channel} releases published for ${platform} yet`
                  : "Prebuilt releases are not published on this environment"}
              </h2>
              <p className="text-sm leading-relaxed text-zinc-400">
                {configured
                  ? "Check another release channel above or build directly from source using Cargo."
                  : "Release storage is currently unconfigured or no tagged public builds have been published yet. You can build ClipForge directly from the open-source repository."}
              </p>
              <div className="flex flex-wrap gap-3 pt-1">
                <Link
                  href="/docs/development"
                  className="inline-flex items-center gap-1.5 rounded-lg bg-white/[0.08] px-4 py-2 text-xs font-medium text-white hover:bg-white/[0.14]"
                >
                  <Terminal className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>Build from Source</span>
                </Link>
                <a
                  href="https://github.com/ClipForge/ClipForge/releases"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1.5 rounded-lg border border-white/10 px-4 py-2 text-xs font-medium text-zinc-300 hover:bg-white/5"
                >
                  <span>GitHub Releases</span>
                </a>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
