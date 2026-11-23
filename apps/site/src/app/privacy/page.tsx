import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Privacy",
  description:
    "ClipForge privacy policy: zero telemetry, local-first desktop storage, and no tracking cookies.",
};

export default function PrivacyPage() {
  return (
    <div className="mx-auto max-w-3xl px-4 py-16 sm:px-6">
      <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
        Privacy Policy
      </p>
      <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
        Privacy by Architecture
      </h1>
      <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
        ClipForge is designed around a simple rule: your recordings and usage
        stay on your hardware.
      </p>

      <div className="mt-10 space-y-6">
        {[
          {
            title: "ClipForge Desktop Application",
            body: "The desktop app collects zero telemetry, crash analytics, or usage metrics. Video recordings, thumbnails, and your SQLite clip library are stored strictly on your local disk. Optional ClipForge-Server authentication tokens are stored in your system keyring (secret-tool / org.freedesktop.secrets) and are never written in plaintext to config.json.",
          },
          {
            title: "This Website",
            body: "This website does not use advertising trackers, third-party analytics scripts, or tracking cookies. API requests to /api/releases and /api/download/[releaseId] exist solely to serve verified release metadata and public release artifacts.",
          },
          {
            title: "Optional Self-Hosted ClipForge-Server",
            body: "If you choose to connect the desktop app to your own or another ClipForge-Server instance, uploaded clips and account credentials are governed by whoever operates that server instance.",
          },
        ].map((section) => (
          <section
            key={section.title}
            className="rounded-2xl border border-white/[0.08] bg-[#111114] p-6"
          >
            <h2 className="text-base font-semibold text-white">
              {section.title}
            </h2>
            <p className="mt-2 text-sm leading-relaxed text-zinc-400">
              {section.body}
            </p>
          </section>
        ))}
      </div>
    </div>
  );
}
