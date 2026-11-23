import type { Metadata } from "next";
import { fetchReleaseIndex } from "@/lib/releases/service";

export const dynamic = "force-dynamic";

export const metadata: Metadata = {
  title: "Changelog",
  description: "Release history and engineering milestones for ClipForge.",
};

export default async function ChangelogPage() {
  const index = await fetchReleaseIndex();

  return (
    <div className="mx-auto max-w-4xl px-4 py-16 sm:px-6">
      <div className="max-w-2xl">
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          Release Notes
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
          Changelog
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
          Published release notes and workspace milestones.
        </p>
      </div>

      <div className="mt-12 space-y-6">
        {index.releases.map((rel) => (
          <article
            key={`${rel.version}-${rel.channel}`}
            className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6"
          >
            <div className="flex flex-wrap items-center gap-2.5">
              <span className="rounded-full border border-violet-500/30 bg-violet-500/15 px-2.5 py-0.5 font-mono text-xs text-violet-300">
                {rel.version}
              </span>
              <span className="rounded-full border border-white/10 bg-white/5 px-2.5 py-0.5 text-xs capitalize text-zinc-300">
                {rel.channel}
              </span>
              <time
                dateTime={rel.publishedAt}
                className="text-xs text-zinc-500"
              >
                {new Date(rel.publishedAt).toLocaleDateString("en-US", {
                  year: "numeric",
                  month: "short",
                  day: "numeric",
                })}
              </time>
            </div>
            <p className="mt-4 whitespace-pre-line text-sm leading-relaxed text-zinc-300">
              {rel.changelog}
            </p>
          </article>
        ))}

        {/* Verified Repository Foundation Entry */}
        <article className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6">
          <div className="flex flex-wrap items-center gap-2.5">
            <span className="rounded-full border border-violet-500/30 bg-violet-500/15 px-2.5 py-0.5 font-mono text-xs text-violet-300">
              v0.1.0-dev
            </span>
            <span className="rounded-full border border-white/10 bg-white/5 px-2.5 py-0.5 text-xs text-zinc-300">
              Workspace Foundation
            </span>
          </div>
          <h2 className="mt-3 text-lg font-semibold text-white">
            Obsidian Forge Desktop, Capture Pipelines &amp; Standalone Server
          </h2>
          <ul className="mt-3 list-disc space-y-1.5 pl-5 text-sm text-zinc-400">
            <li>
              5-screen Rune desktop UI (Library, Editor, Recording, Uploads,
              Settings) with off-UI-thread FFmpeg + cpal AutoMix playback.
            </li>
            <li>
              Linux gpu-screen-recorder supervision, deterministic Auto/T1–T4
              audio routing, and XDG GlobalShortcuts + X11 hotkey fallback.
            </li>
            <li>
              Windows WGC + WASAPI capture modules and portable ZIP packaging
              pipeline implemented in crates/capture and packaging/windows.
            </li>
            <li>
              SQLite v3 clip catalog with ClipOrigin tracking and
              .clipforge_trash recovery.
            </li>
            <li>
              Optional ClipForge-Server separated into its own 5-crate
              workspace with RustFS S3 and Argon2id auth.
            </li>
          </ul>
        </article>
      </div>
    </div>
  );
}
