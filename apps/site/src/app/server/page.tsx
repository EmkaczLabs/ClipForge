import type { Metadata } from "next";
import { Server, Database, Shield, Globe, ExternalLink } from "lucide-react";

export const metadata: Metadata = {
  title: "ClipForge-Server",
  description:
    "Optional, separately deployable self-hosted sharing server for ClipForge.",
};

export default function ServerPage() {
  return (
    <div className="mx-auto max-w-5xl px-4 py-16 sm:px-6">
      <div className="max-w-2xl">
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          Optional Companion
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
          ClipForge-Server
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
          <strong className="text-zinc-200">ClipForge-Server</strong> is an
          optional, separately deployable Rust backend for users who want to
          upload clips from the desktop app and share them via clean web links.
          ClipForge Desktop works 100% offline without it.
        </p>

        <div className="mt-6 flex flex-wrap gap-3">
          <a
            href="https://github.com/EmkaczLabs/ClipForge-Server"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 rounded-xl bg-violet-600 px-5 py-2.5 text-sm font-medium text-white shadow-[0_0_24px_rgba(139,92,246,0.3)] transition-colors hover:bg-violet-500"
          >
            <span>View ClipForge-Server on GitHub</span>
            <ExternalLink className="h-4 w-4" aria-hidden="true" />
          </a>
        </div>
      </div>

      <div className="mt-12 grid grid-cols-1 gap-4 md:grid-cols-2">
        {[
          {
            icon: Server,
            title: "5-Crate Modular Rust Workspace",
            body: "Built with Axum (/api/v1), OpenAPI 3.1.0 contracts (clipforge-api), SQLite metadata (clipforge-db), and zero dependencies on the desktop repository.",
          },
          {
            icon: Database,
            title: "Local Disk or RustFS S3 Storage",
            body: "Store uploaded clips on a local filesystem volume or an S3-compatible object store such as RustFS using SigV4.",
          },
          {
            icon: Shield,
            title: "Argon2id Auth & Keyring Tokens",
            body: "Passwords hashed with Argon2id, SHA-256 bearer tokens, and desktop credentials stored safely in your OS keyring (secret-tool).",
          },
          {
            icon: Globe,
            title: "Web Player & Link Previews",
            body: "Includes a standalone web viewer with HTTP byte-range streaming and OpenGraph, Twitter, and oEmbed metadata for Discord and Slack previews.",
          },
        ].map((card) => {
          const Icon = card.icon;
          return (
            <div
              key={card.title}
              className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6"
            >
              <div className="flex h-10 w-10 items-center justify-center rounded-xl border border-violet-500/25 bg-violet-500/10 text-violet-400">
                <Icon className="h-5 w-5" aria-hidden="true" />
              </div>
              <h2 className="mt-4 text-base font-semibold text-white">
                {card.title}
              </h2>
              <p className="mt-2 text-sm leading-relaxed text-zinc-400">
                {card.body}
              </p>
            </div>
          );
        })}
      </div>

      <div className="mt-10 rounded-2xl border border-white/[0.08] bg-[#111114] p-6 sm:p-8">
        <h2 className="text-lg font-semibold text-white">
          Deploy Separately with Docker Compose
        </h2>
        <p className="mt-2 text-sm text-zinc-400">
          ClipForge-Server ships with a{" "}
          <code className="font-mono text-zinc-200">Dockerfile</code> and{" "}
          <code className="font-mono text-zinc-200">compose.yaml</code> so you
          can run it on your own VPS, homelab, or LXC container:
        </p>
        <pre className="mt-4 overflow-x-auto rounded-xl border border-white/[0.08] bg-[#09090b] p-4 font-mono text-xs text-zinc-300">
          {`git clone https://github.com/EmkaczLabs/ClipForge-Server.git
cd ClipForge-Server
cp .env.example .env
docker compose up -d`}
        </pre>
      </div>
    </div>
  );
}
