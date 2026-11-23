import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Features",
  description:
    "Feature matrix for ClipForge across Instant Replay, Recording, Clip Library, Export, Sharing, Performance, and Privacy.",
};

type FeatureStatus = "Available" | "In Development" | "Planned";

interface FeatureItem {
  name: string;
  status: FeatureStatus;
  summary: string;
}

interface FeatureCategory {
  category: string;
  description: string;
  items: FeatureItem[];
}

const CATEGORIES: FeatureCategory[] = [
  {
    category: "Instant Replay",
    description:
      "Capture rolling highlights without filling your drive with continuous recordings.",
    items: [
      {
        name: "Linux Rolling Replay Buffer (GSR)",
        status: "Available",
        summary:
          "Supervises gpu-screen-recorder as a managed child process and saves replays via SIGUSR1.",
      },
      {
        name: "Wayland & X11 Global Hotkeys",
        status: "Available",
        summary:
          "Uses XDG Desktop Portal GlobalShortcuts (zbus) with X11 fallback and in-app F9/F10/F11 bindings.",
      },
      {
        name: "Windows Segmented Replay Ring",
        status: "In Development",
        summary:
          "Bounded FFmpeg segment ring (-segment_wrap) with RegisterHotKey; implemented in crates/capture, pending live Windows hardware/CI verification.",
      },
    ],
  },
  {
    category: "Recording",
    description:
      "Native display and window capture with deterministic multi-track audio.",
    items: [
      {
        name: "Linux Screen & Window Capture",
        status: "Available",
        summary:
          "Hardware-accelerated capture via gpu-screen-recorder with automatic deduplication.",
      },
      {
        name: "Deterministic Audio Routing (Auto / T1–T4)",
        status: "Available",
        summary:
          "Mix into primary track 0:a:0 or isolate system and microphone streams into separate tracks (t2:–t4:).",
      },
      {
        name: "Windows WGC + WASAPI Capture",
        status: "In Development",
        summary:
          "Direct3D11/DXGI Windows Graphics Capture + COINIT_MULTITHREADED WASAPI loopback/mic; pending Windows runner hardware verification.",
      },
    ],
  },
  {
    category: "Clip Library",
    description:
      "Fast local indexing, filtering, and safe deletion powered by SQLite.",
    items: [
      {
        name: "SQLite v3 Clip Catalog",
        status: "Available",
        summary:
          "Bundled rusqlite database with ClipOrigin tracking (Captured, Imported, Sample) and DirectoryScanner.",
      },
      {
        name: "Soft-Delete Trash (.clipforge_trash)",
        status: "Available",
        summary:
          "Managed clips move to .clipforge_trash with restore metadata; imported and sample files are disk-protected.",
      },
      {
        name: "Inspector & Multi-Select Management",
        status: "Available",
        summary:
          "320px inspector panel, inline rename, context menu, and search/tag filtering.",
      },
    ],
  },
  {
    category: "Export",
    description:
      "Non-blocking FFmpeg media processing and multi-track playback.",
    items: [
      {
        name: "Fast Trim (-c copy) & Precise Trim",
        status: "Available",
        summary:
          "Keyframe-fast stream copy or frame-accurate re-encoding with ExportCancelToken cancellation.",
      },
      {
        name: "Off-UI-Thread Video & AutoMix Player",
        status: "Available",
        summary:
          "FFmpeg RGBA8 frame pipe paired with persistent-thread cpal PipeWire/ALSA/WASAPI audio playback.",
      },
      {
        name: "Custom Export Presets & GIF/WebM",
        status: "Planned",
        summary:
          "Additional target format presets and social-size compression profiles.",
      },
    ],
  },
  {
    category: "Sharing",
    description:
      "Optional self-hosted clip uploads via ClipForge-Server.",
    items: [
      {
        name: "Resumable Multipart Uploads (/api/v1)",
        status: "Available",
        summary:
          "HttpCloudClient uploads to an optional ClipForge-Server instance with progress tracking and share links.",
      },
      {
        name: "Web Share Viewer & OG/oEmbed Previews",
        status: "Available",
        summary:
          "Standalone ClipForge-Server serves rich OpenGraph/Twitter/oEmbed link previews and range-streamed playback.",
      },
    ],
  },
  {
    category: "Performance",
    description:
      "Lean native Rust binary without Electron or browser runtime overhead.",
    items: [
      {
        name: "Rune GPU-Accelerated Desktop UI",
        status: "Available",
        summary:
          "Obsidian Forge 5-screen interface built on anomalyco/rune with off-thread media & I/O.",
      },
      {
        name: "Hardware Video Encoders",
        status: "Available",
        summary:
          "NVENC, AMF, and QSV hardware encoding support with libx264 fallback.",
      },
      {
        name: "macOS ScreenCaptureKit Backend",
        status: "Planned",
        summary:
          "Future platform expansion after Linux and Windows 1.0 parity.",
      },
    ],
  },
  {
    category: "Privacy",
    description:
      "Local-first architecture with zero telemetry and strict credential hygiene.",
    items: [
      {
        name: "100% Offline Core Workflow",
        status: "Available",
        summary:
          "Capture, library indexing, playback, and trimming require no account or internet access.",
      },
      {
        name: "OS Keyring Token Storage",
        status: "Available",
        summary:
          "Optional server tokens are stored via secret-tool (org.freedesktop.secrets), never plaintext in config.json.",
      },
    ],
  },
];

function StatusBadge({ status }: { status: FeatureStatus }) {
  const styles: Record<FeatureStatus, string> = {
    Available:
      "border-emerald-500/30 bg-emerald-500/10 text-emerald-300",
    "In Development":
      "border-amber-500/30 bg-amber-500/10 text-amber-300",
    Planned:
      "border-zinc-700 bg-zinc-800/70 text-zinc-400",
  };

  return (
    <span
      className={`inline-flex items-center rounded-full border px-2.5 py-0.5 text-[11px] font-medium ${styles[status]}`}
    >
      {status}
    </span>
  );
}

export default function FeaturesPage() {
  return (
    <div className="mx-auto max-w-6xl px-4 py-16 sm:px-6">
      <div className="max-w-2xl">
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          Capabilities
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
          Features &amp; Implementation Status
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
          An honest breakdown of what is implemented and verified today, what is
          actively in development, and what is planned next.
        </p>
      </div>

      <div className="mt-12 space-y-10">
        {CATEGORIES.map((cat) => (
          <section
            key={cat.category}
            className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6 sm:p-8"
          >
            <div className="border-b border-white/[0.07] pb-4">
              <h2 className="text-xl font-semibold text-white">
                {cat.category}
              </h2>
              <p className="mt-1 text-sm text-zinc-400">{cat.description}</p>
            </div>

            <div className="mt-6 grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
              {cat.items.map((item) => (
                <div
                  key={item.name}
                  className="flex flex-col justify-between rounded-xl border border-white/[0.06] bg-[#0b0b0e] p-4"
                >
                  <div>
                    <div className="flex items-start justify-between gap-2">
                      <h3 className="text-sm font-semibold text-zinc-100">
                        {item.name}
                      </h3>
                      <StatusBadge status={item.status} />
                    </div>
                    <p className="mt-2 text-xs leading-relaxed text-zinc-400">
                      {item.summary}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
