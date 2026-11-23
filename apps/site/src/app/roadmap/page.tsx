import type { Metadata } from "next";
import { CheckCircle2, Clock, CircleDot } from "lucide-react";

export const metadata: Metadata = {
  title: "Roadmap",
  description:
    "Development milestones and upcoming engineering work for ClipForge.",
};

interface Milestone {
  phase: string;
  title: string;
  status: "Completed" | "In Progress" | "Planned";
  items: string[];
}

const MILESTONES: Milestone[] = [
  {
    phase: "Milestone 1",
    title: "Workspace, Common Contracts & Obsidian Forge UI",
    status: "Completed",
    items: [
      "Cargo workspace skeleton (apps/desktop, crates/common, capture, media, library, cloud-client)",
      "Obsidian Forge 5-screen Rune UI (Library, Editor, Recording, Uploads, Settings)",
      "AppConfig persistence and strict secret-tool keyring token storage",
    ],
  },
  {
    phase: "Milestone 2",
    title: "Capture & Media Processing Pipeline",
    status: "Completed",
    items: [
      "Linux gpu-screen-recorder lifecycle, SIGUSR1 replay save, and Auto/T1–T4 audio routing",
      "Off-UI-thread FFmpeg RGBA8 video pipe + cpal AutoMix multi-track audio playback",
      "Fast Trim (-c copy) and Precise Trim with ExportCancelToken",
    ],
  },
  {
    phase: "Milestone 3",
    title: "Local SQLite Library & Optional Cloud Sharing",
    status: "Completed",
    items: [
      "SQLite v3 clip catalog, DirectoryScanner, and .clipforge_trash soft-delete protection",
      "Standalone ClipForge-Server (/api/v1, Argon2id auth, Local + RustFS S3, OG/oEmbed web player)",
    ],
  },
  {
    phase: "Milestone 4",
    title: "Windows Native Capture Verification & Public Releases",
    status: "In Progress",
    items: [
      "Windows Graphics Capture (WGC), WASAPI audio isolation, and RegisterHotKey verification on physical Windows 10/11 hardware",
      "Automated portable Windows ZIP & Linux release publishing to RustFS",
      "In-app desktop update checks via /api/updates/latest",
    ],
  },
  {
    phase: "Future",
    title: "Platform Expansion & Export Presets",
    status: "Planned",
    items: [
      "Additional export presets (GIF, WebM, target file size profiles)",
      "macOS ScreenCaptureKit capture backend exploration",
    ],
  },
];

export default function RoadmapPage() {
  return (
    <div className="mx-auto max-w-4xl px-4 py-16 sm:px-6">
      <div className="max-w-2xl">
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          Direction
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
          Project Roadmap
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
          Current milestone progress across the ClipForge desktop client and
          capture backends.
        </p>
      </div>

      <div className="mt-12 space-y-6">
        {MILESTONES.map((m) => {
          const Icon =
            m.status === "Completed"
              ? CheckCircle2
              : m.status === "In Progress"
              ? Clock
              : CircleDot;
          const iconColor =
            m.status === "Completed"
              ? "text-emerald-400"
              : m.status === "In Progress"
              ? "text-amber-400"
              : "text-zinc-500";

          return (
            <div
              key={m.phase}
              className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2.5">
                  <Icon className={`h-5 w-5 ${iconColor}`} aria-hidden="true" />
                  <span className="font-mono text-xs text-violet-400">
                    {m.phase}
                  </span>
                </div>
                <span className="rounded-full border border-white/10 bg-white/5 px-2.5 py-0.5 text-xs text-zinc-300">
                  {m.status}
                </span>
              </div>
              <h2 className="mt-3 text-lg font-semibold text-white">
                {m.title}
              </h2>
              <ul className="mt-3 list-disc space-y-1.5 pl-5 text-sm text-zinc-400">
                {m.items.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
          );
        })}
      </div>
    </div>
  );
}
