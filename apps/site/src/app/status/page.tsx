import type { Metadata } from "next";
import { Activity, CheckCircle2, AlertTriangle, Info } from "lucide-react";
import { checkStorageHealth } from "@/lib/releases/service";

export const dynamic = "force-dynamic";

export const metadata: Metadata = {
  title: "Status",
  description: "Public service health status for the ClipForge website and release delivery.",
};

export default async function StatusPage() {
  const storage = await checkStorageHealth();
  const isDegraded = storage.configured && storage.reachable === false;

  return (
    <div className="mx-auto max-w-3xl px-4 py-16 sm:px-6">
      <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-widest text-violet-400">
        <Activity className="h-4 w-4" aria-hidden="true" />
        <span>System Status</span>
      </div>
      <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
        Service Health
      </h1>
      <p className="mt-3 text-sm text-zinc-400">
        Live operational status for the ClipForge web application and public
        release distribution endpoints.
      </p>

      <div className="mt-10 space-y-4">
        <div className="flex items-center justify-between rounded-2xl border border-white/[0.08] bg-[#111114] p-5">
          <div>
            <h2 className="text-sm font-semibold text-white">
              Web Application &amp; API
            </h2>
            <p className="mt-0.5 text-xs text-zinc-400">
              Pages, documentation, and release metadata endpoints
            </p>
          </div>
          <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-3 py-1 text-xs font-medium text-emerald-300">
            <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />
            Operational
          </span>
        </div>

        <div className="flex items-center justify-between rounded-2xl border border-white/[0.08] bg-[#111114] p-5">
          <div>
            <h2 className="text-sm font-semibold text-white">
              Release Artifact Storage
            </h2>
            <p className="mt-0.5 text-xs text-zinc-400">
              Public release index and binary download delivery
            </p>
          </div>
          {!storage.configured ? (
            <span className="inline-flex items-center gap-1.5 rounded-full border border-zinc-700 bg-zinc-800/80 px-3 py-1 text-xs font-medium text-zinc-300">
              <Info className="h-3.5 w-3.5" aria-hidden="true" />
              Unconfigured
            </span>
          ) : isDegraded ? (
            <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/30 bg-amber-500/10 px-3 py-1 text-xs font-medium text-amber-300">
              <AlertTriangle className="h-3.5 w-3.5" aria-hidden="true" />
              Degraded
            </span>
          ) : (
            <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-3 py-1 text-xs font-medium text-emerald-300">
              <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />
              Operational
            </span>
          )}
        </div>
      </div>

      <p className="mt-6 font-mono text-xs text-zinc-500">
        Health endpoint: <code className="text-zinc-300">GET /api/health</code>
      </p>
    </div>
  );
}
