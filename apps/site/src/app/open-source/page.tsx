import type { Metadata } from "next";
import Link from "next/link";
import { Code2, ExternalLink, Scale } from "lucide-react";

export const metadata: Metadata = {
  title: "Open Source",
  description:
    "ClipForge is open-source software under the MIT License. Explore repositories, third-party notices, and how to contribute.",
};

export default function OpenSourcePage() {
  return (
    <div className="mx-auto max-w-4xl px-4 py-16 sm:px-6">
      <div className="max-w-2xl">
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          MIT Licensed
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
          Open Source
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
          ClipForge is developed in the open under the permissive{" "}
          <strong className="text-zinc-200">MIT License</strong>. You can
          inspect every line of capture, media, library, and server code, or
          build everything from source.
        </p>
      </div>

      <div className="mt-10 grid grid-cols-1 gap-4 sm:grid-cols-2">
        <a
          href="https://github.com/ClipForge/ClipForge"
          target="_blank"
          rel="noopener noreferrer"
          className="group rounded-2xl border border-white/[0.08] bg-[#111114] p-6 transition-colors hover:border-violet-500/40"
        >
          <div className="flex items-center justify-between">
            <Code2 className="h-5 w-5 text-violet-400" aria-hidden="true" />
            <ExternalLink
              className="h-4 w-4 text-zinc-500 group-hover:text-zinc-300"
              aria-hidden="true"
            />
          </div>
          <h2 className="mt-4 text-base font-semibold text-white">
            ClipForge Desktop
          </h2>
          <p className="mt-2 text-xs leading-relaxed text-zinc-400">
            Rust workspace containing apps/desktop (Rune UI), crates/capture,
            crates/media, crates/library, crates/cloud-client, and apps/site.
          </p>
        </a>

        <a
          href="https://github.com/EmkaczLabs/ClipForge-Server"
          target="_blank"
          rel="noopener noreferrer"
          className="group rounded-2xl border border-white/[0.08] bg-[#111114] p-6 transition-colors hover:border-violet-500/40"
        >
          <div className="flex items-center justify-between">
            <Code2 className="h-5 w-5 text-violet-400" aria-hidden="true" />
            <ExternalLink
              className="h-4 w-4 text-zinc-500 group-hover:text-zinc-300"
              aria-hidden="true"
            />
          </div>
          <h2 className="mt-4 text-base font-semibold text-white">
            ClipForge-Server
          </h2>
          <p className="mt-2 text-xs leading-relaxed text-zinc-400">
            Standalone 5-crate Axum server, Docker Compose setup, and web share
            player.
          </p>
        </a>
      </div>

      <div className="mt-8 rounded-2xl border border-white/[0.08] bg-[#111114] p-6 sm:p-8">
        <div className="flex items-center gap-2 text-white">
          <Scale className="h-5 w-5 text-violet-400" aria-hidden="true" />
          <h2 className="text-lg font-semibold">Core Open-Source Ecosystem</h2>
        </div>
        <ul className="mt-4 list-disc space-y-2 pl-5 text-sm text-zinc-400">
          <li>
            <strong className="text-zinc-200">Rune</strong> (anomalyco/rune) —
            GPU-accelerated Rust UI framework powering Obsidian Forge.
          </li>
          <li>
            <strong className="text-zinc-200">gpu-screen-recorder</strong> —
            Low-overhead Linux screen and replay capture engine.
          </li>
          <li>
            <strong className="text-zinc-200">FFmpeg / FFprobe</strong> —
            Media decoding, probing, trimming, and Windows encoding muxing.
          </li>
          <li>
            <strong className="text-zinc-200">SQLite (rusqlite)</strong> —
            Local-first embedded relational clip catalog.
          </li>
        </ul>
        <div className="mt-6">
          <Link
            href="/docs/contributing"
            className="text-sm font-medium text-violet-400 hover:text-violet-300"
          >
            Read the Contributing Guide &rarr;
          </Link>
        </div>
      </div>
    </div>
  );
}
