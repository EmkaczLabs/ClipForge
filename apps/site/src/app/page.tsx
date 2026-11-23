import Link from "next/link";
import {
  ArrowRight,
  Download,
  HardDrive,
  Lock,
  Cpu,
  Terminal,
  Server,
} from "lucide-react";
import { FadeIn } from "@/components/MotionPrimitives";
import { ProductPreview } from "@/components/ProductPreview";

export default function HomePage() {
  return (
    <div className="bg-ambient-violet">
      {/* Hero */}
      <section className="mx-auto max-w-6xl px-4 pb-16 pt-20 sm:px-6 sm:pt-28">
        <FadeIn className="mx-auto max-w-3xl text-center">
          <div className="mb-6 inline-flex items-center gap-2 rounded-full border border-violet-500/30 bg-violet-500/10 px-3 py-1 text-xs font-medium text-violet-300">
            <span className="h-1.5 w-1.5 rounded-full bg-violet-400" />
            <span>Open-Source Desktop Capture &bull; Rust + Rune</span>
          </div>
          <h1 className="text-balance text-4xl font-semibold tracking-tight text-white sm:text-6xl">
            Capture the moment.
            <br />
            <span className="bg-gradient-to-r from-violet-300 via-violet-400 to-zinc-200 bg-clip-text text-transparent">
              Keep the memory.
            </span>
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-pretty text-base leading-relaxed text-zinc-400 sm:text-lg">
            ClipForge is a fast, local-first desktop recorder and Instant Replay
            tool built in Rust. Native GPU capture, multi-track audio, SQLite
            library management, and FFmpeg trimming—with zero accounts or
            telemetry.
          </p>

          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Link
              href="/download"
              className="inline-flex items-center gap-2 rounded-xl bg-violet-600 px-5 py-3 text-sm font-medium text-white shadow-[0_0_28px_rgba(139,92,246,0.35)] transition-colors hover:bg-violet-500"
            >
              <Download className="h-4 w-4" aria-hidden="true" />
              <span>Download ClipForge</span>
            </Link>
            <Link
              href="/docs"
              className="inline-flex items-center gap-2 rounded-xl border border-white/[0.12] bg-white/[0.03] px-5 py-3 text-sm font-medium text-zinc-200 transition-colors hover:bg-white/[0.07]"
            >
              <span>Read the Docs</span>
              <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </Link>
          </div>
        </FadeIn>

        <FadeIn delay={0.12} className="mt-14">
          <ProductPreview />
        </FadeIn>
      </section>

      {/* Local-first Philosophy */}
      <section className="border-t border-white/[0.07] py-20">
        <div className="mx-auto max-w-6xl px-4 sm:px-6">
          <div className="max-w-2xl">
            <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
              Philosophy
            </p>
            <h2 className="mt-2 text-2xl font-semibold tracking-tight text-white sm:text-3xl">
              Your clips belong on your machine.
            </h2>
            <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
              Modern clipping tools shouldn&apos;t force sign-ins, background
              cloud sync, or heavy browser runtimes. ClipForge runs natively and
              keeps your media local by default.
            </p>
          </div>

          <div className="mt-10 grid grid-cols-1 gap-4 md:grid-cols-3">
            {[
              {
                icon: HardDrive,
                title: "Local-First SQLite Library",
                body: "All recordings are indexed in a local rusqlite database with soft-delete trash protection (.clipforge_trash) and zero cloud lock-in.",
              },
              {
                icon: Cpu,
                title: "Native Rust & GPU Pipelines",
                body: "Powered by Rune UI, gpu-screen-recorder on Linux, Windows Graphics Capture (WGC) on Windows, and off-thread FFmpeg processing.",
              },
              {
                icon: Lock,
                title: "Zero Telemetry. Keyring Auth.",
                body: "No analytics, no tracking cookies, and no plaintext tokens. Optional server credentials are stored in your OS keyring via secret-tool.",
              },
            ].map((item) => {
              const Icon = item.icon;
              return (
                <div
                  key={item.title}
                  className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6"
                >
                  <div className="flex h-10 w-10 items-center justify-center rounded-xl border border-violet-500/25 bg-violet-500/10 text-violet-400">
                    <Icon className="h-5 w-5" aria-hidden="true" />
                  </div>
                  <h3 className="mt-4 text-base font-semibold text-zinc-100">
                    {item.title}
                  </h3>
                  <p className="mt-2 text-sm leading-relaxed text-zinc-400">
                    {item.body}
                  </p>
                </div>
              );
            })}
          </div>
        </div>
      </section>

      {/* Platform Availability & Optional Server */}
      <section className="border-t border-white/[0.07] py-20">
        <div className="mx-auto grid max-w-6xl grid-cols-1 gap-6 px-4 sm:px-6 lg:grid-cols-2">
          <div className="rounded-2xl border border-white/[0.08] bg-[#111114] p-7">
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-violet-400">
              <Terminal className="h-4 w-4" aria-hidden="true" />
              <span>Platform Availability</span>
            </div>
            <h3 className="mt-3 text-xl font-semibold text-white">
              Built for Linux x64 &amp; Windows x64
            </h3>
            <p className="mt-2 text-sm leading-relaxed text-zinc-400">
              Linux capture supervises <code className="text-zinc-200">gpu-screen-recorder</code>{" "}
              with Wayland XDG GlobalShortcuts and X11 fallback. Windows capture
              uses native WGC, WASAPI audio isolation, and portable ZIP
              packaging with bundled FFmpeg.
            </p>
            <div className="mt-6 flex flex-wrap gap-3">
              <Link
                href="/download"
                className="inline-flex items-center gap-1.5 rounded-lg bg-white/[0.08] px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-white/[0.14]"
              >
                <span>View Downloads</span>
                <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
              </Link>
              <Link
                href="/features"
                className="inline-flex items-center gap-1.5 rounded-lg px-4 py-2 text-sm font-medium text-zinc-400 transition-colors hover:text-zinc-200"
              >
                <span>Feature Matrix</span>
              </Link>
            </div>
          </div>

          <div className="rounded-2xl border border-white/[0.08] bg-[#111114] p-7">
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-violet-400">
              <Server className="h-4 w-4" aria-hidden="true" />
              <span>Optional Companion</span>
            </div>
            <h3 className="mt-3 text-xl font-semibold text-white">
              Self-Hostable ClipForge-Server
            </h3>
            <p className="mt-2 text-sm leading-relaxed text-zinc-400">
              Want to share clips with friends via clean links? Deploy{" "}
              <code className="text-zinc-200">ClipForge-Server</code> separately
              with Docker Compose, local or RustFS S3 storage, and Argon2id
              authentication. Completely optional.
            </p>
            <div className="mt-6 flex flex-wrap gap-3">
              <Link
                href="/server"
                className="inline-flex items-center gap-1.5 rounded-lg bg-white/[0.08] px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-white/[0.14]"
              >
                <span>Explore ClipForge-Server</span>
                <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
              </Link>
              <Link
                href="/open-source"
                className="inline-flex items-center gap-1.5 rounded-lg px-4 py-2 text-sm font-medium text-zinc-400 transition-colors hover:text-zinc-200"
              >
                <span>MIT License</span>
              </Link>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
