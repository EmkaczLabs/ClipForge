import type { Metadata } from "next";
import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { getAllDocsMeta } from "@/lib/docs";

export const metadata: Metadata = {
  title: "Documentation",
  description:
    "Documentation for ClipForge: installation, recording, instant replay, clip management, troubleshooting, and development.",
};

export default function DocsIndexPage() {
  const docs = getAllDocsMeta();

  return (
    <div className="space-y-8">
      <div>
        <p className="text-xs font-semibold uppercase tracking-widest text-violet-400">
          Documentation
        </p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white">
          ClipForge Docs
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-zinc-400 sm:text-base">
          Guides for installing, configuring, and developing ClipForge on Linux
          and Windows.
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {docs.map((doc) => (
          <Link
            key={doc.slug}
            href={`/docs/${doc.slug}`}
            className="group rounded-2xl border border-white/[0.08] bg-[#111114] p-5 transition-colors hover:border-violet-500/40"
          >
            <span className="text-[11px] font-medium uppercase tracking-wider text-violet-400">
              {doc.section}
            </span>
            <div className="mt-1.5 flex items-center justify-between gap-2">
              <h2 className="text-base font-semibold text-zinc-100 group-hover:text-white">
                {doc.title}
              </h2>
              <ArrowRight
                className="h-4 w-4 text-zinc-500 transition-transform group-hover:translate-x-0.5 group-hover:text-violet-400"
                aria-hidden="true"
              />
            </div>
            <p className="mt-2 text-xs leading-relaxed text-zinc-400">
              {doc.description}
            </p>
          </Link>
        ))}
      </div>
    </div>
  );
}
